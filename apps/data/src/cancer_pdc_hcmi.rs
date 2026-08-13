use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

use anyhow::{Context, Result, bail};
use md5::Md5;
use reqwest::Client;
use serde::{Deserialize, Deserializer, Serialize, de};
use sha2::Digest as _;
use uuid::Uuid;
use world_domain::Digest;

const GRAPHQL_ENDPOINT: &str = "https://pdc.cancer.gov/graphql";
const STUDY_ID: &str = "PDC000711";
const STUDY_UUID: &str = "ec0e442b-a0b8-4dc7-a4ba-6b5409fc68de";
const SOURCE_FILE_ID: &str = "86e9b7f6-0776-4cb7-b761-dee14321b318";
const SOURCE_FILE_NAME: &str = "Global_all_original.txt";
const SOURCE_FILE_SIZE: u64 = 8_118_871;
const SOURCE_FILE_MD5: &str = "333eef379eaea258efca326d579eef21";
const SOURCE_METADATA_FILE: &str = "file-metadata.json";
const BIOSPECIMEN_FILE: &str = "biospecimens.json";
const SNAPSHOT_FILE: &str = "source-snapshot.json";
const DERIVED_FILE: &str = "pdc000711-gbm-proteome.tsv";
const DERIVED_METADATA_FILE: &str = "pdc000711-gbm-proteome.metadata.json";
const MAX_GRAPHQL_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const SOURCE_SET_DOMAIN: &str = "a-tiny-civilization/pdc000711/source-set/v1";
const FILE_QUERY: &str = r#"{ filesPerStudy(pdc_study_id:"PDC000711" file_name:"Global_all_original.txt" offset:0 limit:10) { study_id pdc_study_id file_id file_name file_size md5sum file_location data_category file_type file_format signedUrl { url } } }"#;
const BIOSPECIMEN_QUERY: &str = r#"{ biospecimenPerStudy(pdc_study_id:"PDC000711" acceptDUA:true) { aliquot_id sample_id case_id aliquot_submitter_id sample_submitter_id case_submitter_id sample_type disease_type primary_site } }"#;
const ANNOTATION_COLUMNS: [&str; 4] = ["T: Index", "T: NumberPSM", "T: ProteinID", "T: MaxPepProb"];

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SourceManifest {
    schema_version: u16,
    manifest_id: String,
    source: StudyManifest,
    license: LicenseManifest,
    endpoints: EndpointManifest,
    queries: QueryManifest,
    file: FileManifest,
    expected_source_shape: ExpectedSourceShape,
    gbm_selection: GbmSelection,
    scientific_boundary: ScientificBoundary,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StudyManifest {
    custodian: String,
    program: String,
    study_title: String,
    pdc_study_id: String,
    study_version_uuid: String,
    study_url: String,
    hcmi_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LicenseManifest {
    spdx_identifier: String,
    license_url: String,
    source_terms_url: String,
    attribution: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct EndpointManifest {
    graphql: String,
    api_documentation: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct QueryManifest {
    file_metadata: String,
    biospecimens: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FileManifest {
    file_id: String,
    file_name: String,
    file_size: u64,
    md5: String,
    file_location: String,
    data_category: String,
    file_type: String,
    file_format: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ExpectedSourceShape {
    biospecimen_records: usize,
    model_columns: usize,
    data_rows: usize,
    annotation_columns: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct GbmSelection {
    disease_type: String,
    primary_site: String,
    expected_model_count: usize,
    expected_sample_type_counts: BTreeMap<String, usize>,
    expected_case_submitter_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ScientificBoundary {
    measurement: String,
    missing_value_policy: String,
    allowed_claim: String,
    prohibited_claim: String,
}

#[derive(Debug, Serialize)]
struct GraphqlRequest<'a> {
    query: &'a str,
}

#[derive(Debug, Deserialize)]
struct FileEnvelope {
    data: Option<FileResponseData>,
    #[serde(default)]
    errors: Vec<GraphqlError>,
}

#[derive(Debug, Deserialize)]
struct FileResponseData {
    #[serde(rename = "filesPerStudy")]
    files_per_study: Vec<PdcFile>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct PdcFile {
    study_id: String,
    pdc_study_id: String,
    file_id: String,
    file_name: String,
    #[serde(deserialize_with = "deserialize_u64")]
    file_size: u64,
    md5sum: String,
    file_location: String,
    data_category: String,
    file_type: String,
    file_format: String,
    #[serde(rename = "signedUrl")]
    signed_url: SignedUrl,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SignedUrl {
    url: String,
}

#[derive(Debug, Deserialize)]
struct BiospecimenEnvelope {
    data: Option<BiospecimenResponseData>,
    #[serde(default)]
    errors: Vec<GraphqlError>,
}

#[derive(Debug, Deserialize)]
struct BiospecimenResponseData {
    #[serde(rename = "biospecimenPerStudy")]
    biospecimen_per_study: Vec<Biospecimen>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
struct Biospecimen {
    aliquot_id: String,
    sample_id: String,
    case_id: String,
    aliquot_submitter_id: String,
    sample_submitter_id: String,
    case_submitter_id: String,
    sample_type: String,
    disease_type: String,
    primary_site: String,
}

#[derive(Debug, Deserialize)]
struct GraphqlError {
    message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SourceSnapshot {
    schema_version: u16,
    snapshot_id: String,
    manifest_id: String,
    manifest_sha256: Digest,
    pdc_study_id: String,
    study_version_uuid: String,
    source_file: SnapshotSourceFile,
    file_metadata: SnapshotArtifact,
    biospecimen_metadata: SnapshotBiospecimens,
    source_set_sha256: Digest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SnapshotSourceFile {
    file_id: String,
    file_name: String,
    byte_length: u64,
    md5: String,
    sha256: Digest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SnapshotArtifact {
    file_name: String,
    byte_length: u64,
    sha256: Digest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SnapshotBiospecimens {
    file_name: String,
    byte_length: u64,
    sha256: Digest,
    record_count: usize,
    selected_gbm_model_count: usize,
}

#[derive(Debug, Serialize)]
struct DerivedMetadata {
    schema_version: u16,
    artifact_id: String,
    artifact_file_name: String,
    media_type: String,
    artifact_content_address: String,
    artifact_sha256: Digest,
    artifact_byte_length: u64,
    source: DerivedSource,
    transformation: Transformation,
    dimensions: DerivedDimensions,
    join_provenance: Vec<JoinProvenance>,
    limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DerivedSource {
    manifest_id: String,
    manifest_sha256: Digest,
    source_set_sha256: Digest,
    pdc_study_id: String,
    study_version_uuid: String,
    file_id: String,
    source_file_sha256: Digest,
    biospecimen_metadata_sha256: Digest,
}

#[derive(Debug, Serialize)]
struct Transformation {
    model_selection: String,
    column_order: String,
    missing_value_policy: String,
    annotation_columns_preserved: Vec<String>,
    numeric_values_reparsed: bool,
    imputation_applied: bool,
}

#[derive(Debug, Serialize)]
struct DerivedDimensions {
    data_rows: usize,
    model_columns: usize,
    annotation_columns: usize,
    total_columns: usize,
    observed_model_cells: u64,
    missing_model_cells: u64,
}

#[derive(Clone, Debug, Serialize)]
struct JoinProvenance {
    derived_column_index: usize,
    source_column_index: usize,
    matrix_header: String,
    join_field: String,
    case_id: String,
    case_submitter_id: String,
    sample_id: String,
    sample_submitter_id: String,
    aliquot_id: String,
    aliquot_submitter_id: String,
    sample_type: String,
    disease_type: String,
    primary_site: String,
}

struct VerifiedSource {
    snapshot: SourceSnapshot,
    biospecimens: Vec<Biospecimen>,
    source_bytes: Vec<u8>,
}

struct MatrixDerivation {
    bytes: Vec<u8>,
    rows: usize,
    observed_cells: u64,
    missing_cells: u64,
    joins: Vec<JoinProvenance>,
}

pub async fn acquire(manifest_path: &Path, output_directory: &Path) -> Result<()> {
    let (manifest, manifest_bytes) = read_manifest(manifest_path)?;
    let manifest_sha256 = Digest::sha256(&manifest_bytes);
    let snapshot_path = output_directory.join(SNAPSHOT_FILE);
    if snapshot_path.exists() {
        let verified = verify_source_directory(&manifest, manifest_sha256, output_directory)?;
        println!(
            "verified PDC000711 source snapshot {} ({} GBM models)",
            verified.snapshot.source_set_sha256,
            verified
                .snapshot
                .biospecimen_metadata
                .selected_gbm_model_count
        );
        return Ok(());
    }
    fs::create_dir_all(output_directory).with_context(|| {
        format!(
            "create PDC000711 source directory {}",
            output_directory.display()
        )
    })?;

    let client = Client::builder()
        .https_only(true)
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(120))
        .user_agent("a-tiny-civilization-pdc000711-acquisition/0.1")
        .build()
        .context("construct PDC client")?;

    let file_response = post_graphql(&client, FILE_QUERY).await?;
    let file_envelope: FileEnvelope =
        serde_json::from_slice(&file_response).context("decode PDC file response")?;
    let pdc_file = validated_file_response(file_envelope, &manifest)?;
    let signed_url = pdc_file.signed_url.url.clone();
    let source_path = output_directory.join(SOURCE_FILE_NAME);
    let source_bytes = if source_path.exists() {
        fs::read(&source_path).with_context(|| format!("read {}", source_path.display()))?
    } else {
        let bytes = download_limited(&client, &signed_url, MAX_SOURCE_BYTES).await?;
        verify_source_bytes(&manifest.file, &bytes)?;
        write_new(&source_path, &bytes)?;
        bytes
    };
    verify_source_bytes(&manifest.file, &source_bytes)?;

    let biospecimen_response = post_graphql(&client, BIOSPECIMEN_QUERY).await?;
    let biospecimen_envelope: BiospecimenEnvelope =
        serde_json::from_slice(&biospecimen_response).context("decode PDC biospecimen response")?;
    let biospecimens = validated_biospecimens(biospecimen_envelope, &manifest)?;

    let stable_file_metadata = StableFileMetadata::from(&pdc_file);
    let file_metadata_bytes = pretty_json_bytes(&stable_file_metadata)?;
    let biospecimen_bytes = pretty_json_bytes(&biospecimens)?;
    write_or_verify_new(
        &output_directory.join(SOURCE_METADATA_FILE),
        &file_metadata_bytes,
    )?;
    write_or_verify_new(&output_directory.join(BIOSPECIMEN_FILE), &biospecimen_bytes)?;

    let source_file_sha256 = Digest::sha256(&source_bytes);
    let file_metadata_sha256 = Digest::sha256(&file_metadata_bytes);
    let biospecimen_sha256 = Digest::sha256(&biospecimen_bytes);
    let source_set_sha256 = source_set_digest(
        manifest_sha256,
        source_file_sha256,
        file_metadata_sha256,
        biospecimen_sha256,
    );
    let snapshot = SourceSnapshot {
        schema_version: 1,
        snapshot_id: format!("pdc000711:{source_set_sha256}"),
        manifest_id: manifest.manifest_id.clone(),
        manifest_sha256,
        pdc_study_id: STUDY_ID.to_owned(),
        study_version_uuid: STUDY_UUID.to_owned(),
        source_file: SnapshotSourceFile {
            file_id: SOURCE_FILE_ID.to_owned(),
            file_name: SOURCE_FILE_NAME.to_owned(),
            byte_length: u64::try_from(source_bytes.len())?,
            md5: hex::encode(Md5::digest(&source_bytes)),
            sha256: source_file_sha256,
        },
        file_metadata: SnapshotArtifact {
            file_name: SOURCE_METADATA_FILE.to_owned(),
            byte_length: u64::try_from(file_metadata_bytes.len())?,
            sha256: file_metadata_sha256,
        },
        biospecimen_metadata: SnapshotBiospecimens {
            file_name: BIOSPECIMEN_FILE.to_owned(),
            byte_length: u64::try_from(biospecimen_bytes.len())?,
            sha256: biospecimen_sha256,
            record_count: biospecimens.len(),
            selected_gbm_model_count: selected_biospecimens(&biospecimens, &manifest).len(),
        },
        source_set_sha256,
    };
    write_json_new(&snapshot_path, &snapshot)?;
    println!(
        "acquired PDC000711 file {} and {} biospecimens; source snapshot {}",
        snapshot.source_file.file_id,
        snapshot.biospecimen_metadata.record_count,
        snapshot.source_set_sha256
    );
    Ok(())
}

pub fn derive(
    manifest_path: &Path,
    source_directory: &Path,
    output_directory: &Path,
) -> Result<()> {
    let (manifest, manifest_bytes) = read_manifest(manifest_path)?;
    let manifest_sha256 = Digest::sha256(&manifest_bytes);
    let source = verify_source_directory(&manifest, manifest_sha256, source_directory)?;
    let derivation = derive_matrix(&source.source_bytes, &source.biospecimens, &manifest)?;
    let artifact_sha256 = Digest::sha256(&derivation.bytes);
    let metadata = DerivedMetadata {
        schema_version: 1,
        artifact_id: format!("pdc000711-hcmi-gbm-proteome:{artifact_sha256}"),
        artifact_file_name: DERIVED_FILE.to_owned(),
        media_type: "text/tab-separated-values; charset=utf-8".to_owned(),
        artifact_content_address: format!("sha256:{artifact_sha256}"),
        artifact_sha256,
        artifact_byte_length: u64::try_from(derivation.bytes.len())?,
        source: DerivedSource {
            manifest_id: source.snapshot.manifest_id.clone(),
            manifest_sha256: source.snapshot.manifest_sha256,
            source_set_sha256: source.snapshot.source_set_sha256,
            pdc_study_id: source.snapshot.pdc_study_id.clone(),
            study_version_uuid: source.snapshot.study_version_uuid.clone(),
            file_id: source.snapshot.source_file.file_id.clone(),
            source_file_sha256: source.snapshot.source_file.sha256,
            biospecimen_metadata_sha256: source.snapshot.biospecimen_metadata.sha256,
        },
        transformation: Transformation {
            model_selection: "Exact inner join of matrix header to PDC case_submitter_id where disease_type is Glioblastoma and primary_site is Brain; the expected 30-case set is pinned by the checked-in manifest.".to_owned(),
            column_order: "Selected model columns retain their original Global_all_original.txt order; the four source annotation columns remain last and in source order.".to_owned(),
            missing_value_policy: "Empty source fields remain empty fields; no value is filled, imputed, interpolated, converted to zero, or otherwise synthesized.".to_owned(),
            annotation_columns_preserved: manifest.expected_source_shape.annotation_columns.clone(),
            numeric_values_reparsed: false,
            imputation_applied: false,
        },
        dimensions: DerivedDimensions {
            data_rows: derivation.rows,
            model_columns: derivation.joins.len(),
            annotation_columns: ANNOTATION_COLUMNS.len(),
            total_columns: derivation.joins.len() + ANNOTATION_COLUMNS.len(),
            observed_model_cells: derivation.observed_cells,
            missing_model_cells: derivation.missing_cells,
        },
        join_provenance: derivation.joins,
        limitations: vec![
            manifest.scientific_boundary.measurement.clone(),
            manifest.scientific_boundary.prohibited_claim.clone(),
            "T: Index and T: ProteinID are retained as source text. Excel-like labels such as 1-Mar are not silently repaired or remapped.".to_owned(),
            "A blank field is missing source data, not a measured zero or evidence of absence.".to_owned(),
        ],
    };
    let metadata_bytes = pretty_json_bytes(&metadata)?;
    fs::create_dir_all(output_directory).with_context(|| {
        format!(
            "create PDC000711 derived directory {}",
            output_directory.display()
        )
    })?;
    write_new(&output_directory.join(DERIVED_FILE), &derivation.bytes)?;
    write_new(
        &output_directory.join(DERIVED_METADATA_FILE),
        &metadata_bytes,
    )?;
    println!(
        "derived {} rows x {} GBM models with {} preserved missing cells; artifact {}",
        metadata.dimensions.data_rows,
        metadata.dimensions.model_columns,
        metadata.dimensions.missing_model_cells,
        metadata.artifact_sha256
    );
    Ok(())
}

#[derive(Debug, Deserialize, Serialize)]
struct StableFileMetadata {
    study_id: String,
    pdc_study_id: String,
    file_id: String,
    file_name: String,
    file_size: u64,
    md5sum: String,
    file_location: String,
    data_category: String,
    file_type: String,
    file_format: String,
}

impl From<&PdcFile> for StableFileMetadata {
    fn from(file: &PdcFile) -> Self {
        Self {
            study_id: file.study_id.clone(),
            pdc_study_id: file.pdc_study_id.clone(),
            file_id: file.file_id.clone(),
            file_name: file.file_name.clone(),
            file_size: file.file_size,
            md5sum: file.md5sum.clone(),
            file_location: file.file_location.clone(),
            data_category: file.data_category.clone(),
            file_type: file.file_type.clone(),
            file_format: file.file_format.clone(),
        }
    }
}

fn read_manifest(path: &Path) -> Result<(SourceManifest, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let manifest: SourceManifest =
        serde_json::from_slice(&bytes).with_context(|| format!("decode {}", path.display()))?;
    validate_manifest(&manifest)?;
    Ok((manifest, bytes))
}

fn validate_manifest(manifest: &SourceManifest) -> Result<()> {
    let expected_annotations = ANNOTATION_COLUMNS
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let expected_sample_types = BTreeMap::from([
        ("Expanded Next Generation Cancer Model".to_owned(), 3),
        ("Next Generation Cancer Model".to_owned(), 27),
    ]);
    if manifest.schema_version != 1
        || manifest.manifest_id != "pdc000711-hcmi-gbm-proteome-source-v1"
        || manifest.source.pdc_study_id != STUDY_ID
        || manifest.source.study_version_uuid != STUDY_UUID
        || manifest.endpoints.graphql != GRAPHQL_ENDPOINT
        || manifest.queries.file_metadata != FILE_QUERY
        || manifest.queries.biospecimens != BIOSPECIMEN_QUERY
        || manifest.file.file_id != SOURCE_FILE_ID
        || manifest.file.file_name != SOURCE_FILE_NAME
        || manifest.file.file_size != SOURCE_FILE_SIZE
        || manifest.file.md5 != SOURCE_FILE_MD5
        || manifest.file.file_location != "studies/711/suppl/Global_all_original.txt"
        || manifest.file.data_category != "Alternate Processing Pipeline"
        || manifest.file.file_type != "Text"
        || manifest.file.file_format != "tsv"
        || manifest.license.spdx_identifier != "CC-BY-4.0"
        || manifest.license.license_url != "https://creativecommons.org/licenses/by/4.0/"
        || manifest.expected_source_shape.biospecimen_records != 75
        || manifest.expected_source_shape.model_columns != 75
        || manifest.expected_source_shape.data_rows != 12_342
        || manifest.expected_source_shape.annotation_columns != expected_annotations
        || manifest.gbm_selection.disease_type != "Glioblastoma"
        || manifest.gbm_selection.primary_site != "Brain"
        || manifest.gbm_selection.expected_model_count != 30
        || manifest.gbm_selection.expected_sample_type_counts != expected_sample_types
    {
        bail!("unsupported or inconsistent PDC000711 source manifest");
    }
    require_https(&manifest.source.study_url, "study URL")?;
    require_https(&manifest.source.hcmi_url, "HCMI URL")?;
    require_https(&manifest.license.license_url, "license URL")?;
    require_https(&manifest.license.source_terms_url, "source terms URL")?;
    require_https(
        &manifest.endpoints.api_documentation,
        "API documentation URL",
    )?;
    if Uuid::parse_str(&manifest.source.study_version_uuid).is_err()
        || Uuid::parse_str(&manifest.file.file_id).is_err()
        || manifest.license.attribution.trim().is_empty()
        || manifest.source.custodian.trim().is_empty()
        || manifest.source.program.trim().is_empty()
        || manifest.source.study_title.trim().is_empty()
        || manifest.scientific_boundary.measurement.trim().is_empty()
        || manifest
            .scientific_boundary
            .missing_value_policy
            .trim()
            .is_empty()
        || manifest.scientific_boundary.allowed_claim.trim().is_empty()
        || manifest
            .scientific_boundary
            .prohibited_claim
            .trim()
            .is_empty()
    {
        bail!("PDC000711 manifest has malformed identity or policy fields");
    }
    let ids = &manifest.gbm_selection.expected_case_submitter_ids;
    if ids.len() != 30
        || ids.windows(2).any(|pair| pair[0] >= pair[1])
        || ids.iter().any(|id| !id.starts_with("HCM-BROD-"))
    {
        bail!("PDC000711 expected GBM case identifiers are not canonical");
    }
    Ok(())
}

fn validated_file_response(envelope: FileEnvelope, manifest: &SourceManifest) -> Result<PdcFile> {
    require_no_graphql_errors(&envelope.errors, "file metadata")?;
    let mut files = envelope
        .data
        .context("PDC file response has no data")?
        .files_per_study;
    if files.len() != 1 {
        bail!(
            "PDC returned {} matching source files, expected one",
            files.len()
        );
    }
    let file = files.pop().context("PDC source file disappeared")?;
    if file.study_id != manifest.source.study_version_uuid
        || file.pdc_study_id != manifest.source.pdc_study_id
        || file.file_id != manifest.file.file_id
        || file.file_name != manifest.file.file_name
        || file.file_size != manifest.file.file_size
        || file.md5sum != manifest.file.md5
        || file.file_location != manifest.file.file_location
        || file.data_category != manifest.file.data_category
        || file.file_type != manifest.file.file_type
        || file.file_format != manifest.file.file_format
    {
        bail!("PDC file metadata differs from the checked-in manifest");
    }
    require_https(&file.signed_url.url, "PDC signed download URL")?;
    Ok(file)
}

fn validated_biospecimens(
    envelope: BiospecimenEnvelope,
    manifest: &SourceManifest,
) -> Result<Vec<Biospecimen>> {
    require_no_graphql_errors(&envelope.errors, "biospecimens")?;
    let mut records = envelope
        .data
        .context("PDC biospecimen response has no data")?
        .biospecimen_per_study;
    records.sort();
    if records.len() != manifest.expected_source_shape.biospecimen_records {
        bail!(
            "PDC returned {} biospecimens, expected {}",
            records.len(),
            manifest.expected_source_shape.biospecimen_records
        );
    }
    let mut aliquots = BTreeSet::new();
    for record in &records {
        if !aliquots.insert(&record.aliquot_id)
            || Uuid::parse_str(&record.aliquot_id).is_err()
            || Uuid::parse_str(&record.sample_id).is_err()
            || Uuid::parse_str(&record.case_id).is_err()
            || record.case_submitter_id.trim().is_empty()
            || record.sample_submitter_id.trim().is_empty()
            || record.aliquot_submitter_id.trim().is_empty()
        {
            bail!("PDC biospecimen response has malformed or duplicate identities");
        }
    }
    let selected = selected_biospecimens(&records, manifest);
    let selected_ids = selected
        .iter()
        .map(|record| record.case_submitter_id.clone())
        .collect::<BTreeSet<_>>();
    let expected_ids = manifest
        .gbm_selection
        .expected_case_submitter_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let sample_types = selected.iter().fold(BTreeMap::new(), |mut counts, record| {
        *counts.entry(record.sample_type.clone()).or_insert(0) += 1;
        counts
    });
    if selected.len() != manifest.gbm_selection.expected_model_count
        || selected_ids != expected_ids
        || sample_types != manifest.gbm_selection.expected_sample_type_counts
    {
        bail!("PDC GBM biospecimen selection differs from the checked-in manifest");
    }
    Ok(records)
}

fn selected_biospecimens<'a>(
    records: &'a [Biospecimen],
    manifest: &SourceManifest,
) -> Vec<&'a Biospecimen> {
    records
        .iter()
        .filter(|record| {
            record.disease_type == manifest.gbm_selection.disease_type
                && record.primary_site == manifest.gbm_selection.primary_site
        })
        .collect()
}

fn verify_source_directory(
    manifest: &SourceManifest,
    manifest_sha256: Digest,
    directory: &Path,
) -> Result<VerifiedSource> {
    let snapshot: SourceSnapshot = read_json(&directory.join(SNAPSHOT_FILE))?;
    let source_bytes = fs::read(directory.join(SOURCE_FILE_NAME))
        .with_context(|| format!("read PDC source file {SOURCE_FILE_NAME}"))?;
    verify_source_bytes(&manifest.file, &source_bytes)?;
    let file_metadata_bytes = fs::read(directory.join(SOURCE_METADATA_FILE))
        .with_context(|| format!("read PDC metadata {SOURCE_METADATA_FILE}"))?;
    let file_metadata: StableFileMetadata = serde_json::from_slice(&file_metadata_bytes)
        .context("decode canonical PDC file metadata")?;
    if pretty_json_bytes(&file_metadata)? != file_metadata_bytes
        || file_metadata.study_id != manifest.source.study_version_uuid
        || file_metadata.pdc_study_id != manifest.source.pdc_study_id
        || file_metadata.file_id != manifest.file.file_id
        || file_metadata.file_name != manifest.file.file_name
        || file_metadata.file_size != manifest.file.file_size
        || file_metadata.md5sum != manifest.file.md5
        || file_metadata.file_location != manifest.file.file_location
        || file_metadata.data_category != manifest.file.data_category
        || file_metadata.file_type != manifest.file.file_type
        || file_metadata.file_format != manifest.file.file_format
    {
        bail!("PDC file metadata is noncanonical or differs from the trust manifest");
    }
    let biospecimen_bytes = fs::read(directory.join(BIOSPECIMEN_FILE))
        .with_context(|| format!("read PDC metadata {BIOSPECIMEN_FILE}"))?;
    let biospecimens: Vec<Biospecimen> =
        serde_json::from_slice(&biospecimen_bytes).context("decode canonical PDC biospecimens")?;
    if pretty_json_bytes(&biospecimens)? != biospecimen_bytes {
        bail!("PDC biospecimen metadata is not canonical JSON");
    }
    let envelope = BiospecimenEnvelope {
        data: Some(BiospecimenResponseData {
            biospecimen_per_study: biospecimens.clone(),
        }),
        errors: Vec::new(),
    };
    let biospecimens = validated_biospecimens(envelope, manifest)?;
    let file_sha256 = Digest::sha256(&source_bytes);
    let file_metadata_sha256 = Digest::sha256(&file_metadata_bytes);
    let biospecimen_sha256 = Digest::sha256(&biospecimen_bytes);
    let source_set_sha256 = source_set_digest(
        manifest_sha256,
        file_sha256,
        file_metadata_sha256,
        biospecimen_sha256,
    );
    if snapshot.schema_version != 1
        || snapshot.snapshot_id != format!("pdc000711:{source_set_sha256}")
        || snapshot.manifest_id != manifest.manifest_id
        || snapshot.manifest_sha256 != manifest_sha256
        || snapshot.pdc_study_id != STUDY_ID
        || snapshot.study_version_uuid != STUDY_UUID
        || snapshot.source_file.file_id != SOURCE_FILE_ID
        || snapshot.source_file.file_name != SOURCE_FILE_NAME
        || snapshot.source_file.byte_length != u64::try_from(source_bytes.len())?
        || snapshot.source_file.md5 != SOURCE_FILE_MD5
        || snapshot.source_file.sha256 != file_sha256
        || snapshot.file_metadata.file_name != SOURCE_METADATA_FILE
        || snapshot.file_metadata.byte_length != u64::try_from(file_metadata_bytes.len())?
        || snapshot.file_metadata.sha256 != file_metadata_sha256
        || snapshot.biospecimen_metadata.file_name != BIOSPECIMEN_FILE
        || snapshot.biospecimen_metadata.byte_length != u64::try_from(biospecimen_bytes.len())?
        || snapshot.biospecimen_metadata.sha256 != biospecimen_sha256
        || snapshot.biospecimen_metadata.record_count != biospecimens.len()
        || snapshot.biospecimen_metadata.selected_gbm_model_count
            != manifest.gbm_selection.expected_model_count
        || snapshot.source_set_sha256 != source_set_sha256
    {
        bail!("PDC000711 source snapshot is inconsistent with retained bytes");
    }
    Ok(VerifiedSource {
        snapshot,
        biospecimens,
        source_bytes,
    })
}

fn derive_matrix(
    source: &[u8],
    biospecimens: &[Biospecimen],
    manifest: &SourceManifest,
) -> Result<MatrixDerivation> {
    let text = std::str::from_utf8(source).context("PDC source matrix is not UTF-8")?;
    let mut lines = text.split_terminator('\n');
    let header_line = lines
        .next()
        .context("PDC source matrix has no header")?
        .strip_suffix('\r')
        .unwrap_or_else(|| text.lines().next().unwrap_or_default());
    let header = header_line.split('\t').collect::<Vec<_>>();
    let annotation_count = ANNOTATION_COLUMNS.len();
    let expected_columns = manifest.expected_source_shape.model_columns + annotation_count;
    if header.len() != expected_columns
        || header[manifest.expected_source_shape.model_columns..] != ANNOTATION_COLUMNS
    {
        bail!("PDC source matrix shape or annotation columns changed");
    }
    let selected_records = selected_biospecimens(biospecimens, manifest);
    let mut by_case = BTreeMap::new();
    for record in selected_records {
        if by_case
            .insert(record.case_submitter_id.as_str(), record)
            .is_some()
        {
            bail!(
                "PDC GBM case {} has more than one matrix join record",
                record.case_submitter_id
            );
        }
    }
    let mut selected_columns = Vec::new();
    let mut joins = Vec::new();
    for (source_index, name) in header[..manifest.expected_source_shape.model_columns]
        .iter()
        .enumerate()
    {
        if let Some(record) = by_case.get(name) {
            selected_columns.push(source_index);
            joins.push(JoinProvenance {
                derived_column_index: joins.len(),
                source_column_index: source_index,
                matrix_header: (*name).to_owned(),
                join_field: "case_submitter_id".to_owned(),
                case_id: record.case_id.clone(),
                case_submitter_id: record.case_submitter_id.clone(),
                sample_id: record.sample_id.clone(),
                sample_submitter_id: record.sample_submitter_id.clone(),
                aliquot_id: record.aliquot_id.clone(),
                aliquot_submitter_id: record.aliquot_submitter_id.clone(),
                sample_type: record.sample_type.clone(),
                disease_type: record.disease_type.clone(),
                primary_site: record.primary_site.clone(),
            });
        }
    }
    if selected_columns.len() != manifest.gbm_selection.expected_model_count
        || joins
            .iter()
            .map(|join| join.matrix_header.as_str())
            .collect::<BTreeSet<_>>()
            != by_case.keys().copied().collect::<BTreeSet<_>>()
    {
        bail!("PDC source matrix does not contain the exact 30 GBM headers once each");
    }

    let mut output = Vec::with_capacity(source.len() / 2);
    write_selected_row(
        &mut output,
        &header,
        &selected_columns,
        manifest.expected_source_shape.model_columns,
    );
    let mut rows = 0usize;
    let mut observed_cells = 0u64;
    let mut missing_cells = 0u64;
    for line in lines {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            bail!("PDC source matrix contains an empty data row");
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != expected_columns {
            bail!(
                "PDC source row {} has {} columns, expected {}",
                rows + 1,
                fields.len(),
                expected_columns
            );
        }
        for index in &selected_columns {
            if fields[*index].is_empty() {
                missing_cells += 1;
            } else {
                observed_cells += 1;
            }
        }
        write_selected_row(
            &mut output,
            &fields,
            &selected_columns,
            manifest.expected_source_shape.model_columns,
        );
        rows += 1;
    }
    if rows != manifest.expected_source_shape.data_rows {
        bail!(
            "PDC source matrix has {rows} data rows, expected {}",
            manifest.expected_source_shape.data_rows
        );
    }
    Ok(MatrixDerivation {
        bytes: output,
        rows,
        observed_cells,
        missing_cells,
        joins,
    })
}

fn write_selected_row(
    output: &mut Vec<u8>,
    fields: &[&str],
    selected_columns: &[usize],
    annotation_start: usize,
) {
    let mut first = true;
    for value in selected_columns
        .iter()
        .map(|index| fields[*index])
        .chain(fields[annotation_start..].iter().copied())
    {
        if !first {
            output.push(b'\t');
        }
        output.extend_from_slice(value.as_bytes());
        first = false;
    }
    output.push(b'\n');
}

fn verify_source_bytes(file: &FileManifest, bytes: &[u8]) -> Result<()> {
    if u64::try_from(bytes.len())? != file.file_size {
        bail!("PDC source file has wrong byte length");
    }
    if hex::encode(Md5::digest(bytes)) != file.md5 {
        bail!("PDC source file failed its pinned MD5");
    }
    Ok(())
}

fn source_set_digest(
    manifest: Digest,
    source_file: Digest,
    file_metadata: Digest,
    biospecimens: Digest,
) -> Digest {
    Digest::sha256(
        format!("{SOURCE_SET_DOMAIN}\0{manifest}\0{source_file}\0{file_metadata}\0{biospecimens}")
            .as_bytes(),
    )
}

async fn post_graphql(client: &Client, query: &str) -> Result<Vec<u8>> {
    let response = client
        .post(GRAPHQL_ENDPOINT)
        .json(&GraphqlRequest { query })
        .send()
        .await
        .context("request PDC GraphQL")?
        .error_for_status()
        .context("PDC rejected GraphQL request")?;
    read_limited(response, MAX_GRAPHQL_BYTES, "PDC GraphQL response").await
}

async fn download_limited(client: &Client, url: &str, limit: u64) -> Result<Vec<u8>> {
    require_https(url, "PDC signed download URL")?;
    let response = client
        .get(url)
        .send()
        .await
        .context("request PDC signed download")?
        .error_for_status()
        .context("PDC signed download was rejected")?;
    read_limited(response, limit, "PDC source download").await
}

async fn read_limited(response: reqwest::Response, limit: u64, label: &str) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        bail!("{label} exceeds bounded size");
    }
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("read {label}"))?;
    if u64::try_from(bytes.len())? > limit {
        bail!("{label} exceeds bounded size");
    }
    Ok(bytes.to_vec())
}

fn require_no_graphql_errors(errors: &[GraphqlError], label: &str) -> Result<()> {
    if !errors.is_empty() {
        let messages = errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        bail!("PDC {label} query failed: {messages}");
    }
    Ok(())
}

fn require_https(value: &str, label: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).with_context(|| format!("parse {label}"))?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
        bail!("{label} must be an absolute HTTPS URL without user information");
    }
    Ok(())
}

fn deserialize_u64<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number {
        Integer(u64),
        Text(String),
    }
    match Number::deserialize(deserializer)? {
        Number::Integer(value) => Ok(value),
        Number::Text(value) => value.parse().map_err(de::Error::custom),
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("decode {}", path.display()))
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_new(path, &pretty_json_bytes(value)?)
}

fn pretty_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).context("encode canonical JSON")?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_or_verify_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        let existing = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        if existing != bytes {
            bail!("refusing to replace differing artifact {}", path.display());
        }
        return Ok(());
    }
    write_new(path, bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("create new artifact {}", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] =
        include_bytes!("../../../data/cancer-research/pdc000711-hcmi-gbm-proteome-source-v1.json");
    const FILE_RESPONSE: &[u8] = include_bytes!("../testdata/pdc000711/files-response.json");
    const BIOSPECIMENS: &[u8] = include_bytes!("../testdata/pdc000711/biospecimens-response.json");
    const MATRIX: &[u8] = include_bytes!("../testdata/pdc000711/Global_all_original.txt");

    fn manifest() -> SourceManifest {
        let manifest: SourceManifest = serde_json::from_slice(MANIFEST).expect("manifest fixture");
        validate_manifest(&manifest).expect("valid checked-in manifest");
        manifest
    }

    fn fixture_biospecimens(manifest: &mut SourceManifest) -> Vec<Biospecimen> {
        manifest.expected_source_shape.biospecimen_records = 30;
        let envelope: BiospecimenEnvelope =
            serde_json::from_slice(BIOSPECIMENS).expect("biospecimen fixture");
        validated_biospecimens(envelope, manifest).expect("valid biospecimen fixture")
    }

    #[test]
    fn checked_in_manifest_pins_exact_file_and_cc_by_attribution() {
        let manifest = manifest();
        assert_eq!(manifest.file.file_size, SOURCE_FILE_SIZE);
        assert_eq!(manifest.file.md5, SOURCE_FILE_MD5);
        assert_eq!(manifest.license.spdx_identifier, "CC-BY-4.0");
        assert!(manifest.license.attribution.contains(STUDY_ID));

        let envelope: FileEnvelope =
            serde_json::from_slice(FILE_RESPONSE).expect("file response fixture");
        let file = validated_file_response(envelope, &manifest).expect("pinned PDC file");
        assert_eq!(file.study_id, STUDY_UUID);
        assert_eq!(file.file_id, SOURCE_FILE_ID);
    }

    #[test]
    fn biospecimen_fixture_selects_the_exact_30_gbm_models() {
        let mut manifest = manifest();
        let records = fixture_biospecimens(&mut manifest);
        let selected = selected_biospecimens(&records, &manifest);
        assert_eq!(selected.len(), 30);
        assert!(selected.iter().all(|record| {
            record.disease_type == "Glioblastoma" && record.primary_site == "Brain"
        }));
    }

    #[test]
    fn matrix_fixture_derives_30_source_order_columns_without_imputation() {
        let mut manifest = manifest();
        let biospecimens = fixture_biospecimens(&mut manifest);
        manifest.expected_source_shape.model_columns = 31;
        manifest.expected_source_shape.data_rows = 2;

        let first = derive_matrix(MATRIX, &biospecimens, &manifest).expect("derive fixture");
        let second = derive_matrix(MATRIX, &biospecimens, &manifest).expect("repeat fixture");
        assert_eq!(first.bytes, second.bytes);
        assert_eq!(first.joins.len(), 30);
        assert_eq!(first.rows, 2);
        assert_eq!(first.missing_cells, 2);
        assert_eq!(first.observed_cells, 58);
        let text = std::str::from_utf8(&first.bytes).expect("derived UTF-8");
        let header = text.lines().next().expect("derived header");
        assert_eq!(header.split('\t').count(), 34);
        assert!(header.starts_with("HCM-BROD-0002-C71\tHCM-BROD-0012-C71"));
        assert!(header.ends_with("T: Index\tT: NumberPSM\tT: ProteinID\tT: MaxPepProb"));
        assert!(text.contains("1-Mar\t7\tP111;P222\t0.99"));
        let second_row = text.lines().nth(2).expect("second data row");
        assert!(second_row.starts_with("\t2\t2"));
        assert!(second_row.contains("\t\t2-Sep\t5\tP333\t0.90"));
    }

    #[test]
    fn exact_source_verifier_rejects_wrong_size_before_md5() {
        let mut file = manifest().file;
        file.file_size = 4;
        file.md5 = hex::encode(Md5::digest(b"test"));
        verify_source_bytes(&file, b"test").expect("matching fixture bytes");
        let error = verify_source_bytes(&file, b"tests").expect_err("wrong size");
        assert!(error.to_string().contains("wrong byte length"));
    }
}
