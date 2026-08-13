use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use md5::Md5;
use reqwest::{
    Client, Response, StatusCode, Url,
    header::{ACCEPT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, RANGE},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use uuid::Uuid;

const MANIFEST_ID: &str = "edinburgh-ds8038-gsc-response-source-v1";
const ITEM_UUID: &str = "2bbcb530-4bd8-45d8-b0da-f1681352862a";
const HANDLE: &str = "10283/9113";
const DOI: &str = "10.7488/ds/8038";
const TITLE: &str = "High Content Drug Screening & Analysis Dataset: A comprehensive pharmacological survey across heterogeneous patient-derived glioblastoma stem cell models";
const RIGHTS: &str = "Creative Commons Attribution 4.0 International Public License";
const API_ORIGIN: &str = "https://datashare.ed.ac.uk";
const ITEM_API_URL: &str =
    "https://datashare.ed.ac.uk/server/api/core/items/2bbcb530-4bd8-45d8-b0da-f1681352862a";
const DISCOVERY_FILE: &str = "discovery.json";
const SNAPSHOT_FILE: &str = "source-snapshot.json";
const SOURCE_SET_DOMAIN: &str = "a-tiny-civilization/edinburgh-ds8038/source-set/v1";

#[derive(Clone, Copy)]
struct ExpectedArtifact {
    id: &'static str,
    name: &'static str,
    uuid: &'static str,
    md5: &'static str,
    maximum_byte_length: u64,
    acquire_in_v1: bool,
}

const EXPECTED_ARTIFACTS: [ExpectedArtifact; 13] = [
    ExpectedArtifact {
        id: "plate-readme-text",
        name: "1.README_Repository_HTS_Barcoding_QC_byPlate.txt",
        uuid: "af9a7cae-5383-4f49-86f8-eb3bd23b203f",
        md5: "b083f841429222de064e817eba6903fe",
        maximum_byte_length: 100_000,
        acquire_in_v1: true,
    },
    ExpectedArtifact {
        id: "plate-readme-workbook",
        name: "1.README_Repository_HTS_Barcoding_QC_byPlate.xlsx",
        uuid: "07b3a98e-c146-45d4-a79c-87942448521c",
        md5: "f702a2cdf76fb8458643824347588888",
        maximum_byte_length: 100_000,
        acquire_in_v1: false,
    },
    ExpectedArtifact {
        id: "c3l-high-content-screen",
        name: "2.C3L_HighContentScreening.zip",
        uuid: "185fddfa-a84d-42c4-bfc8-c08f5adbb216",
        md5: "5ebb3098257de06269feb913dea8d066",
        maximum_byte_length: 1_500_000_000,
        acquire_in_v1: false,
    },
    ExpectedArtifact {
        id: "lopac-high-content-screen",
        name: "3.LOPAC_HighContentScreening.zip",
        uuid: "bab1f1f4-ab54-4308-8589-bed19f75551b",
        md5: "77815306b58ec7c340706501bb9922c6",
        maximum_byte_length: 1_500_000_000,
        acquire_in_v1: false,
    },
    ExpectedArtifact {
        id: "prestwick-fda-high-content-screen",
        name: "4.PrestwickFDA_HighContentScreening.zip",
        uuid: "1b348cba-d2b7-44fa-8917-0a29601de819",
        md5: "6121ad88ecc2487c7150780cde3cb46b",
        maximum_byte_length: 1_200_000_000,
        acquire_in_v1: false,
    },
    ExpectedArtifact {
        id: "kcgs-high-content-screen",
        name: "5.KCGS_HighContentScreening.zip",
        uuid: "ec9f5a00-e60c-4b9e-b5f2-c0cd16181df0",
        md5: "fdba46256143ce0315c6728515883909",
        maximum_byte_length: 500_000_000,
        acquire_in_v1: false,
    },
    ExpectedArtifact {
        id: "targetmol-high-content-screen",
        name: "6.TargetMol_HighContentScreening.zip",
        uuid: "e84b1526-1e48-420e-a844-87fdaf971930",
        md5: "2e08066af7e6a96000b42320f7e8fca6",
        maximum_byte_length: 400_000_000,
        acquire_in_v1: false,
    },
    ExpectedArtifact {
        id: "hit-validation-high-content",
        name: "7.Hit_Validation_HighContent.zip",
        uuid: "7f7e626e-99fc-4f9d-9a89-19191943703d",
        md5: "bf5784ff589f523d4e4679aeba19f275",
        maximum_byte_length: 300_000_000,
        acquire_in_v1: true,
    },
    ExpectedArtifact {
        id: "qc-normalised-distributions",
        name: "8.QC_Normalised_Distributions.xlsx",
        uuid: "a98b30f8-f319-49fc-91bc-d25a01062d7a",
        md5: "b237b46fc85d70767bb79137f62944ed",
        maximum_byte_length: 5_000_000,
        acquire_in_v1: true,
    },
    ExpectedArtifact {
        id: "rppa-cytokine-antibodies",
        name: "ANTIBODIES_RPPA_Cytokine_array.csv",
        uuid: "617e9f13-c506-4aff-9b29-ca9480a2901e",
        md5: "3a8c8d8195e75d52a0eb03b004856558",
        maximum_byte_length: 100_000,
        acquire_in_v1: false,
    },
    ExpectedArtifact {
        id: "cellprofiler-pipeline",
        name: "GCGR_CellPainting_3.1.5.cpproj",
        uuid: "f4df60a5-9a31-4c0e-b193-95de65c5c8a1",
        md5: "2fac59f04b778d89a63f4319f26c5a9a",
        maximum_byte_length: 1_000_000,
        acquire_in_v1: true,
    },
    ExpectedArtifact {
        id: "auc-analysis-script",
        name: "AUC_script_Figure3B.Rmd",
        uuid: "f19a89df-a909-42cf-8f4e-6ca0b60dcbb1",
        md5: "afb6486a960eb6bdb376f2cdfe874f56",
        maximum_byte_length: 100_000,
        acquire_in_v1: true,
    },
    ExpectedArtifact {
        id: "license-text",
        name: "license_text",
        uuid: "209e6e37-ce06-48f0-8a06-0a411938a3ac",
        md5: "2946b37a07baeba80cea628909e28cae",
        maximum_byte_length: 100_000,
        acquire_in_v1: true,
    },
];

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SourceManifest {
    schema_version: u16,
    manifest_id: String,
    source: SourceIdentity,
    license: LicenseIdentity,
    discovery: DiscoveryPolicy,
    inventory: Vec<ArtifactManifest>,
    dataset_summary: DatasetSummary,
    intended_use: IntendedUse,
    split_firewall: SplitFirewall,
    limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SourceIdentity {
    title: String,
    doi: String,
    doi_url: String,
    handle: String,
    landing_url: String,
    full_record_url: String,
    item_uuid: String,
    item_api_url: String,
    related_article_doi: String,
    related_article_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LicenseIdentity {
    spdx_identifier: String,
    license_url: String,
    metadata_evidence_url: String,
    metadata_rights_value: String,
    license_bitstream_uuid: String,
    attribution: String,
    commercial_reuse_boundary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DiscoveryPolicy {
    api_origin: String,
    bitstream_api_template: String,
    download_template: String,
    identity_policy: String,
    byte_length_policy: String,
    metadata_response_max_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArtifactManifest {
    artifact_id: String,
    file_name: String,
    bitstream_uuid: String,
    api_url: String,
    download_url: String,
    expected_byte_length: Option<u64>,
    byte_length_resolution: String,
    maximum_byte_length: u64,
    md5: String,
    acquire_in_v1: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DatasetSummary {
    model_count: usize,
    model_ids: Vec<String>,
    transcriptomic_subtypes: BTreeMap<String, Vec<String>>,
    screened_compound_count: usize,
    primary_screen_hit_count: usize,
    dose_response_validated_compound_count: usize,
    validated_original_hit_count: usize,
    validated_target_class_alternative_count: usize,
    cellprofiler_version: String,
    approximate_features_per_cell_before_redundancy_filter: usize,
    replicate_images_per_well: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IntendedUse {
    evidence_class: String,
    allowed: String,
    prohibited: String,
    v1_subset: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SplitFirewall {
    primary_split_unit: String,
    fixed_release_gate: FixedReleaseGate,
    cross_validation: String,
    compound_generalization: String,
    inseparable_units: Vec<String>,
    normalization: String,
    qc: String,
    answer_key_boundary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FixedReleaseGate {
    training_lines: Vec<String>,
    calibration_line: String,
    untouched_final_line: String,
}

#[derive(Clone, Debug, Deserialize)]
struct DspaceItem {
    uuid: String,
    name: String,
    handle: String,
    #[serde(rename = "type")]
    kind: String,
    metadata: BTreeMap<String, Vec<DspaceMetadataValue>>,
}

#[derive(Clone, Debug, Deserialize)]
struct DspaceMetadataValue {
    value: String,
}

#[derive(Clone, Debug, Deserialize)]
struct DspaceBitstream {
    uuid: String,
    name: String,
    #[serde(rename = "sizeBytes")]
    size_bytes: u64,
    #[serde(rename = "checkSum")]
    checksum: DspaceChecksum,
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "_links")]
    links: DspaceBitstreamLinks,
}

#[derive(Clone, Debug, Deserialize)]
struct DspaceChecksum {
    #[serde(rename = "checkSumAlgorithm")]
    algorithm: String,
    value: String,
}

#[derive(Clone, Debug, Deserialize)]
struct DspaceBitstreamLinks {
    content: DspaceLink,
}

#[derive(Clone, Debug, Deserialize)]
struct DspaceLink {
    href: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DiscoverySnapshot {
    schema_version: u16,
    item_uuid: String,
    handle: String,
    title: String,
    doi: String,
    rights: String,
    artifacts: Vec<DiscoveredArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DiscoveredArtifact {
    artifact_id: String,
    file_name: String,
    bitstream_uuid: String,
    byte_length: u64,
    md5: String,
    content_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SourceSnapshot {
    schema_version: u16,
    snapshot_id: String,
    manifest_id: String,
    manifest_sha256: String,
    discovery: SnapshotFile,
    artifacts: Vec<SnapshotArtifact>,
    source_set_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SnapshotFile {
    file_name: String,
    byte_length: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SnapshotArtifact {
    artifact_id: String,
    file_name: String,
    bitstream_uuid: String,
    byte_length: u64,
    md5: String,
    sha256: String,
}

struct FileDigests {
    byte_length: u64,
    md5: String,
    sha256: String,
}

pub async fn acquire(manifest_path: &Path, output_directory: &Path) -> Result<()> {
    let (manifest, manifest_bytes) = read_manifest(manifest_path)?;
    let manifest_sha256 = sha256_bytes(&manifest_bytes);
    let snapshot_path = output_directory.join(SNAPSHOT_FILE);
    if snapshot_path.exists() {
        let snapshot = verify_source_directory(&manifest, &manifest_sha256, output_directory)?;
        println!(
            "verified Edinburgh DS8038 source snapshot {} ({} artifacts)",
            snapshot.source_set_sha256,
            snapshot.artifacts.len()
        );
        return Ok(());
    }

    fs::create_dir_all(output_directory).with_context(|| {
        format!(
            "create Edinburgh DS8038 source directory {}",
            output_directory.display()
        )
    })?;
    let client = Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::limited(3))
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(120))
        .user_agent("a-tiny-civilization-edinburgh-ds8038-acquisition/0.1")
        .build()
        .context("construct Edinburgh DataShare client")?;

    let discovery_path = output_directory.join(DISCOVERY_FILE);
    let (discovery, discovery_bytes) = if discovery_path.exists() {
        read_canonical_discovery(&discovery_path, &manifest)?
    } else {
        let discovered = discover(&client, &manifest).await?;
        let bytes = pretty_json_bytes(&discovered)?;
        write_new(&discovery_path, &bytes)?;
        (discovered, bytes)
    };

    for artifact in &discovery.artifacts {
        let declared = manifest_artifact(&manifest, &artifact.artifact_id)?;
        acquire_artifact(&client, declared, artifact, output_directory).await?;
    }

    let snapshot = build_snapshot(
        &manifest,
        &manifest_sha256,
        &discovery,
        &discovery_bytes,
        output_directory,
    )?;
    write_new(&snapshot_path, &pretty_json_bytes(&snapshot)?)?;
    println!(
        "acquired Edinburgh DS8038 hit-validation slice; source snapshot {}",
        snapshot.source_set_sha256
    );
    Ok(())
}

fn read_manifest(path: &Path) -> Result<(SourceManifest, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let manifest: SourceManifest =
        serde_json::from_slice(&bytes).with_context(|| format!("decode {}", path.display()))?;
    validate_manifest(&manifest)?;
    Ok((manifest, bytes))
}

fn validate_manifest(manifest: &SourceManifest) -> Result<()> {
    if manifest.schema_version != 1
        || manifest.manifest_id != MANIFEST_ID
        || manifest.source.title != TITLE
        || manifest.source.doi != DOI
        || manifest.source.doi_url != format!("https://doi.org/{DOI}")
        || manifest.source.handle != HANDLE
        || manifest.source.landing_url != format!("https://datashare.ed.ac.uk/handle/{HANDLE}")
        || manifest.source.full_record_url
            != format!("https://datashare.ed.ac.uk/handle/{HANDLE}?show=full")
        || manifest.source.item_uuid != ITEM_UUID
        || manifest.source.item_api_url != ITEM_API_URL
        || manifest.source.related_article_doi != "10.1016/j.isci.2026.115839"
        || manifest.source.related_article_url != "https://doi.org/10.1016/j.isci.2026.115839"
        || manifest.license.spdx_identifier != "CC-BY-4.0"
        || manifest.license.license_url != "https://creativecommons.org/licenses/by/4.0/"
        || manifest.license.metadata_evidence_url
            != format!("https://datashare.ed.ac.uk/handle/{HANDLE}?show=full")
        || manifest.license.metadata_rights_value != RIGHTS
        || manifest.license.license_bitstream_uuid != "209e6e37-ce06-48f0-8a06-0a411938a3ac"
        || manifest.discovery.api_origin != API_ORIGIN
        || manifest.discovery.bitstream_api_template
            != "https://datashare.ed.ac.uk/server/api/core/bitstreams/{bitstream_uuid}"
        || manifest.discovery.download_template
            != "https://datashare.ed.ac.uk/bitstreams/{bitstream_uuid}/download"
        || manifest.discovery.metadata_response_max_bytes != 2 * 1024 * 1024
    {
        bail!("unsupported or inconsistent Edinburgh DS8038 source manifest");
    }
    for (value, label) in [
        (&manifest.source.doi_url, "DOI URL"),
        (&manifest.source.landing_url, "landing URL"),
        (&manifest.source.full_record_url, "full record URL"),
        (&manifest.source.item_api_url, "item API URL"),
        (&manifest.source.related_article_url, "article URL"),
        (&manifest.license.license_url, "license URL"),
        (
            &manifest.license.metadata_evidence_url,
            "rights evidence URL",
        ),
    ] {
        require_https(value, label)?;
    }
    if manifest.license.attribution.trim().is_empty()
        || manifest.license.commercial_reuse_boundary.trim().is_empty()
        || manifest.discovery.identity_policy.trim().is_empty()
        || manifest.discovery.byte_length_policy.trim().is_empty()
        || manifest.intended_use.evidence_class.trim().is_empty()
        || manifest.intended_use.allowed.trim().is_empty()
        || manifest.intended_use.prohibited.trim().is_empty()
        || manifest.intended_use.v1_subset.trim().is_empty()
        || manifest.limitations.is_empty()
        || manifest
            .limitations
            .iter()
            .any(|value| value.trim().is_empty())
    {
        bail!("Edinburgh DS8038 manifest has empty provenance or scientific-boundary fields");
    }
    validate_inventory(manifest)?;
    validate_science_boundary(manifest)?;
    Ok(())
}

fn validate_inventory(manifest: &SourceManifest) -> Result<()> {
    if manifest.inventory.len() != EXPECTED_ARTIFACTS.len() {
        bail!("Edinburgh DS8038 inventory must pin exactly 13 bitstreams");
    }
    let mut ids = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut uuids = BTreeSet::new();
    for (artifact, expected) in manifest.inventory.iter().zip(EXPECTED_ARTIFACTS) {
        let expected_api = format!(
            "https://datashare.ed.ac.uk/server/api/core/bitstreams/{}",
            expected.uuid
        );
        let expected_download = format!(
            "https://datashare.ed.ac.uk/bitstreams/{}/download",
            expected.uuid
        );
        if artifact.artifact_id != expected.id
            || artifact.file_name != expected.name
            || artifact.bitstream_uuid != expected.uuid
            || artifact.api_url != expected_api
            || artifact.download_url != expected_download
            || artifact.expected_byte_length.is_some()
            || artifact.byte_length_resolution != "exact_uuid_api_sizeBytes"
            || artifact.maximum_byte_length != expected.maximum_byte_length
            || artifact.md5 != expected.md5
            || artifact.acquire_in_v1 != expected.acquire_in_v1
            || !is_lower_hex_md5(&artifact.md5)
            || Uuid::parse_str(&artifact.bitstream_uuid).is_err()
            || !is_safe_file_name(&artifact.file_name)
            || !ids.insert(&artifact.artifact_id)
            || !names.insert(&artifact.file_name)
            || !uuids.insert(&artifact.bitstream_uuid)
        {
            bail!(
                "Edinburgh DS8038 inventory entry {} differs from its pinned identity",
                artifact.artifact_id
            );
        }
        require_datashare_https(&artifact.api_url, "bitstream API URL")?;
        require_datashare_https(&artifact.download_url, "bitstream download URL")?;
    }
    if manifest
        .inventory
        .iter()
        .filter(|artifact| artifact.acquire_in_v1)
        .count()
        != 6
    {
        bail!("Edinburgh DS8038 v1 slice must contain exactly six artifacts");
    }
    Ok(())
}

fn validate_science_boundary(manifest: &SourceManifest) -> Result<()> {
    let summary = &manifest.dataset_summary;
    let expected_models = ["E13", "E21", "E28", "E31", "E34", "E57"];
    let expected_subtypes = BTreeMap::from([
        (
            "classical".to_owned(),
            vec!["E13".to_owned(), "E28".to_owned()],
        ),
        (
            "mesenchymal".to_owned(),
            vec!["E21".to_owned(), "E57".to_owned()],
        ),
        (
            "proneural".to_owned(),
            vec!["E31".to_owned(), "E34".to_owned()],
        ),
    ]);
    if summary.model_count != 6
        || summary.model_ids != expected_models
        || summary.transcriptomic_subtypes != expected_subtypes
        || summary.screened_compound_count != 3_866
        || summary.primary_screen_hit_count != 211
        || summary.dose_response_validated_compound_count != 164
        || summary.validated_original_hit_count != 143
        || summary.validated_target_class_alternative_count != 21
        || summary.cellprofiler_version != "3.1.5"
        || summary.approximate_features_per_cell_before_redundancy_filter != 1_006
        || summary.replicate_images_per_well != 6
    {
        bail!("Edinburgh DS8038 dataset summary differs from the admitted evidence slice");
    }
    let firewall = &manifest.split_firewall;
    if firewall.primary_split_unit != "patient_derived_gsc_line"
        || firewall.fixed_release_gate.training_lines != ["E13", "E21", "E28", "E31"]
        || firewall.fixed_release_gate.calibration_line != "E34"
        || firewall.fixed_release_gate.untouched_final_line != "E57"
        || firewall.cross_validation.trim().is_empty()
        || firewall.compound_generalization.trim().is_empty()
        || firewall.inseparable_units.len() < 3
        || firewall.normalization.trim().is_empty()
        || firewall.qc.trim().is_empty()
        || firewall.answer_key_boundary.trim().is_empty()
    {
        bail!("Edinburgh DS8038 split firewall is missing or inconsistent");
    }
    Ok(())
}

async fn discover(client: &Client, manifest: &SourceManifest) -> Result<DiscoverySnapshot> {
    let item_bytes = get_limited_exact(
        client,
        &manifest.source.item_api_url,
        manifest.discovery.metadata_response_max_bytes,
        "DataShare item metadata",
    )
    .await?;
    let item: DspaceItem =
        serde_json::from_slice(&item_bytes).context("decode DataShare item metadata")?;
    validate_item_response(&item, manifest)?;

    let mut artifacts = Vec::new();
    for declared in manifest
        .inventory
        .iter()
        .filter(|artifact| artifact.acquire_in_v1)
    {
        let bytes = get_limited_exact(
            client,
            &declared.api_url,
            manifest.discovery.metadata_response_max_bytes,
            "DataShare bitstream metadata",
        )
        .await?;
        let response: DspaceBitstream =
            serde_json::from_slice(&bytes).context("decode DataShare bitstream metadata")?;
        artifacts.push(validate_bitstream_response(&response, declared)?);
    }
    let discovery = DiscoverySnapshot {
        schema_version: 1,
        item_uuid: ITEM_UUID.to_owned(),
        handle: HANDLE.to_owned(),
        title: TITLE.to_owned(),
        doi: DOI.to_owned(),
        rights: RIGHTS.to_owned(),
        artifacts,
    };
    validate_discovery(&discovery, manifest)?;
    Ok(discovery)
}

fn validate_item_response(item: &DspaceItem, manifest: &SourceManifest) -> Result<()> {
    let values = item
        .metadata
        .values()
        .flatten()
        .map(|entry| entry.value.as_str())
        .collect::<BTreeSet<_>>();
    let has_doi = values.contains(DOI) || values.contains(&manifest.source.doi_url.as_str());
    if item.uuid != ITEM_UUID
        || item.name != TITLE
        || item.handle != HANDLE
        || item.kind != "item"
        || !values.contains(TITLE)
        || !values.contains(RIGHTS)
        || !has_doi
    {
        bail!("DataShare item metadata differs from the checked-in identity or license");
    }
    Ok(())
}

fn validate_bitstream_response(
    response: &DspaceBitstream,
    declared: &ArtifactManifest,
) -> Result<DiscoveredArtifact> {
    let expected_content = format!(
        "https://datashare.ed.ac.uk/server/api/core/bitstreams/{}/content",
        declared.bitstream_uuid
    );
    if response.uuid != declared.bitstream_uuid
        || response.name != declared.file_name
        || response.kind != "bitstream"
        || response.checksum.algorithm != "MD5"
        || response.checksum.value.to_ascii_lowercase() != declared.md5
        || response.size_bytes == 0
        || response.size_bytes > declared.maximum_byte_length
        || response.links.content.href != expected_content
    {
        bail!(
            "DataShare bitstream metadata differs from pinned artifact {}",
            declared.artifact_id
        );
    }
    require_datashare_https(&response.links.content.href, "bitstream content URL")?;
    Ok(DiscoveredArtifact {
        artifact_id: declared.artifact_id.clone(),
        file_name: declared.file_name.clone(),
        bitstream_uuid: declared.bitstream_uuid.clone(),
        byte_length: response.size_bytes,
        md5: declared.md5.clone(),
        content_url: response.links.content.href.clone(),
    })
}

fn validate_discovery(discovery: &DiscoverySnapshot, manifest: &SourceManifest) -> Result<()> {
    if discovery.schema_version != 1
        || discovery.item_uuid != ITEM_UUID
        || discovery.handle != HANDLE
        || discovery.title != TITLE
        || discovery.doi != DOI
        || discovery.rights != RIGHTS
    {
        bail!("Edinburgh DS8038 discovery identity is inconsistent");
    }
    let required = manifest
        .inventory
        .iter()
        .filter(|artifact| artifact.acquire_in_v1)
        .collect::<Vec<_>>();
    if discovery.artifacts.len() != required.len() {
        bail!("Edinburgh DS8038 discovery does not contain exactly the v1 artifact set");
    }
    for (found, declared) in discovery.artifacts.iter().zip(required) {
        let expected_content = format!(
            "https://datashare.ed.ac.uk/server/api/core/bitstreams/{}/content",
            declared.bitstream_uuid
        );
        if found.artifact_id != declared.artifact_id
            || found.file_name != declared.file_name
            || found.bitstream_uuid != declared.bitstream_uuid
            || found.md5 != declared.md5
            || found.byte_length == 0
            || found.byte_length > declared.maximum_byte_length
            || found.content_url != expected_content
        {
            bail!(
                "Edinburgh DS8038 discovery differs from pinned artifact {}",
                declared.artifact_id
            );
        }
    }
    Ok(())
}

fn read_canonical_discovery(
    path: &Path,
    manifest: &SourceManifest,
) -> Result<(DiscoverySnapshot, Vec<u8>)> {
    reject_symlink(path)?;
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let discovery: DiscoverySnapshot =
        serde_json::from_slice(&bytes).with_context(|| format!("decode {}", path.display()))?;
    if pretty_json_bytes(&discovery)? != bytes {
        bail!("Edinburgh DS8038 discovery is not canonical JSON");
    }
    validate_discovery(&discovery, manifest)?;
    Ok((discovery, bytes))
}

async fn acquire_artifact(
    client: &Client,
    declared: &ArtifactManifest,
    discovered: &DiscoveredArtifact,
    directory: &Path,
) -> Result<()> {
    let final_path = directory.join(&declared.file_name);
    if final_path.exists() {
        verify_artifact_file(&final_path, discovered)?;
        return Ok(());
    }
    let part_path = partial_path(directory, &declared.file_name);
    let part_existed = part_path.exists();
    let existing_length = if part_existed {
        reject_symlink(&part_path)?;
        fs::metadata(&part_path)
            .with_context(|| format!("inspect partial artifact {}", part_path.display()))?
            .len()
    } else {
        0
    };
    if existing_length > discovered.byte_length {
        bail!(
            "partial artifact {} exceeds the discovered exact byte length",
            part_path.display()
        );
    }
    if existing_length < discovered.byte_length {
        download_remaining(
            client,
            declared,
            discovered,
            &part_path,
            existing_length,
            part_existed,
        )
        .await?;
    }
    verify_artifact_file(&part_path, discovered)?;
    fs::hard_link(&part_path, &final_path).with_context(|| {
        format!(
            "publish create-only artifact {} from {}",
            final_path.display(),
            part_path.display()
        )
    })?;
    fs::remove_file(&part_path)
        .with_context(|| format!("remove completed partial artifact {}", part_path.display()))?;
    Ok(())
}

async fn download_remaining(
    client: &Client,
    declared: &ArtifactManifest,
    discovered: &DiscoveredArtifact,
    part_path: &Path,
    offset: u64,
    part_existed: bool,
) -> Result<()> {
    let mut request = client
        .get(&declared.download_url)
        .header(ACCEPT_ENCODING, "identity");
    if offset > 0 {
        request = request.header(RANGE, format!("bytes={offset}-"));
    }
    let mut response = request
        .send()
        .await
        .with_context(|| format!("request DataShare artifact {}", declared.artifact_id))?;
    validate_download_response(&response, declared, discovered, offset)?;

    let mut file = if part_existed {
        OpenOptions::new().append(true).open(part_path)
    } else {
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(part_path)
    }
    .with_context(|| format!("open partial artifact {}", part_path.display()))?;
    let mut written = offset;
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("stream DataShare artifact {}", declared.artifact_id))?
    {
        written = written
            .checked_add(u64::try_from(chunk.len())?)
            .context("DataShare artifact length overflow")?;
        if written > discovered.byte_length {
            bail!(
                "DataShare artifact {} exceeded its discovered exact byte length",
                declared.artifact_id
            );
        }
        file.write_all(&chunk)?;
    }
    file.sync_all()?;
    if written != discovered.byte_length {
        bail!(
            "DataShare artifact {} ended at {written} bytes, expected {}",
            declared.artifact_id,
            discovered.byte_length
        );
    }
    Ok(())
}

fn validate_download_response(
    response: &Response,
    declared: &ArtifactManifest,
    discovered: &DiscoveredArtifact,
    offset: u64,
) -> Result<()> {
    require_allowed_download_url(response.url(), declared, discovered)?;
    let remaining = discovered
        .byte_length
        .checked_sub(offset)
        .context("download offset exceeds exact byte length")?;
    if response
        .content_length()
        .is_some_and(|length| length != remaining)
    {
        bail!(
            "DataShare artifact {} response length differs from {remaining}",
            declared.artifact_id
        );
    }
    if offset == 0 {
        if response.status() != StatusCode::OK {
            bail!(
                "DataShare artifact {} returned {}, expected 200",
                declared.artifact_id,
                response.status()
            );
        }
        if response.headers().contains_key(CONTENT_RANGE) {
            bail!("fresh DataShare download unexpectedly returned Content-Range");
        }
    } else {
        if response.status() != StatusCode::PARTIAL_CONTENT {
            bail!(
                "resumed DataShare artifact {} returned {}, expected 206",
                declared.artifact_id,
                response.status()
            );
        }
        let value = response
            .headers()
            .get(CONTENT_RANGE)
            .context("resumed DataShare response omitted Content-Range")?
            .to_str()
            .context("DataShare Content-Range is not ASCII")?;
        validate_content_range(value, offset, discovered.byte_length)?;
    }
    if let Some(raw) = response.headers().get(CONTENT_LENGTH) {
        let header_length = raw
            .to_str()
            .context("DataShare Content-Length is not ASCII")?
            .parse::<u64>()
            .context("DataShare Content-Length is not an integer")?;
        if header_length != remaining {
            bail!("DataShare Content-Length differs from the required remaining length");
        }
    }
    Ok(())
}

fn validate_content_range(value: &str, start: u64, total: u64) -> Result<()> {
    let body = value
        .strip_prefix("bytes ")
        .context("DataShare Content-Range must use bytes")?;
    let (range, parsed_total) = body
        .split_once('/')
        .context("DataShare Content-Range has no total")?;
    let (parsed_start, parsed_end) = range
        .split_once('-')
        .context("DataShare Content-Range has no end")?;
    let parsed_start = parsed_start.parse::<u64>()?;
    let parsed_end = parsed_end.parse::<u64>()?;
    let parsed_total = parsed_total.parse::<u64>()?;
    if parsed_start != start
        || parsed_total != total
        || parsed_end != total.checked_sub(1).context("zero total byte length")?
        || parsed_end < parsed_start
    {
        bail!("DataShare Content-Range differs from the requested exact tail");
    }
    Ok(())
}

fn require_allowed_download_url(
    url: &Url,
    declared: &ArtifactManifest,
    discovered: &DiscoveredArtifact,
) -> Result<()> {
    require_datashare_url(url, "final DataShare download URL")?;
    let declared_url = Url::parse(&declared.download_url)?;
    let content_url = Url::parse(&discovered.content_url)?;
    if url.path() != declared_url.path() && url.path() != content_url.path() {
        bail!("DataShare download redirected to an unpinned path");
    }
    Ok(())
}

fn build_snapshot(
    manifest: &SourceManifest,
    manifest_sha256: &str,
    discovery: &DiscoverySnapshot,
    discovery_bytes: &[u8],
    directory: &Path,
) -> Result<SourceSnapshot> {
    validate_discovery(discovery, manifest)?;
    let mut artifacts = Vec::new();
    for discovered in &discovery.artifacts {
        let path = directory.join(&discovered.file_name);
        let digests = verify_artifact_file(&path, discovered)?;
        artifacts.push(SnapshotArtifact {
            artifact_id: discovered.artifact_id.clone(),
            file_name: discovered.file_name.clone(),
            bitstream_uuid: discovered.bitstream_uuid.clone(),
            byte_length: digests.byte_length,
            md5: digests.md5,
            sha256: digests.sha256,
        });
    }
    let discovery_sha256 = sha256_bytes(discovery_bytes);
    let source_set_sha256 = source_set_digest(manifest_sha256, &discovery_sha256, &artifacts);
    Ok(SourceSnapshot {
        schema_version: 1,
        snapshot_id: format!("edinburgh-ds8038:{source_set_sha256}"),
        manifest_id: MANIFEST_ID.to_owned(),
        manifest_sha256: manifest_sha256.to_owned(),
        discovery: SnapshotFile {
            file_name: DISCOVERY_FILE.to_owned(),
            byte_length: u64::try_from(discovery_bytes.len())?,
            sha256: discovery_sha256,
        },
        artifacts,
        source_set_sha256,
    })
}

fn verify_source_directory(
    manifest: &SourceManifest,
    manifest_sha256: &str,
    directory: &Path,
) -> Result<SourceSnapshot> {
    let (discovery, discovery_bytes) =
        read_canonical_discovery(&directory.join(DISCOVERY_FILE), manifest)?;
    let expected = build_snapshot(
        manifest,
        manifest_sha256,
        &discovery,
        &discovery_bytes,
        directory,
    )?;
    let snapshot_path = directory.join(SNAPSHOT_FILE);
    reject_symlink(&snapshot_path)?;
    let snapshot_bytes =
        fs::read(&snapshot_path).context("read Edinburgh DS8038 source snapshot")?;
    let retained: SourceSnapshot = serde_json::from_slice(&snapshot_bytes)
        .context("decode Edinburgh DS8038 source snapshot")?;
    if pretty_json_bytes(&retained)? != snapshot_bytes
        || pretty_json_bytes(&retained)? != pretty_json_bytes(&expected)?
    {
        bail!("Edinburgh DS8038 source snapshot is noncanonical or inconsistent");
    }
    Ok(retained)
}

fn verify_artifact_file(path: &Path, artifact: &DiscoveredArtifact) -> Result<FileDigests> {
    reject_symlink(path)?;
    let digests = file_digests(path)?;
    if digests.byte_length != artifact.byte_length {
        bail!(
            "artifact {} has byte length {}, expected {}",
            artifact.artifact_id,
            digests.byte_length,
            artifact.byte_length
        );
    }
    if digests.md5 != artifact.md5 {
        bail!("artifact {} failed its pinned MD5", artifact.artifact_id);
    }
    Ok(digests)
}

fn file_digests(path: &Path) -> Result<FileDigests> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut md5 = Md5::new();
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
            .context("artifact byte length overflow")?;
        md5.update(&buffer[..count]);
        sha256.update(&buffer[..count]);
    }
    Ok(FileDigests {
        byte_length,
        md5: hex::encode(md5.finalize()),
        sha256: hex::encode(sha256.finalize()),
    })
}

async fn get_limited_exact(client: &Client, url: &str, limit: u64, label: &str) -> Result<Vec<u8>> {
    require_datashare_https(url, label)?;
    let requested = Url::parse(url)?;
    let response = client
        .get(url)
        .header(ACCEPT_ENCODING, "identity")
        .send()
        .await
        .with_context(|| format!("request {label}"))?;
    if response.status() != StatusCode::OK {
        bail!("{label} returned {}", response.status());
    }
    if response.url() != &requested {
        bail!("{label} redirected away from its exact pinned URL");
    }
    read_limited(response, limit, label).await
}

async fn read_limited(mut response: Response, limit: u64, label: &str) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit)
    {
        bail!("{label} exceeds its bounded response size");
    }
    let mut bytes = Vec::new();
    let mut length = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .with_context(|| format!("read {label}"))?
    {
        length = length
            .checked_add(u64::try_from(chunk.len())?)
            .context("metadata response length overflow")?;
        if length > limit {
            bail!("{label} exceeds its bounded response size");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn manifest_artifact<'a>(
    manifest: &'a SourceManifest,
    artifact_id: &str,
) -> Result<&'a ArtifactManifest> {
    manifest
        .inventory
        .iter()
        .find(|artifact| artifact.artifact_id == artifact_id && artifact.acquire_in_v1)
        .with_context(|| format!("discovery references undeclared artifact {artifact_id}"))
}

fn source_set_digest(
    manifest_sha256: &str,
    discovery_sha256: &str,
    artifacts: &[SnapshotArtifact],
) -> String {
    let mut digest = Sha256::new();
    digest.update(SOURCE_SET_DOMAIN.as_bytes());
    digest.update([0]);
    digest.update(manifest_sha256.as_bytes());
    digest.update([0]);
    digest.update(discovery_sha256.as_bytes());
    for artifact in artifacts {
        digest.update([0]);
        digest.update(artifact.artifact_id.as_bytes());
        digest.update([0]);
        digest.update(artifact.byte_length.to_string().as_bytes());
        digest.update([0]);
        digest.update(artifact.md5.as_bytes());
        digest.update([0]);
        digest.update(artifact.sha256.as_bytes());
    }
    hex::encode(digest.finalize())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn pretty_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).context("encode canonical JSON")?;
    bytes.push(b'\n');
    Ok(bytes)
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

fn partial_path(directory: &Path, name: &str) -> PathBuf {
    directory.join(format!("{name}.part"))
}

fn reject_symlink(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect retained artifact {}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        bail!(
            "retained artifact {} must be a regular file",
            path.display()
        );
    }
    Ok(())
}

fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

fn is_lower_hex_md5(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn require_https(value: &str, label: &str) -> Result<()> {
    let url = Url::parse(value).with_context(|| format!("parse {label}"))?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
        bail!("{label} must be an absolute HTTPS URL without user information");
    }
    Ok(())
}

fn require_datashare_https(value: &str, label: &str) -> Result<()> {
    let url = Url::parse(value).with_context(|| format!("parse {label}"))?;
    require_datashare_url(&url, label)
}

fn require_datashare_url(url: &Url, label: &str) -> Result<()> {
    if url.scheme() != "https"
        || url.host_str() != Some("datashare.ed.ac.uk")
        || !url.username().is_empty()
    {
        bail!("{label} must remain on the HTTPS DataShare origin");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] = include_bytes!(
        "../../../data/cancer-research/edinburgh-ds8038-gsc-response-source-v1.json"
    );
    const ITEM_RESPONSE: &[u8] = include_bytes!("../testdata/edinburgh-ds8038/item-response.json");
    const BITSTREAM_RESPONSES: &[u8] =
        include_bytes!("../testdata/edinburgh-ds8038/bitstreams-response.json");

    fn manifest() -> SourceManifest {
        let manifest: SourceManifest = serde_json::from_slice(MANIFEST).expect("manifest fixture");
        validate_manifest(&manifest).expect("valid Edinburgh manifest");
        manifest
    }

    fn bitstreams() -> Vec<DspaceBitstream> {
        serde_json::from_slice(BITSTREAM_RESPONSES).expect("bitstream fixtures")
    }

    #[test]
    fn checked_in_manifest_pins_full_inventory_open_license_and_split_firewall() {
        let manifest = manifest();
        assert_eq!(manifest.inventory.len(), 13);
        assert_eq!(
            manifest
                .inventory
                .iter()
                .filter(|artifact| artifact.acquire_in_v1)
                .count(),
            6
        );
        assert_eq!(manifest.license.spdx_identifier, "CC-BY-4.0");
        assert_eq!(
            manifest.split_firewall.fixed_release_gate.training_lines,
            ["E13", "E21", "E28", "E31"]
        );
        assert_eq!(
            manifest
                .split_firewall
                .fixed_release_gate
                .untouched_final_line,
            "E57"
        );
    }

    #[test]
    fn item_fixture_pins_doi_title_and_rights() {
        let manifest = manifest();
        let item: DspaceItem = serde_json::from_slice(ITEM_RESPONSE).expect("item fixture");
        validate_item_response(&item, &manifest).expect("exact item evidence");
    }

    #[test]
    fn bitstream_fixtures_pin_the_exact_six_in_stable_order() {
        let manifest = manifest();
        let declared = manifest
            .inventory
            .iter()
            .filter(|artifact| artifact.acquire_in_v1)
            .collect::<Vec<_>>();
        let responses = bitstreams();
        assert_eq!(responses.len(), declared.len());
        for (response, artifact) in responses.iter().zip(declared) {
            let discovered =
                validate_bitstream_response(response, artifact).expect("pinned bitstream");
            assert_eq!(discovered.artifact_id, artifact.artifact_id);
            assert!(discovered.byte_length > 0);
        }
    }

    #[test]
    fn bitstream_validation_rejects_wrong_checksum_size_and_duplicate_discovery() {
        let manifest = manifest();
        let declared = manifest
            .inventory
            .iter()
            .find(|artifact| artifact.acquire_in_v1)
            .expect("v1 artifact");
        let mut response = bitstreams().remove(0);
        response.checksum.value = "00000000000000000000000000000000".to_owned();
        assert!(validate_bitstream_response(&response, declared).is_err());
        response.checksum.value = declared.md5.clone();
        response.size_bytes = 0;
        assert!(validate_bitstream_response(&response, declared).is_err());
        response.size_bytes = declared.maximum_byte_length + 1;
        assert!(validate_bitstream_response(&response, declared).is_err());

        let responses = bitstreams();
        let required = manifest
            .inventory
            .iter()
            .filter(|artifact| artifact.acquire_in_v1)
            .collect::<Vec<_>>();
        let mut artifacts = responses
            .iter()
            .zip(required)
            .map(|(value, artifact)| {
                validate_bitstream_response(value, artifact).expect("fixture discovery")
            })
            .collect::<Vec<_>>();
        artifacts[1] = artifacts[0].clone();
        let discovery = DiscoverySnapshot {
            schema_version: 1,
            item_uuid: ITEM_UUID.to_owned(),
            handle: HANDLE.to_owned(),
            title: TITLE.to_owned(),
            doi: DOI.to_owned(),
            rights: RIGHTS.to_owned(),
            artifacts,
        };
        assert!(validate_discovery(&discovery, &manifest).is_err());
    }

    #[test]
    fn content_range_must_cover_the_exact_requested_tail() {
        validate_content_range("bytes 40-99/100", 40, 100).expect("exact tail");
        assert!(validate_content_range("bytes 40-98/100", 40, 100).is_err());
        assert!(validate_content_range("bytes 0-99/100", 40, 100).is_err());
        assert!(validate_content_range("items 40-99/100", 40, 100).is_err());
    }

    #[test]
    fn streaming_integrity_and_create_only_writes_are_enforced() {
        let directory = std::env::temp_dir().join(format!(
            "a-tiny-civilization-edinburgh-test-{}",
            Uuid::new_v4()
        ));
        fs::create_dir(&directory).expect("test directory");
        let path = directory.join("artifact.bin");
        write_new(&path, b"test").expect("first create");
        assert!(write_new(&path, b"changed").is_err());
        let artifact = DiscoveredArtifact {
            artifact_id: "fixture".to_owned(),
            file_name: "artifact.bin".to_owned(),
            bitstream_uuid: Uuid::new_v4().to_string(),
            byte_length: 4,
            md5: "098f6bcd4621d373cade4e832627b4f6".to_owned(),
            content_url: "https://datashare.ed.ac.uk/server/api/core/bitstreams/fixture/content"
                .to_owned(),
        };
        let digests = verify_artifact_file(&path, &artifact).expect("matching bytes");
        assert_eq!(
            digests.sha256,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
        fs::remove_dir_all(directory).expect("remove test directory");
    }
}
