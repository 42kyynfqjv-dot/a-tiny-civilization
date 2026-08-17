use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use reqwest::{
    Client, Response, StatusCode, Url,
    header::{ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, RANGE},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const MANIFEST_ID: &str = "aacr-gbm5k-dependency-source-v1";
const MANIFEST_SHA256: &str = "567a12c7e76945f231c8470781bd370f67be1ba662f358bc56b0c110c0eff726";
const ARTICLE_ID: u64 = 28_183_566;
const ARTICLE_VERSION: u16 = 1;
const TITLE: &str =
    "Table S4 from Fitness Screens Map State-Specific Glioblastoma Stem Cell Vulnerabilities";
const DATASET_DOI: &str = "10.1158/0008-5472.28183566";
const RESOURCE_DOI: &str = "10.1158/0008-5472.CAN-23-4024";
const ARTICLE_API_URL: &str = "https://api.figshare.com/v2/articles/28183566/versions/1";
const ARTICLE_API_FILE: &str = "article-v1-api-response.json";
const SNAPSHOT_FILE: &str = "source-snapshot.json";
const SOURCE_SET_DOMAIN: &str = "a-tiny-civilization/aacr-gbm5k/source-set/v1";
const MAX_API_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
struct SourceManifest {
    schema_version: u16,
    manifest_id: String,
    source: SourceIdentity,
    license: LicenseIdentity,
    artifact: ArtifactPolicy,
    evidence: EvidenceBoundary,
    leakage_boundary: LeakageBoundary,
    limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct SourceIdentity {
    custodian: String,
    title: String,
    article_id: u64,
    article_version: u16,
    article_api_url: String,
    landing_url: String,
    dataset_doi: String,
    dataset_doi_url: String,
    related_article_title: String,
    related_article_doi: String,
    related_article_pubmed_url: String,
    reviewed_on: String,
}

#[derive(Clone, Debug, Deserialize)]
struct LicenseIdentity {
    spdx_identifier: String,
    license_name: String,
    license_url: String,
    metadata_evidence_url: String,
    attribution: String,
    commercial_reuse_boundary: String,
}

#[derive(Clone, Debug, Deserialize)]
struct ArtifactPolicy {
    description: String,
    expected_file_count: usize,
    allowed_media_types: Vec<String>,
    allowed_file_extensions: Vec<String>,
    minimum_byte_length: u64,
    maximum_byte_length: u64,
    identity_resolution: String,
    checksum_policy: String,
}

#[derive(Clone, Debug, Deserialize)]
struct EvidenceBoundary {
    assay: String,
    reported_gsc_culture_count: usize,
    reported_material: String,
    allowed_claim: String,
    prohibited_claim: String,
}

#[derive(Clone, Debug, Deserialize)]
struct LeakageBoundary {
    access_class: String,
    research_prompt_access: bool,
    research_memory_access: bool,
    campaign_selection_access: bool,
    observer_projection_access: String,
    split_policy: String,
    identifier_policy: String,
}

#[derive(Clone, Debug, Deserialize)]
struct FigshareArticle {
    id: u64,
    title: String,
    doi: String,
    resource_doi: String,
    version: u16,
    is_public: bool,
    is_confidential: bool,
    status: String,
    defined_type_name: String,
    license: FigshareLicense,
    files: Vec<FigshareFile>,
}

#[derive(Clone, Debug, Deserialize)]
struct FigshareLicense {
    name: String,
    url: String,
}

#[derive(Clone, Debug, Deserialize)]
struct FigshareFile {
    id: u64,
    name: String,
    size: u64,
    is_link_only: bool,
    download_url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct DiscoveredFile {
    file_id: u64,
    file_name: String,
    byte_length: u64,
    download_url: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SourceSnapshot {
    schema_version: u16,
    snapshot_id: String,
    manifest_id: String,
    manifest_sha256: String,
    article_id: u64,
    article_version: u16,
    article_api_response: SnapshotArtifact,
    source_file: SnapshotSourceFile,
    source_set_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SnapshotArtifact {
    file_name: String,
    byte_length: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SnapshotSourceFile {
    file_id: u64,
    file_name: String,
    byte_length: u64,
    sha256: String,
}

struct FileDigests {
    byte_length: u64,
    sha256: String,
}

pub async fn acquire(manifest_path: &Path, output_directory: &Path) -> Result<()> {
    let (manifest, manifest_bytes) = read_manifest(manifest_path)?;
    let manifest_sha256 = sha256_bytes(&manifest_bytes);
    prepare_output_directory(output_directory)?;
    let snapshot_path = output_directory.join(SNAPSHOT_FILE);
    if snapshot_path.exists() {
        let snapshot = verify_source_directory(&manifest, &manifest_sha256, output_directory)?;
        println!(
            "verified AACR GBM5K dependency snapshot {} (file {})",
            snapshot.source_set_sha256, snapshot.source_file.file_id
        );
        return Ok(());
    }

    let client = Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::limited(5))
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(120))
        .user_agent("a-tiny-civilization-aacr-gbm5k-acquisition/0.1")
        .build()
        .context("construct AACR Figshare client")?;
    let api_path = output_directory.join(ARTICLE_API_FILE);
    let (discovered, api_bytes) = if api_path.exists() {
        read_and_validate_api_response(&api_path, &manifest)?
    } else {
        let bytes = get_limited_exact(&client, ARTICLE_API_URL, MAX_API_BYTES).await?;
        let discovered = parse_and_validate_api_response(&bytes, &manifest)?;
        write_new(&api_path, &bytes)?;
        (discovered, bytes)
    };
    acquire_file(&client, &discovered, output_directory).await?;
    let snapshot = build_snapshot(&manifest_sha256, &api_bytes, &discovered, output_directory)?;
    write_new(&snapshot_path, &pretty_json_bytes(&snapshot)?)?;
    println!(
        "acquired AACR GBM5K dependency source; snapshot {}",
        snapshot.source_set_sha256
    );
    Ok(())
}

fn read_manifest(path: &Path) -> Result<(SourceManifest, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if sha256_bytes(&bytes) != MANIFEST_SHA256 {
        bail!("AACR GBM5K trust manifest bytes changed without a method revision");
    }
    let manifest: SourceManifest =
        serde_json::from_slice(&bytes).with_context(|| format!("decode {}", path.display()))?;
    validate_manifest(&manifest)?;
    Ok((manifest, bytes))
}

fn validate_manifest(manifest: &SourceManifest) -> Result<()> {
    let source = &manifest.source;
    let license = &manifest.license;
    let policy = &manifest.artifact;
    let evidence = &manifest.evidence;
    let leakage = &manifest.leakage_boundary;
    if manifest.schema_version != 1
        || manifest.manifest_id != MANIFEST_ID
        || source.title != TITLE
        || source.article_id != ARTICLE_ID
        || source.article_version != ARTICLE_VERSION
        || source.article_api_url != ARTICLE_API_URL
        || source.landing_url
            != "https://aacr.figshare.com/articles/dataset/Table_S4_from_Fitness_Screens_Map_State-Specific_Glioblastoma_Stem_Cell_Vulnerabilities/28183566/1"
        || source.dataset_doi != DATASET_DOI
        || source.dataset_doi_url != format!("https://doi.org/{DATASET_DOI}")
        || source.related_article_doi != RESOURCE_DOI
        || source.related_article_pubmed_url != "https://pubmed.ncbi.nlm.nih.gov/39186687/"
        || source.reviewed_on != "2026-08-13"
        || license.spdx_identifier != "CC-BY-4.0"
        || license.license_name != "Creative Commons Attribution 4.0 International"
        || license.license_url != "https://creativecommons.org/licenses/by/4.0/"
        || policy.expected_file_count != 1
        || policy.allowed_file_extensions != ["xlsx"]
        || !policy.allowed_media_types.iter().any(|value| {
            value == "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        })
        || policy.minimum_byte_length < 1_024
        || policy.maximum_byte_length > 16 * 1024 * 1024
        || policy.maximum_byte_length <= policy.minimum_byte_length
        || evidence.reported_gsc_culture_count != 30
        || evidence.reported_material != "patient-derived glioblastoma stem cell cultures"
        || leakage.access_class != "qualification_worker_only"
        || leakage.research_prompt_access
        || leakage.research_memory_access
        || leakage.campaign_selection_access
        || manifest.limitations.len() != 5
    {
        bail!("unsupported or inconsistent AACR GBM5K source manifest");
    }
    for (value, label) in [
        (&source.custodian, "custodian"),
        (&source.related_article_title, "related article title"),
        (&license.metadata_evidence_url, "license evidence URL"),
        (&license.attribution, "attribution"),
        (
            &license.commercial_reuse_boundary,
            "commercial reuse boundary",
        ),
        (&policy.description, "artifact description"),
        (&policy.identity_resolution, "identity policy"),
        (&policy.checksum_policy, "checksum policy"),
        (&evidence.assay, "assay"),
        (&evidence.allowed_claim, "allowed claim"),
        (&evidence.prohibited_claim, "prohibited claim"),
        (&leakage.observer_projection_access, "observer boundary"),
        (&leakage.split_policy, "split policy"),
        (&leakage.identifier_policy, "identifier policy"),
    ] {
        if value.trim().is_empty() {
            bail!("AACR GBM5K manifest has an empty {label}");
        }
    }
    for value in [
        &source.article_api_url,
        &source.landing_url,
        &source.dataset_doi_url,
        &source.related_article_pubmed_url,
        &license.license_url,
        &license.metadata_evidence_url,
    ] {
        require_https(value, "manifest URL")?;
    }
    if manifest
        .limitations
        .iter()
        .any(|value| value.trim().is_empty())
    {
        bail!("AACR GBM5K manifest contains an empty limitation");
    }
    Ok(())
}

fn parse_and_validate_api_response(
    bytes: &[u8],
    manifest: &SourceManifest,
) -> Result<DiscoveredFile> {
    if bytes.is_empty() || u64::try_from(bytes.len())? > MAX_API_BYTES {
        bail!("AACR Figshare article response is empty or oversized");
    }
    let article: FigshareArticle =
        serde_json::from_slice(bytes).context("decode AACR Figshare article metadata")?;
    validate_article(&article, manifest)
}

fn validate_article(
    article: &FigshareArticle,
    manifest: &SourceManifest,
) -> Result<DiscoveredFile> {
    if article.id != ARTICLE_ID
        || article.version != ARTICLE_VERSION
        || article.title != TITLE
        || article.doi != DATASET_DOI
        || !article.resource_doi.eq_ignore_ascii_case(RESOURCE_DOI)
        || !article.is_public
        || article.is_confidential
        || article.status != "public"
        || article.defined_type_name != "dataset"
        || article.license.url != manifest.license.license_url
        || !matches!(article.license.name.as_str(), "CC BY" | "CC BY 4.0")
        || article.files.len() != manifest.artifact.expected_file_count
    {
        bail!("AACR Figshare article identity, license, or version changed");
    }
    let file = article
        .files
        .first()
        .context("AACR Figshare article omitted its one declared file")?;
    if file.id == 0
        || file.is_link_only
        || !is_safe_xlsx_name(&file.name)
        || file.size < manifest.artifact.minimum_byte_length
        || file.size > manifest.artifact.maximum_byte_length
    {
        bail!("AACR Figshare file identity metadata is invalid");
    }
    require_figshare_download_url(&file.download_url, file.id)?;
    Ok(DiscoveredFile {
        file_id: file.id,
        file_name: file.name.clone(),
        byte_length: file.size,
        download_url: file.download_url.clone(),
    })
}

fn read_and_validate_api_response(
    path: &Path,
    manifest: &SourceManifest,
) -> Result<(DiscoveredFile, Vec<u8>)> {
    reject_symlink(path)?;
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let discovered = parse_and_validate_api_response(&bytes, manifest)?;
    Ok((discovered, bytes))
}

async fn acquire_file(client: &Client, source: &DiscoveredFile, directory: &Path) -> Result<()> {
    let final_path = directory.join(&source.file_name);
    if final_path.exists() {
        verify_file(&final_path, source)?;
        return Ok(());
    }
    let part_path = directory.join(format!("{}.part", source.file_name));
    let part_exists = part_path.exists();
    let offset = if part_exists {
        reject_symlink(&part_path)?;
        fs::metadata(&part_path)
            .with_context(|| format!("inspect {}", part_path.display()))?
            .len()
    } else {
        0
    };
    if offset > source.byte_length {
        bail!("partial AACR GBM5K file exceeds the exact source length");
    }
    if offset < source.byte_length {
        download_remaining(client, source, &part_path, offset, part_exists).await?;
    }
    verify_file(&part_path, source)?;
    fs::hard_link(&part_path, &final_path).with_context(|| {
        format!(
            "publish create-only AACR GBM5K file {}",
            final_path.display()
        )
    })?;
    fs::remove_file(&part_path)
        .with_context(|| format!("remove completed partial file {}", part_path.display()))?;
    Ok(())
}

async fn download_remaining(
    client: &Client,
    source: &DiscoveredFile,
    part_path: &Path,
    offset: u64,
    part_exists: bool,
) -> Result<()> {
    require_figshare_download_url(&source.download_url, source.file_id)?;
    let mut request = client
        .get(&source.download_url)
        .header(ACCEPT_ENCODING, "identity");
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    let mut response = request.send().await.context("download AACR GBM5K file")?;
    validate_download_response(&response, source, offset)?;
    let mut file = open_partial_file(part_path, offset, part_exists)?;
    let mut written = offset;
    while let Some(chunk) = response.chunk().await.context("stream AACR GBM5K file")? {
        written = written
            .checked_add(u64::try_from(chunk.len())?)
            .context("AACR GBM5K file length overflow")?;
        if written > source.byte_length {
            bail!("AACR GBM5K download exceeded its exact byte length");
        }
        file.write_all(&chunk)?;
    }
    file.sync_all()?;
    if written != source.byte_length {
        bail!(
            "AACR GBM5K download ended at {written}, expected {} bytes",
            source.byte_length
        );
    }
    Ok(())
}

fn open_partial_file(path: &Path, offset: u64, already_exists: bool) -> Result<File> {
    if already_exists {
        let actual = fs::metadata(path)
            .with_context(|| format!("inspect partial file {}", path.display()))?
            .len();
        if actual != offset {
            bail!("AACR GBM5K partial file changed after its resume offset was frozen");
        }
        OpenOptions::new().append(true).open(path)
    } else if offset == 0 {
        OpenOptions::new().create_new(true).write(true).open(path)
    } else {
        bail!("AACR GBM5K nonzero resume offset has no partial file");
    }
    .with_context(|| format!("open partial file {}", path.display()))
}

fn validate_download_response(
    response: &Response,
    source: &DiscoveredFile,
    offset: u64,
) -> Result<()> {
    if response.url().scheme() != "https" {
        bail!("AACR GBM5K download redirected away from HTTPS");
    }
    let remaining = source
        .byte_length
        .checked_sub(offset)
        .context("AACR GBM5K download offset exceeds its exact length")?;
    if response.content_length() != Some(remaining) {
        bail!("AACR GBM5K response omitted or changed its exact remaining length");
    }
    if offset == 0 {
        if response.status() != StatusCode::OK || response.headers().contains_key(CONTENT_RANGE) {
            bail!("fresh AACR GBM5K response was not one complete 200 response");
        }
    } else {
        if response.status() != StatusCode::PARTIAL_CONTENT {
            bail!("resumed AACR GBM5K response did not honor the byte range");
        }
        let range = response
            .headers()
            .get(CONTENT_RANGE)
            .context("resumed AACR GBM5K response omitted Content-Range")?
            .to_str()?;
        validate_content_range(range, offset, source.byte_length)?;
    }
    if let Some(value) = response.headers().get(CONTENT_LENGTH)
        && value.to_str()?.parse::<u64>()? != remaining
    {
        bail!("AACR GBM5K Content-Length differs from the required tail");
    }
    Ok(())
}

fn validate_content_range(value: &str, start: u64, total: u64) -> Result<()> {
    let body = value
        .strip_prefix("bytes ")
        .context("AACR GBM5K Content-Range must use bytes")?;
    let (range, reported_total) = body
        .split_once('/')
        .context("AACR GBM5K Content-Range omitted its total")?;
    let (reported_start, reported_end) = range
        .split_once('-')
        .context("AACR GBM5K Content-Range omitted its end")?;
    if reported_start.parse::<u64>()? != start
        || reported_total.parse::<u64>()? != total
        || reported_end.parse::<u64>()? != total.checked_sub(1).context("zero source length")?
    {
        bail!("AACR GBM5K Content-Range differs from the exact requested tail");
    }
    Ok(())
}

fn build_snapshot(
    manifest_sha256: &str,
    api_bytes: &[u8],
    source: &DiscoveredFile,
    directory: &Path,
) -> Result<SourceSnapshot> {
    let api_sha256 = sha256_bytes(api_bytes);
    let file_digests = verify_file(&directory.join(&source.file_name), source)?;
    let source_set_sha256 = source_set_digest(
        manifest_sha256,
        &api_sha256,
        source.file_id,
        &file_digests.sha256,
    );
    Ok(SourceSnapshot {
        schema_version: 1,
        snapshot_id: format!("aacr-gbm5k:{source_set_sha256}"),
        manifest_id: MANIFEST_ID.to_owned(),
        manifest_sha256: manifest_sha256.to_owned(),
        article_id: ARTICLE_ID,
        article_version: ARTICLE_VERSION,
        article_api_response: SnapshotArtifact {
            file_name: ARTICLE_API_FILE.to_owned(),
            byte_length: u64::try_from(api_bytes.len())?,
            sha256: api_sha256,
        },
        source_file: SnapshotSourceFile {
            file_id: source.file_id,
            file_name: source.file_name.clone(),
            byte_length: file_digests.byte_length,
            sha256: file_digests.sha256,
        },
        source_set_sha256,
    })
}

fn verify_source_directory(
    manifest: &SourceManifest,
    manifest_sha256: &str,
    directory: &Path,
) -> Result<SourceSnapshot> {
    let api_path = directory.join(ARTICLE_API_FILE);
    let (source, api_bytes) = read_and_validate_api_response(&api_path, manifest)?;
    let expected = build_snapshot(manifest_sha256, &api_bytes, &source, directory)?;
    let snapshot_path = directory.join(SNAPSHOT_FILE);
    reject_symlink(&snapshot_path)?;
    let bytes =
        fs::read(&snapshot_path).with_context(|| format!("read {}", snapshot_path.display()))?;
    let retained: SourceSnapshot =
        serde_json::from_slice(&bytes).context("decode AACR GBM5K source snapshot")?;
    if pretty_json_bytes(&retained)? != bytes || retained != expected {
        bail!("AACR GBM5K source snapshot is noncanonical or inconsistent");
    }
    Ok(retained)
}

fn verify_file(path: &Path, source: &DiscoveredFile) -> Result<FileDigests> {
    reject_symlink(path)?;
    let digests = file_digests(path)?;
    if digests.byte_length != source.byte_length {
        bail!("AACR GBM5K file differs from its exact API identity");
    }
    Ok(digests)
}

fn file_digests(path: &Path) -> Result<FileDigests> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut sha256 = Sha256::new();
    let mut byte_length = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("read {}", path.display()))?;
        if count == 0 {
            break;
        }
        byte_length = byte_length
            .checked_add(u64::try_from(count)?)
            .context("AACR GBM5K byte length overflow")?;
        sha256.update(&buffer[..count]);
    }
    Ok(FileDigests {
        byte_length,
        sha256: hex::encode(sha256.finalize()),
    })
}

async fn get_limited_exact(client: &Client, url: &str, limit: u64) -> Result<Vec<u8>> {
    if url != ARTICLE_API_URL {
        bail!("AACR Figshare request URL is not the pinned immutable-version API");
    }
    let mut response = client
        .get(url)
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .context("request AACR Figshare article metadata")?
        .error_for_status()
        .context("AACR Figshare article metadata returned an error")?;
    if response.url().scheme() != "https"
        || response.url().host_str() != Some("api.figshare.com")
        || response.url().path() != "/v2/articles/28183566/versions/1"
    {
        bail!("AACR Figshare metadata request redirected away from its pinned endpoint");
    }
    if response
        .content_length()
        .is_none_or(|length| length == 0 || length > limit)
    {
        bail!("AACR Figshare metadata length is missing, empty, or oversized");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if u64::try_from(bytes.len())?.saturating_add(u64::try_from(chunk.len())?) > limit {
            bail!("AACR Figshare metadata exceeded its byte ceiling");
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.is_empty() {
        bail!("AACR Figshare metadata was empty");
    }
    Ok(bytes)
}

fn prepare_output_directory(path: &Path) -> Result<()> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            bail!("AACR GBM5K output must be a real directory, not a symlink");
        }
    } else {
        fs::create_dir_all(path).with_context(|| format!("create {}", path.display()))?;
    }
    Ok(())
}

fn require_figshare_download_url(value: &str, file_id: u64) -> Result<()> {
    let url = Url::parse(value).context("parse AACR Figshare download URL")?;
    let host = url
        .host_str()
        .context("AACR Figshare URL omitted its host")?;
    let expected_paths = [
        format!("/files/{file_id}"),
        format!("/ndownloader/files/{file_id}"),
    ];
    if url.scheme() != "https"
        || !matches!(
            host,
            "ndownloader.figshare.com" | "figshare.com" | "aacr.figshare.com"
        )
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || !expected_paths.iter().any(|path| url.path() == path)
    {
        bail!("AACR Figshare download URL is outside the exact public file route");
    }
    Ok(())
}

fn require_https(value: &str, label: &str) -> Result<()> {
    let url = Url::parse(value).with_context(|| format!("parse {label}"))?;
    if url.scheme() != "https" || url.host_str().is_none() {
        bail!("{label} must be an absolute HTTPS URL");
    }
    Ok(())
}

fn is_safe_xlsx_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            == Path::new(value)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
        && value
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("xlsx"))
        && !value.chars().any(char::is_control)
}

fn reject_symlink(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect source artifact {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!("source artifact {} must be a regular file", path.display());
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn pretty_json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn source_set_digest(
    manifest_sha256: &str,
    api_sha256: &str,
    file_id: u64,
    file_sha256: &str,
) -> String {
    let mut digest = Sha256::new();
    for value in [
        SOURCE_SET_DOMAIN.as_bytes(),
        manifest_sha256.as_bytes(),
        api_sha256.as_bytes(),
    ] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value);
    }
    let file_id_bytes = file_id.to_be_bytes();
    digest.update((file_id_bytes.len() as u64).to_be_bytes());
    digest.update(file_id_bytes);
    let file_sha256_bytes = file_sha256.as_bytes();
    digest.update((file_sha256_bytes.len() as u64).to_be_bytes());
    digest.update(file_sha256_bytes);
    hex::encode(digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] =
        include_bytes!("../../../data/cancer-research/aacr-gbm5k-dependency-source-v1.json");

    fn manifest() -> SourceManifest {
        let manifest: SourceManifest = serde_json::from_slice(MANIFEST).expect("manifest JSON");
        validate_manifest(&manifest).expect("valid checked-in manifest");
        manifest
    }

    fn article() -> FigshareArticle {
        FigshareArticle {
            id: ARTICLE_ID,
            title: TITLE.to_owned(),
            doi: DATASET_DOI.to_owned(),
            resource_doi: RESOURCE_DOI.to_owned(),
            version: ARTICLE_VERSION,
            is_public: true,
            is_confidential: false,
            status: "public".to_owned(),
            defined_type_name: "dataset".to_owned(),
            license: FigshareLicense {
                name: "CC BY 4.0".to_owned(),
                url: "https://creativecommons.org/licenses/by/4.0/".to_owned(),
            },
            files: vec![FigshareFile {
                id: 42_424_242,
                name: "Table_S4.xlsx".to_owned(),
                size: 277_800,
                is_link_only: false,
                download_url: "https://aacr.figshare.com/ndownloader/files/42424242".to_owned(),
            }],
        }
    }

    #[test]
    fn checked_in_manifest_pins_commercial_license_and_closed_leakage() {
        assert_eq!(sha256_bytes(MANIFEST), MANIFEST_SHA256);
        let manifest = manifest();
        assert_eq!(manifest.source.article_id, ARTICLE_ID);
        assert_eq!(manifest.source.article_version, 1);
        assert_eq!(manifest.license.spdx_identifier, "CC-BY-4.0");
        assert!(!manifest.leakage_boundary.research_prompt_access);
        assert!(!manifest.leakage_boundary.research_memory_access);
        assert!(!manifest.leakage_boundary.campaign_selection_access);
    }

    #[test]
    fn exact_version_api_resolves_one_bounded_non_link_xlsx() {
        let file = validate_article(&article(), &manifest()).expect("valid article");
        assert_eq!(file.file_id, 42_424_242);
        assert_eq!(file.file_name, "Table_S4.xlsx");
        assert_eq!(file.byte_length, 277_800);
    }

    #[test]
    fn changed_license_version_or_file_cardinality_fails_closed() {
        let manifest = manifest();
        let mut changed = article();
        changed.version = 2;
        assert!(validate_article(&changed, &manifest).is_err());
        changed = article();
        changed.license.url = "https://creativecommons.org/licenses/by-nc/4.0/".to_owned();
        assert!(validate_article(&changed, &manifest).is_err());
        changed = article();
        changed.files.push(changed.files[0].clone());
        assert!(validate_article(&changed, &manifest).is_err());
    }

    #[test]
    fn link_only_unsafe_or_oversized_file_fails_closed() {
        let manifest = manifest();
        let mut changed = article();
        changed.files[0].is_link_only = true;
        assert!(validate_article(&changed, &manifest).is_err());
        changed = article();
        changed.files[0].name = "../Table_S4.xlsx".to_owned();
        assert!(validate_article(&changed, &manifest).is_err());
        changed = article();
        changed.files[0].size = manifest.artifact.maximum_byte_length + 1;
        assert!(validate_article(&changed, &manifest).is_err());
    }

    #[test]
    fn download_url_is_bound_to_the_exact_file_id() {
        assert!(
            require_figshare_download_url(
                "https://ndownloader.figshare.com/files/42424242",
                42_424_242
            )
            .is_ok()
        );
        assert!(
            require_figshare_download_url("https://evil.example/files/42424242", 42_424_242)
                .is_err()
        );
        assert!(
            require_figshare_download_url(
                "https://aacr.figshare.com/ndownloader/files/42424243",
                42_424_242
            )
            .is_err()
        );
    }

    #[test]
    fn range_validation_requires_the_exact_remaining_tail() {
        assert!(validate_content_range("bytes 100-999/1000", 100, 1_000).is_ok());
        assert!(validate_content_range("bytes 100-998/1000", 100, 1_000).is_err());
        assert!(validate_content_range("bytes 99-999/1000", 100, 1_000).is_err());
    }

    #[test]
    fn an_existing_zero_byte_partial_is_resumable_without_recreation() {
        let directory =
            std::env::temp_dir().join(format!("atiny-gbm5k-zero-partial-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("create test directory");
        let path = directory.join("Table_S4.xlsx.part");
        File::create(&path).expect("create zero-byte partial");
        let mut file = open_partial_file(&path, 0, true).expect("open existing partial");
        file.write_all(b"resume").expect("append resumed bytes");
        drop(file);
        assert_eq!(fs::read(&path).expect("read resumed bytes"), b"resume");
        fs::remove_file(path).expect("remove test file");
        fs::remove_dir(directory).expect("remove test directory");
    }
}
