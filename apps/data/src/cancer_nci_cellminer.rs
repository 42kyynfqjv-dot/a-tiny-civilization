use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{Cursor, Read, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};
use calamine::{Data, Reader, Xlsx};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use world_data::CancerDatasetRegistry;
use world_domain::Digest;
use zip::ZipArchive;

const CELLMINER_ROOT: &str = "https://discover.nci.nih.gov";
const SINGLE_FILE: &str = "DTP_NCI60_ZSCORE.zip";
const SINGLE_INNER: &str = "output/DTP_NCI60_ZSCORE.xlsx";
const COMBO_FILE: &str = "DTP_NCI60_ALMANAC_COMBO_SCORE.zip";
const COMBO_INNER: &str = "output/DTP_NCI60_ALMANAC_COMBO_SCORE.xlsx";
const SINGLE_SOURCE_ID: &str = "nci-cellminer-nci60";
const COMBO_SOURCE_ID: &str = "nci-cellminer-almanac";
const SINGLE_SPLIT_DOMAIN: &str =
    "a-tiny-civilization/nci-cellminer/single-agent-compound-split/v1";
const COMBO_SPLIT_DOMAIN: &str = "a-tiny-civilization/nci-cellminer/almanac-pair-split/v1";
const CHALLENGE_SET_DOMAIN: &str =
    "a-tiny-civilization/nci-cellminer/cns-response-challenge-set/v1";
const CHALLENGE_ANSWER_DOMAIN: &str =
    "a-tiny-civilization/nci-cellminer/cns-response-challenge-answers/v1";
const MIN_MECHANISM_SUPPORT: usize = 3;
const MAX_DOWNLOAD_BYTES: u64 = 64 * 1024 * 1024;
const CNS_LINES: [&str; 6] = [
    "CNS:SF-268",
    "CNS:SF-295",
    "CNS:SF-539",
    "CNS:SNB-19",
    "CNS:SNB-75",
    "CNS:U251",
];

#[derive(Clone, Copy)]
struct ArtifactSpec {
    artifact_id: &'static str,
    file_name: &'static str,
    inner_name: &'static str,
    url_path: &'static str,
}

const ARTIFACTS: [ArtifactSpec; 2] = [
    ArtifactSpec {
        artifact_id: "nci60-average-z-score",
        file_name: SINGLE_FILE,
        inner_name: SINGLE_INNER,
        url_path: "/cellminer/download/processeddataset/DTP_NCI60_ZSCORE.zip",
    },
    ArtifactSpec {
        artifact_id: "nci-almanac-combo-score",
        file_name: COMBO_FILE,
        inner_name: COMBO_INNER,
        url_path: "/cellminer/download/processeddataset/DTP_NCI60_ALMANAC_COMBO_SCORE.zip",
    },
];

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AcquisitionManifest {
    schema_version: u16,
    source: String,
    cellminer_database_version: String,
    export_date: String,
    artifacts: Vec<AcquiredArtifact>,
    source_set_hash: Digest,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct AcquiredArtifact {
    artifact_id: String,
    url: String,
    file_name: String,
    inner_workbook: String,
    byte_length: u64,
    sha256: Digest,
}

#[derive(Debug, Serialize)]
struct CellminerBaseline {
    schema_version: u16,
    baseline_id: String,
    evidence_class: String,
    intended_use: String,
    source_registry_hash: Digest,
    source: BaselineSource,
    single_agent: SingleAgentSummary,
    combinations: CombinationSummary,
    limitations: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct BaselineSource {
    custodian: String,
    cellminer_database_version: String,
    export_date: String,
    source_set_hash: Digest,
    artifacts: Vec<AcquiredArtifact>,
}

#[derive(Debug, Serialize)]
struct SingleAgentSummary {
    source_id: String,
    response_measure: String,
    cns_cell_lines: Vec<String>,
    compound_count: usize,
    compounds_with_known_mechanism: usize,
    fda_approved_compound_count: usize,
    observed_cns_response_count: usize,
    split: SplitSummary,
    held_out_assessment: PredictorAssessment,
    top_observed_fda_approved_cns_profiles: Vec<ObservedSingleProfile>,
}

#[derive(Debug, Serialize)]
struct CombinationSummary {
    source_id: String,
    response_measure: String,
    cns_cell_lines: Vec<String>,
    source_drug_pair_record_count: usize,
    drug_pair_count: usize,
    repeated_canonical_pair_record_count: usize,
    observed_cns_combo_score_count: usize,
    positive_cns_combo_score_count: usize,
    split: SplitSummary,
    held_out_assessment: ComboPredictorAssessment,
    top_observed_cns_combo_profiles: Vec<ObservedComboProfile>,
}

#[derive(Debug, Serialize)]
struct SplitSummary {
    unit: String,
    derivation_domain: String,
    rule: String,
    calibration_unit_count: usize,
    held_out_unit_count: usize,
    calibration_set_commitment: Digest,
    held_out_set_commitment: Digest,
}

#[derive(Debug, Serialize)]
struct PredictorAssessment {
    predictor: String,
    baseline: String,
    minimum_calibration_group_support: usize,
    eligible_held_out_observation_count: usize,
    evaluated_held_out_observation_count: usize,
    evaluated_held_out_compound_count: usize,
    coverage_parts_per_million: u32,
    predictor_mean_absolute_error_z_milli: u64,
    baseline_mean_absolute_error_z_milli: u64,
    relative_mae_improvement_parts_per_million: i64,
}

#[derive(Debug, Serialize)]
struct ComboPredictorAssessment {
    predictor: String,
    baselines: Vec<String>,
    minimum_calibration_group_support: usize,
    eligible_held_out_observation_count: usize,
    evaluated_held_out_observation_count: usize,
    evaluated_held_out_pair_count: usize,
    coverage_parts_per_million: u32,
    predictor_mean_absolute_error_score_milli: u64,
    no_interaction_zero_mean_absolute_error_score_milli: u64,
    calibration_line_median_mean_absolute_error_score_milli: u64,
}

#[derive(Debug, Serialize)]
struct ObservedSingleProfile {
    nsc: u64,
    drug_name: String,
    mechanism: Option<String>,
    observed_cns_line_count: usize,
    mean_activity_z_milli: i64,
}

#[derive(Debug, Serialize)]
struct ObservedComboProfile {
    nsc_1: u64,
    drug_name_1: String,
    nsc_2: u64,
    drug_name_2: String,
    source_record_count: usize,
    observed_cns_line_count: usize,
    mean_combo_score_milli: i64,
}

#[derive(Debug, Serialize)]
struct CellminerChallengeCatalogue {
    schema_version: u16,
    catalogue_id: String,
    evidence_class: String,
    intended_use: String,
    source_registry_hash: Digest,
    source: BaselineSource,
    cns_cell_lines: Vec<String>,
    single_agent_partition: ChallengePartition,
    combination_partition: ChallengePartition,
    single_agent_candidates: Vec<SingleAgentCandidate>,
    combination_candidates: Vec<CombinationCandidate>,
    leakage_boundary: CatalogueLeakageBoundary,
    limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ChallengePartition {
    source_id: String,
    split: SplitSummary,
    eligibility_rule: String,
    candidate_count: usize,
    candidate_set_commitment: Digest,
}

#[derive(Debug, Serialize)]
struct CatalogueLeakageBoundary {
    access_class: String,
    allowed_in_model_context: bool,
    contains_observed_response_values: bool,
    contains_derived_rank_labels: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ChallengeCompound {
    nsc: u64,
    drug_name: String,
    mechanism: Option<String>,
    fda_approved: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SingleAgentCandidate {
    challenge_id: String,
    compound: ChallengeCompound,
}

#[derive(Debug, Serialize)]
struct CombinationCandidate {
    challenge_id: String,
    first: ChallengeCompound,
    second: ChallengeCompound,
    source_record_count: usize,
}

#[derive(Debug, Serialize)]
struct CellminerChallengeAnswerKey {
    schema_version: u16,
    answer_key_id: String,
    evidence_class: String,
    intended_use: String,
    source_registry_hash: Digest,
    source: BaselineSource,
    cns_cell_lines: Vec<String>,
    ranking_rule: String,
    single_agent_response_measure: String,
    combination_response_measure: String,
    catalogue_reference: ChallengeCatalogueReference,
    single_agent_answers: Vec<SingleAgentAnswer>,
    combination_answers: Vec<CombinationAnswer>,
    leakage_boundary: AnswerKeyLeakageBoundary,
    limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ChallengeCatalogueReference {
    catalogue_id: String,
    catalogue_artifact_sha256: Digest,
    answer_payload_commitment: Digest,
}

#[derive(Debug, Serialize)]
struct AnswerKeyLeakageBoundary {
    access_class: String,
    allowed_in_model_context: bool,
    contains_observed_response_values: bool,
    contains_derived_rank_labels: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct SingleAgentAnswer {
    challenge_id: String,
    nsc: u64,
    observations: Vec<SingleAgentObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct SingleAgentObservation {
    cell_line: String,
    activity_z_milli: i64,
    descending_response_rank: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CombinationAnswer {
    challenge_id: String,
    nsc_1: u64,
    nsc_2: u64,
    observations: Vec<CombinationObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct CombinationObservation {
    cell_line: String,
    combo_score_milli: i64,
    descending_interaction_rank: u8,
    interaction_direction: InteractionDirection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum InteractionDirection {
    Negative,
    Zero,
    Positive,
}

#[derive(Serialize)]
struct ChallengeAnswerPayload<'a> {
    domain: &'static str,
    single_agent_answers: &'a [SingleAgentAnswer],
    combination_answers: &'a [CombinationAnswer],
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WorkbookMetadata {
    database_version: String,
    export_date: String,
}

#[derive(Debug)]
struct SingleDrug {
    nsc: u64,
    name: String,
    fda_approved: bool,
    mechanism: Option<String>,
    cns: [Option<i64>; 6],
}

#[derive(Debug)]
struct DrugPair {
    nsc_1: u64,
    name_1: String,
    mechanism_1: Option<String>,
    nsc_2: u64,
    name_2: String,
    mechanism_2: Option<String>,
    cns: [Option<i64>; 6],
    source_record_count: usize,
}

#[derive(Debug)]
struct DrugPairAccumulator {
    name_1: String,
    mechanism_1: Option<String>,
    name_2: String,
    mechanism_2: Option<String>,
    cns: [Vec<i64>; 6],
    source_record_count: usize,
}

pub async fn acquire(output_directory: &Path) -> Result<()> {
    fs::create_dir_all(output_directory).with_context(|| {
        format!(
            "create CellMiner source directory {}",
            output_directory.display()
        )
    })?;
    let manifest_path = output_directory.join("acquisition.json");
    if manifest_path.exists() {
        let manifest: AcquisitionManifest = read_json(&manifest_path)?;
        verify_manifest(output_directory, &manifest)?;
        println!(
            "verified CellMiner {} export {} ({} artifacts)",
            manifest.cellminer_database_version,
            manifest.export_date,
            manifest.artifacts.len()
        );
        return Ok(());
    }

    let client = Client::builder()
        .https_only(true)
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(120))
        .user_agent("a-tiny-civilization-cellminer-acquisition/0.1")
        .build()
        .context("construct CellMiner client")?;
    let mut acquired = Vec::new();
    let mut metadata = None;
    for spec in ARTIFACTS {
        let path = output_directory.join(spec.file_name);
        if !path.exists() {
            let url = format!("{CELLMINER_ROOT}{}", spec.url_path);
            let bytes = download_limited(&client, &url).await?;
            write_new(&path, &bytes)?;
        }
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        let workbook = workbook_bytes(&bytes, spec.inner_name)?;
        let artifact_metadata = read_workbook_metadata(&workbook)?;
        if let Some(expected) = &metadata {
            if expected != &artifact_metadata {
                bail!("CellMiner workbooks report different export versions");
            }
        } else {
            metadata = Some(artifact_metadata);
        }
        acquired.push(AcquiredArtifact {
            artifact_id: spec.artifact_id.to_owned(),
            url: format!("{CELLMINER_ROOT}{}", spec.url_path),
            file_name: spec.file_name.to_owned(),
            inner_workbook: spec.inner_name.to_owned(),
            byte_length: u64::try_from(bytes.len())?,
            sha256: Digest::sha256(&bytes),
        });
    }
    let metadata = metadata.context("CellMiner acquisition contained no workbooks")?;
    let manifest = AcquisitionManifest {
        schema_version: 1,
        source: "National Cancer Institute CellMiner".to_owned(),
        cellminer_database_version: metadata.database_version,
        export_date: metadata.export_date,
        source_set_hash: artifact_set_hash(&acquired),
        artifacts: acquired,
    };
    write_json_new(&manifest_path, &manifest)?;
    println!(
        "acquired CellMiner {} export {} ({} artifacts)",
        manifest.cellminer_database_version,
        manifest.export_date,
        manifest.artifacts.len()
    );
    Ok(())
}

pub fn derive_baseline(source_directory: &Path, registry_path: &Path, output: &Path) -> Result<()> {
    let registry_bytes = fs::read(registry_path)
        .with_context(|| format!("read registry {}", registry_path.display()))?;
    let registry = CancerDatasetRegistry::from_slice(&registry_bytes)
        .context("validate Cancer World dataset registry")?;
    for required in [SINGLE_SOURCE_ID, COMBO_SOURCE_ID] {
        if !registry
            .sources
            .iter()
            .any(|source| source.source_id == required)
        {
            bail!("Cancer World registry does not contain {required}");
        }
    }
    let manifest: AcquisitionManifest = read_json(&source_directory.join("acquisition.json"))?;
    verify_manifest(source_directory, &manifest)?;

    let single_bytes = verified_artifact(source_directory, &manifest, "nci60-average-z-score")?;
    let combo_bytes = verified_artifact(source_directory, &manifest, "nci-almanac-combo-score")?;
    let single_workbook = workbook_bytes(&single_bytes, SINGLE_INNER)?;
    let combo_workbook = workbook_bytes(&combo_bytes, COMBO_INNER)?;
    let (single_metadata, single_drugs) = read_single_drugs(&single_workbook)?;
    let (combo_metadata, pairs) = read_drug_pairs(&combo_workbook)?;
    let expected_metadata = WorkbookMetadata {
        database_version: manifest.cellminer_database_version.clone(),
        export_date: manifest.export_date.clone(),
    };
    if single_metadata != expected_metadata || combo_metadata != expected_metadata {
        bail!("CellMiner workbook metadata differs from acquisition manifest");
    }

    let baseline = CellminerBaseline {
        schema_version: 1,
        baseline_id: format!(
            "nci-cellminer-{}-cns-response-baseline-v1",
            manifest.cellminer_database_version.replace('.', "-")
        ),
        evidence_class: "in_vitro_immortalized_cell_line_response".to_owned(),
        intended_use: "Leakage-resistant reference checks for Cancer World single-agent and combination response predictions in the NCI-60 CNS panel; not patient efficacy estimation.".to_owned(),
        source_registry_hash: registry.content_digest()?,
        source: BaselineSource {
            custodian: manifest.source,
            cellminer_database_version: manifest.cellminer_database_version,
            export_date: manifest.export_date,
            source_set_hash: manifest.source_set_hash,
            artifacts: manifest.artifacts,
        },
        single_agent: summarize_single(&single_drugs)?,
        combinations: summarize_combinations(&pairs)?,
        limitations: vec![
            "NCI-60 measurements are responses of long-established two-dimensional cell lines, not patients, organoids, xenografts, immune-competent tumors, or clinical outcomes.".to_owned(),
            "The six CNS lines are a broad CNS tumor panel and must not be silently relabeled as a representative glioblastoma patient cohort.".to_owned(),
            "CellMiner z scores are normalized activity patterns after its quality-control procedure; they are not doses, probabilities of response, or comparable to ALMANAC ComboScores.".to_owned(),
            "Positive ALMANAC ComboScores are hypothesis-generating in-vitro interaction signals and do not establish safety, therapeutic index, mechanism, animal efficacy, or clinical benefit.".to_owned(),
            "The CellMiner ALMANAC export contains repeated records for some canonical NSC pairs. This baseline keeps all source records and uses the per-cell-line median when one canonical pair has multiple records.".to_owned(),
            "Mechanism labels are used only by the declared baseline predictor. Candidate models must be evaluated on the committed held-out compound or drug-pair split without reading held-out responses.".to_owned(),
            "The ranked reference profiles report observed public data for auditability; they are not treatment recommendations.".to_owned(),
        ],
    };
    write_json_new(output, &baseline)?;
    println!(
        "derived {} single agents and {} combinations; evaluated {} + {} held-out CNS responses",
        baseline.single_agent.compound_count,
        baseline.combinations.drug_pair_count,
        baseline
            .single_agent
            .held_out_assessment
            .evaluated_held_out_observation_count,
        baseline
            .combinations
            .held_out_assessment
            .evaluated_held_out_observation_count
    );
    Ok(())
}

pub fn derive_challenges(
    source_directory: &Path,
    registry_path: &Path,
    catalogue_output: &Path,
    answer_key_output: &Path,
) -> Result<()> {
    if catalogue_output == answer_key_output {
        bail!("challenge catalogue and answer key must be distinct artifacts");
    }
    for output in [catalogue_output, answer_key_output] {
        if output.exists() {
            bail!("refusing to replace existing artifact {}", output.display());
        }
    }

    let registry_bytes = fs::read(registry_path)
        .with_context(|| format!("read registry {}", registry_path.display()))?;
    let registry = CancerDatasetRegistry::from_slice(&registry_bytes)
        .context("validate Cancer World dataset registry")?;
    for required in [SINGLE_SOURCE_ID, COMBO_SOURCE_ID] {
        if !registry
            .sources
            .iter()
            .any(|source| source.source_id == required)
        {
            bail!("Cancer World registry does not contain {required}");
        }
    }

    let manifest: AcquisitionManifest = read_json(&source_directory.join("acquisition.json"))?;
    verify_manifest(source_directory, &manifest)?;
    let single_bytes = verified_artifact(source_directory, &manifest, "nci60-average-z-score")?;
    let combo_bytes = verified_artifact(source_directory, &manifest, "nci-almanac-combo-score")?;
    let single_workbook = workbook_bytes(&single_bytes, SINGLE_INNER)?;
    let combo_workbook = workbook_bytes(&combo_bytes, COMBO_INNER)?;
    let (single_metadata, single_drugs) = read_single_drugs(&single_workbook)?;
    let (combo_metadata, pairs) = read_drug_pairs(&combo_workbook)?;
    let expected_metadata = WorkbookMetadata {
        database_version: manifest.cellminer_database_version.clone(),
        export_date: manifest.export_date.clone(),
    };
    if single_metadata != expected_metadata || combo_metadata != expected_metadata {
        bail!("CellMiner workbook metadata differs from acquisition manifest");
    }

    let (single_agent_candidates, single_agent_answers) =
        build_single_agent_challenges(&single_drugs)?;
    let (combination_candidates, combination_answers) =
        build_combination_challenges(&single_drugs, &pairs)?;
    let answer_payload_commitment =
        answer_payload_commitment(&single_agent_answers, &combination_answers)?;
    let source = BaselineSource {
        custodian: manifest.source.clone(),
        cellminer_database_version: manifest.cellminer_database_version.clone(),
        export_date: manifest.export_date.clone(),
        source_set_hash: manifest.source_set_hash,
        artifacts: manifest.artifacts.clone(),
    };
    let version = manifest.cellminer_database_version.replace('.', "-");
    let catalogue_id = format!("nci-cellminer-{version}-cns-challenge-catalogue-v1");
    let catalogue = CellminerChallengeCatalogue {
        schema_version: 1,
        catalogue_id: catalogue_id.clone(),
        evidence_class: "in_vitro_immortalized_cell_line_response_challenge_metadata".to_owned(),
        intended_use: "Prompt-safe identities and source metadata for blinded Cancer World qualification against held-out NCI-60 CNS single-agent activity and NCI-ALMANAC combination interaction measurements; not patient efficacy estimation.".to_owned(),
        source_registry_hash: registry.content_digest()?,
        source: source.clone(),
        cns_cell_lines: CNS_LINES.iter().map(|value| (*value).to_owned()).collect(),
        single_agent_partition: challenge_partition_for_single(
            &single_drugs,
            &single_agent_candidates,
        ),
        combination_partition: challenge_partition_for_combinations(
            &pairs,
            &combination_candidates,
        ),
        single_agent_candidates,
        combination_candidates,
        leakage_boundary: CatalogueLeakageBoundary {
            access_class: "prompt_safe_candidate_metadata".to_owned(),
            allowed_in_model_context: true,
            contains_observed_response_values: false,
            contains_derived_rank_labels: false,
        },
        limitations: vec![
            "Candidate inclusion discloses that all six CNS lines have a retained measurement and that the six retained values are not all equal, but discloses no response value, direction, or line-specific rank.".to_owned(),
            "FDA metadata means only whether the exact NCI-60 row was marked FDA approved; false is not a regulatory conclusion and null means the ALMANAC compound lacked a matching NCI-60 metadata row.".to_owned(),
            "Mechanism strings and drug names are source metadata, not verified target engagement, dosing instructions, or treatment recommendations.".to_owned(),
            "The qualification-only answer key is a separate artifact and must never be attached to a model request, retrieval result, memory, or prompt.".to_owned(),
        ],
    };
    verify_catalogue_has_no_response_labels(&catalogue)?;
    let catalogue_bytes = pretty_json_bytes(&catalogue)?;
    let catalogue_artifact_sha256 = Digest::sha256(&catalogue_bytes);

    let answer_key = CellminerChallengeAnswerKey {
        schema_version: 1,
        answer_key_id: format!("nci-cellminer-{version}-cns-challenge-answer-key-v1"),
        evidence_class: "qualification_only_in_vitro_immortalized_cell_line_response".to_owned(),
        intended_use: "Deterministic scoring of predictions produced without access to this artifact; never model context and never evidence of patient efficacy.".to_owned(),
        source_registry_hash: registry.content_digest()?,
        source,
        cns_cell_lines: CNS_LINES.iter().map(|value| (*value).to_owned()).collect(),
        ranking_rule: "Each six-line answer profile is serialized from largest observed value to smallest, with cell-line identifier ascending as the deterministic tie break. Rank 1 is the largest observed value and ties share 1 + the number of strictly larger values (competition ranking). For NCI-60 activity, larger z score means greater relative sensitivity; for ALMANAC, larger ComboScore means a stronger greater-than-additive interaction signal.".to_owned(),
        single_agent_response_measure: "CellMiner average NCI-60 compound-activity z score after quality control, stored as exact parsed value times 1,000.".to_owned(),
        combination_response_measure: "Median NCI-ALMANAC ComboScore across repeated canonical-pair source records for each CNS cell line, stored as value times 1,000.".to_owned(),
        catalogue_reference: ChallengeCatalogueReference {
            catalogue_id,
            catalogue_artifact_sha256,
            answer_payload_commitment,
        },
        single_agent_answers,
        combination_answers,
        leakage_boundary: AnswerKeyLeakageBoundary {
            access_class: "qualification_worker_only".to_owned(),
            allowed_in_model_context: false,
            contains_observed_response_values: true,
            contains_derived_rank_labels: true,
        },
        limitations: vec![
            "These labels measure established two-dimensional NCI-60 cell-line behavior, not patients, organoids, xenografts, immune effects, toxicity, exposure, or clinical benefit.".to_owned(),
            "A high activity z score or positive ComboScore can qualify a prediction against this assay family only; it cannot validate a treatment claim.".to_owned(),
            "Ranks are relative within the six CellMiner CNS lines for one compound or pair and are not comparable across candidates.".to_owned(),
            "The answer key is isolated for blindness, not because the underlying official NCI measurements are private.".to_owned(),
        ],
    };
    verify_challenge_pair(&catalogue, &catalogue_bytes, &answer_key)?;
    let answer_key_bytes = pretty_json_bytes(&answer_key)?;

    write_new(catalogue_output, &catalogue_bytes)?;
    write_new(answer_key_output, &answer_key_bytes)?;
    println!(
        "derived {} single-agent and {} combination held-out challenges; catalogue sha256 {}, answer payload commitment {}",
        catalogue.single_agent_candidates.len(),
        catalogue.combination_candidates.len(),
        catalogue_artifact_sha256,
        answer_payload_commitment,
    );
    Ok(())
}

fn build_single_agent_challenges(
    drugs: &[SingleDrug],
) -> Result<(Vec<SingleAgentCandidate>, Vec<SingleAgentAnswer>)> {
    let mut candidates = Vec::new();
    let mut answers = Vec::new();
    for drug in drugs {
        if !is_held_out(SINGLE_SPLIT_DOMAIN, &drug.nsc.to_string()) {
            continue;
        }
        let Some(values) = complete_cns(&drug.cns) else {
            continue;
        };
        if !has_informative_rank(&values) {
            continue;
        }
        let challenge_id = single_challenge_id(drug.nsc);
        candidates.push(SingleAgentCandidate {
            challenge_id: challenge_id.clone(),
            compound: ChallengeCompound {
                nsc: drug.nsc,
                drug_name: drug.name.clone(),
                mechanism: drug.mechanism.clone(),
                fda_approved: Some(drug.fda_approved),
            },
        });
        let ranks = descending_ranks(&values)?;
        let mut observations = CNS_LINES
            .iter()
            .zip(values)
            .zip(ranks)
            .map(
                |((cell_line, activity_z_milli), descending_response_rank)| {
                    SingleAgentObservation {
                        cell_line: (*cell_line).to_owned(),
                        activity_z_milli,
                        descending_response_rank,
                    }
                },
            )
            .collect::<Vec<_>>();
        observations.sort_by(|left, right| {
            right
                .activity_z_milli
                .cmp(&left.activity_z_milli)
                .then_with(|| left.cell_line.cmp(&right.cell_line))
        });
        answers.push(SingleAgentAnswer {
            challenge_id,
            nsc: drug.nsc,
            observations,
        });
    }
    candidates.sort_by(|left, right| left.challenge_id.cmp(&right.challenge_id));
    answers.sort_by(|left, right| left.challenge_id.cmp(&right.challenge_id));
    ensure_unique_challenge_ids(candidates.iter().map(|value| value.challenge_id.as_str()))?;
    ensure_unique_challenge_ids(answers.iter().map(|value| value.challenge_id.as_str()))?;
    if candidates.is_empty() || candidates.len() != answers.len() {
        bail!("single-agent challenge derivation produced an invalid candidate/answer set");
    }
    Ok((candidates, answers))
}

fn build_combination_challenges(
    drugs: &[SingleDrug],
    pairs: &[DrugPair],
) -> Result<(Vec<CombinationCandidate>, Vec<CombinationAnswer>)> {
    let drug_metadata = drugs
        .iter()
        .map(|drug| (drug.nsc, drug))
        .collect::<BTreeMap<_, _>>();
    let mut candidates = Vec::new();
    let mut answers = Vec::new();
    for pair in pairs {
        if !is_held_out(COMBO_SPLIT_DOMAIN, &pair_id(pair.nsc_1, pair.nsc_2)) {
            continue;
        }
        let Some(values) = complete_cns(&pair.cns) else {
            continue;
        };
        if !has_informative_rank(&values) {
            continue;
        }
        let challenge_id = combination_challenge_id(pair.nsc_1, pair.nsc_2);
        candidates.push(CombinationCandidate {
            challenge_id: challenge_id.clone(),
            first: ChallengeCompound {
                nsc: pair.nsc_1,
                drug_name: pair.name_1.clone(),
                mechanism: pair.mechanism_1.clone(),
                fda_approved: drug_metadata.get(&pair.nsc_1).map(|drug| drug.fda_approved),
            },
            second: ChallengeCompound {
                nsc: pair.nsc_2,
                drug_name: pair.name_2.clone(),
                mechanism: pair.mechanism_2.clone(),
                fda_approved: drug_metadata.get(&pair.nsc_2).map(|drug| drug.fda_approved),
            },
            source_record_count: pair.source_record_count,
        });
        let ranks = descending_ranks(&values)?;
        let mut observations = CNS_LINES
            .iter()
            .zip(values)
            .zip(ranks)
            .map(
                |((cell_line, combo_score_milli), descending_interaction_rank)| {
                    CombinationObservation {
                        cell_line: (*cell_line).to_owned(),
                        combo_score_milli,
                        descending_interaction_rank,
                        interaction_direction: interaction_direction(combo_score_milli),
                    }
                },
            )
            .collect::<Vec<_>>();
        observations.sort_by(|left, right| {
            right
                .combo_score_milli
                .cmp(&left.combo_score_milli)
                .then_with(|| left.cell_line.cmp(&right.cell_line))
        });
        answers.push(CombinationAnswer {
            challenge_id,
            nsc_1: pair.nsc_1,
            nsc_2: pair.nsc_2,
            observations,
        });
    }
    candidates.sort_by(|left, right| left.challenge_id.cmp(&right.challenge_id));
    answers.sort_by(|left, right| left.challenge_id.cmp(&right.challenge_id));
    ensure_unique_challenge_ids(candidates.iter().map(|value| value.challenge_id.as_str()))?;
    ensure_unique_challenge_ids(answers.iter().map(|value| value.challenge_id.as_str()))?;
    if candidates.is_empty() || candidates.len() != answers.len() {
        bail!("combination challenge derivation produced an invalid candidate/answer set");
    }
    Ok((candidates, answers))
}

fn challenge_partition_for_single(
    drugs: &[SingleDrug],
    candidates: &[SingleAgentCandidate],
) -> ChallengePartition {
    let (calibration, held_out) = partition_ids(
        drugs.iter().map(|drug| drug.nsc.to_string()),
        SINGLE_SPLIT_DOMAIN,
    );
    let candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.challenge_id.clone())
        .collect::<Vec<_>>();
    ChallengePartition {
        source_id: SINGLE_SOURCE_ID.to_owned(),
        split: split_summary(
            "NSC compound",
            SINGLE_SPLIT_DOMAIN,
            &calibration,
            &held_out,
        ),
        eligibility_rule: "Held-out whole NSC compound with a retained value for every one of the six declared CNS cell lines and at least two distinct retained values, so the profile contains an informative pairwise rank.".to_owned(),
        candidate_count: candidate_ids.len(),
        candidate_set_commitment: string_set_commitment(
            &format!("{CHALLENGE_SET_DOMAIN}/single-agent"),
            &candidate_ids,
        ),
    }
}

fn challenge_partition_for_combinations(
    pairs: &[DrugPair],
    candidates: &[CombinationCandidate],
) -> ChallengePartition {
    let (calibration, held_out) = partition_ids(
        pairs.iter().map(|pair| pair_id(pair.nsc_1, pair.nsc_2)),
        COMBO_SPLIT_DOMAIN,
    );
    let candidate_ids = candidates
        .iter()
        .map(|candidate| candidate.challenge_id.clone())
        .collect::<Vec<_>>();
    ChallengePartition {
        source_id: COMBO_SOURCE_ID.to_owned(),
        split: split_summary(
            "canonical NSC drug pair",
            COMBO_SPLIT_DOMAIN,
            &calibration,
            &held_out,
        ),
        eligibility_rule: "Held-out whole canonical NSC pair with a retained median ComboScore for every one of the six declared CNS cell lines and at least two distinct retained median values, so the profile contains an informative pairwise rank.".to_owned(),
        candidate_count: candidate_ids.len(),
        candidate_set_commitment: string_set_commitment(
            &format!("{CHALLENGE_SET_DOMAIN}/combination"),
            &candidate_ids,
        ),
    }
}

fn partition_ids(ids: impl Iterator<Item = String>, domain: &str) -> (Vec<String>, Vec<String>) {
    let mut calibration = Vec::new();
    let mut held_out = Vec::new();
    for id in ids {
        if is_held_out(domain, &id) {
            held_out.push(id);
        } else {
            calibration.push(id);
        }
    }
    (calibration, held_out)
}

fn complete_cns(values: &[Option<i64>; 6]) -> Option<[i64; 6]> {
    let mut complete = [0; 6];
    for (target, source) in complete.iter_mut().zip(values) {
        *target = (*source)?;
    }
    Some(complete)
}

fn has_informative_rank(values: &[i64; 6]) -> bool {
    values[1..].iter().any(|value| *value != values[0])
}

fn descending_ranks(values: &[i64; 6]) -> Result<[u8; 6]> {
    let mut ranks = [0; 6];
    for (index, value) in values.iter().enumerate() {
        ranks[index] = u8::try_from(1 + values.iter().filter(|other| *other > value).count())?;
    }
    Ok(ranks)
}

fn interaction_direction(value: i64) -> InteractionDirection {
    match value.cmp(&0) {
        std::cmp::Ordering::Less => InteractionDirection::Negative,
        std::cmp::Ordering::Equal => InteractionDirection::Zero,
        std::cmp::Ordering::Greater => InteractionDirection::Positive,
    }
}

fn single_challenge_id(nsc: u64) -> String {
    format!("nci60-cns-single-nsc-{nsc}")
}

fn combination_challenge_id(left: u64, right: u64) -> String {
    let (left, right) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    format!("nci-almanac-cns-combination-nsc-{left}-{right}")
}

fn ensure_unique_challenge_ids<'a>(ids: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            bail!("challenge identifier {id:?} is duplicated");
        }
    }
    Ok(())
}

fn answer_payload_commitment(
    single_agent_answers: &[SingleAgentAnswer],
    combination_answers: &[CombinationAnswer],
) -> Result<Digest> {
    let payload = ChallengeAnswerPayload {
        domain: CHALLENGE_ANSWER_DOMAIN,
        single_agent_answers,
        combination_answers,
    };
    Ok(Digest::sha256(&serde_json::to_vec(&payload)?))
}

fn verify_catalogue_has_no_response_labels(catalogue: &CellminerChallengeCatalogue) -> Result<()> {
    const FORBIDDEN_KEYS: [&str; 6] = [
        "observations",
        "activity_z_milli",
        "descending_response_rank",
        "combo_score_milli",
        "descending_interaction_rank",
        "interaction_direction",
    ];
    fn inspect(value: &serde_json::Value, path: &str) -> Result<()> {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, value) in fields {
                    if FORBIDDEN_KEYS.contains(&key.as_str()) {
                        bail!("prompt-safe catalogue contains forbidden answer field {path}.{key}");
                    }
                    inspect(value, &format!("{path}.{key}"))?;
                }
            }
            serde_json::Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    inspect(value, &format!("{path}[{index}]"))?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    inspect(&serde_json::to_value(catalogue)?, "$catalogue")
}

fn verify_challenge_pair(
    catalogue: &CellminerChallengeCatalogue,
    catalogue_bytes: &[u8],
    answer_key: &CellminerChallengeAnswerKey,
) -> Result<()> {
    verify_catalogue_has_no_response_labels(catalogue)?;
    if !catalogue.leakage_boundary.allowed_in_model_context
        || catalogue.leakage_boundary.contains_observed_response_values
        || catalogue.leakage_boundary.contains_derived_rank_labels
        || answer_key.leakage_boundary.allowed_in_model_context
        || !answer_key
            .leakage_boundary
            .contains_observed_response_values
        || !answer_key.leakage_boundary.contains_derived_rank_labels
    {
        bail!("CellMiner challenge leakage access classes are inconsistent");
    }
    if answer_key.catalogue_reference.catalogue_id != catalogue.catalogue_id
        || answer_key.catalogue_reference.catalogue_artifact_sha256
            != Digest::sha256(catalogue_bytes)
    {
        bail!("CellMiner answer key does not bind the exact candidate catalogue artifact");
    }
    let actual_commitment = answer_payload_commitment(
        &answer_key.single_agent_answers,
        &answer_key.combination_answers,
    )?;
    if actual_commitment != answer_key.catalogue_reference.answer_payload_commitment {
        bail!("CellMiner answer payload commitment differs from its answer key");
    }
    let single_candidates = catalogue
        .single_agent_candidates
        .iter()
        .map(|candidate| (&candidate.challenge_id, candidate.compound.nsc))
        .collect::<BTreeMap<_, _>>();
    let single_answers = answer_key
        .single_agent_answers
        .iter()
        .map(|answer| (&answer.challenge_id, answer.nsc))
        .collect::<BTreeMap<_, _>>();
    let combination_candidates = catalogue
        .combination_candidates
        .iter()
        .map(|candidate| {
            (
                &candidate.challenge_id,
                (candidate.first.nsc, candidate.second.nsc),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let combination_answers = answer_key
        .combination_answers
        .iter()
        .map(|answer| (&answer.challenge_id, (answer.nsc_1, answer.nsc_2)))
        .collect::<BTreeMap<_, _>>();
    if single_candidates != single_answers || combination_candidates != combination_answers {
        bail!("CellMiner challenge catalogue identities do not match answer-key identities");
    }
    let expected_lines = CNS_LINES.iter().copied().collect::<BTreeSet<_>>();
    for answer in &answer_key.single_agent_answers {
        let actual_lines = answer
            .observations
            .iter()
            .map(|observation| observation.cell_line.as_str())
            .collect::<BTreeSet<_>>();
        if actual_lines != expected_lines
            || answer
                .observations
                .windows(2)
                .all(|pair| pair[0].activity_z_milli == pair[1].activity_z_milli)
            || answer.observations.windows(2).any(|pair| {
                pair[0].activity_z_milli < pair[1].activity_z_milli
                    || (pair[0].activity_z_milli == pair[1].activity_z_milli
                        && pair[0].cell_line > pair[1].cell_line)
            })
            || answer.observations.iter().any(|observation| {
                usize::from(observation.descending_response_rank)
                    != 1 + answer
                        .observations
                        .iter()
                        .filter(|other| other.activity_z_milli > observation.activity_z_milli)
                        .count()
            })
        {
            bail!("CellMiner single-agent answer is incomplete or noncanonical");
        }
    }
    for answer in &answer_key.combination_answers {
        let actual_lines = answer
            .observations
            .iter()
            .map(|observation| observation.cell_line.as_str())
            .collect::<BTreeSet<_>>();
        if actual_lines != expected_lines
            || answer
                .observations
                .windows(2)
                .all(|pair| pair[0].combo_score_milli == pair[1].combo_score_milli)
            || answer.observations.windows(2).any(|pair| {
                pair[0].combo_score_milli < pair[1].combo_score_milli
                    || (pair[0].combo_score_milli == pair[1].combo_score_milli
                        && pair[0].cell_line > pair[1].cell_line)
            })
            || answer.observations.iter().any(|observation| {
                usize::from(observation.descending_interaction_rank)
                    != 1 + answer
                        .observations
                        .iter()
                        .filter(|other| other.combo_score_milli > observation.combo_score_milli)
                        .count()
                    || observation.interaction_direction
                        != interaction_direction(observation.combo_score_milli)
            })
        {
            bail!("CellMiner combination answer is incomplete or noncanonical");
        }
    }
    Ok(())
}

fn summarize_single(drugs: &[SingleDrug]) -> Result<SingleAgentSummary> {
    let mut calibration = Vec::new();
    let mut held_out = Vec::new();
    for drug in drugs {
        let id = drug.nsc.to_string();
        if is_held_out(SINGLE_SPLIT_DOMAIN, &id) {
            held_out.push(id);
        } else {
            calibration.push(id);
        }
    }
    let assessment = assess_single(drugs)?;
    let mut top = drugs
        .iter()
        .filter(|drug| drug.fda_approved)
        .filter_map(|drug| {
            let values = drug.cns.iter().flatten().copied().collect::<Vec<_>>();
            (values.len() >= 4).then(|| ObservedSingleProfile {
                nsc: drug.nsc,
                drug_name: drug.name.clone(),
                mechanism: drug.mechanism.clone(),
                observed_cns_line_count: values.len(),
                mean_activity_z_milli: mean_i64(&values),
            })
        })
        .collect::<Vec<_>>();
    top.sort_by(|left, right| {
        right
            .mean_activity_z_milli
            .cmp(&left.mean_activity_z_milli)
            .then_with(|| left.nsc.cmp(&right.nsc))
    });
    top.truncate(12);
    Ok(SingleAgentSummary {
        source_id: SINGLE_SOURCE_ID.to_owned(),
        response_measure: "CellMiner average NCI-60 compound-activity z score after quality control; higher values mean greater relative sensitivity.".to_owned(),
        cns_cell_lines: CNS_LINES.iter().map(|value| (*value).to_owned()).collect(),
        compound_count: drugs.len(),
        compounds_with_known_mechanism: drugs
            .iter()
            .filter(|drug| drug.mechanism.is_some())
            .count(),
        fda_approved_compound_count: drugs.iter().filter(|drug| drug.fda_approved).count(),
        observed_cns_response_count: drugs
            .iter()
            .map(|drug| drug.cns.iter().flatten().count())
            .sum(),
        split: split_summary(
            "NSC compound",
            SINGLE_SPLIT_DOMAIN,
            &calibration,
            &held_out,
        ),
        held_out_assessment: assessment,
        top_observed_fda_approved_cns_profiles: top,
    })
}

fn summarize_combinations(pairs: &[DrugPair]) -> Result<CombinationSummary> {
    let mut calibration = Vec::new();
    let mut held_out = Vec::new();
    for pair in pairs {
        let id = pair_id(pair.nsc_1, pair.nsc_2);
        if is_held_out(COMBO_SPLIT_DOMAIN, &id) {
            held_out.push(id);
        } else {
            calibration.push(id);
        }
    }
    let mut top = pairs
        .iter()
        .filter_map(|pair| {
            let values = pair.cns.iter().flatten().copied().collect::<Vec<_>>();
            (values.len() >= 4).then(|| ObservedComboProfile {
                nsc_1: pair.nsc_1,
                drug_name_1: pair.name_1.clone(),
                nsc_2: pair.nsc_2,
                drug_name_2: pair.name_2.clone(),
                source_record_count: pair.source_record_count,
                observed_cns_line_count: values.len(),
                mean_combo_score_milli: mean_i64(&values),
            })
        })
        .collect::<Vec<_>>();
    top.sort_by(|left, right| {
        right
            .mean_combo_score_milli
            .cmp(&left.mean_combo_score_milli)
            .then_with(|| left.nsc_1.cmp(&right.nsc_1))
            .then_with(|| left.nsc_2.cmp(&right.nsc_2))
    });
    top.truncate(15);
    Ok(CombinationSummary {
        source_id: COMBO_SOURCE_ID.to_owned(),
        response_measure: "NCI-ALMANAC ComboScore; higher values generally indicate more growth inhibition than expected from the component drugs tested separately.".to_owned(),
        cns_cell_lines: CNS_LINES.iter().map(|value| (*value).to_owned()).collect(),
        source_drug_pair_record_count: pairs
            .iter()
            .map(|pair| pair.source_record_count)
            .sum(),
        drug_pair_count: pairs.len(),
        repeated_canonical_pair_record_count: pairs
            .iter()
            .map(|pair| pair.source_record_count.saturating_sub(1))
            .sum(),
        observed_cns_combo_score_count: pairs
            .iter()
            .map(|pair| pair.cns.iter().flatten().count())
            .sum(),
        positive_cns_combo_score_count: pairs
            .iter()
            .flat_map(|pair| pair.cns.iter().flatten())
            .filter(|value| **value > 0)
            .count(),
        split: split_summary("canonical NSC drug pair", COMBO_SPLIT_DOMAIN, &calibration, &held_out),
        held_out_assessment: assess_combinations(pairs)?,
        top_observed_cns_combo_profiles: top,
    })
}

fn assess_single(drugs: &[SingleDrug]) -> Result<PredictorAssessment> {
    let mut by_mechanism_line: BTreeMap<(String, usize), Vec<i64>> = BTreeMap::new();
    let mut by_line: BTreeMap<usize, Vec<i64>> = BTreeMap::new();
    for drug in drugs
        .iter()
        .filter(|drug| !is_held_out(SINGLE_SPLIT_DOMAIN, &drug.nsc.to_string()))
    {
        for (line, value) in drug.cns.iter().enumerate() {
            if let Some(value) = value {
                by_line.entry(line).or_default().push(*value);
                if let Some(mechanism) = &drug.mechanism {
                    by_mechanism_line
                        .entry((mechanism.clone(), line))
                        .or_default()
                        .push(*value);
                }
            }
        }
    }
    let line_medians = medians_by_key(&by_line)?;
    let mechanism_medians = supported_medians(&by_mechanism_line, MIN_MECHANISM_SUPPORT);
    let mut eligible = 0usize;
    let mut predictor_errors = Vec::new();
    let mut baseline_errors = Vec::new();
    let mut evaluated_compounds = BTreeSet::new();
    for drug in drugs.iter().filter(|drug| {
        is_held_out(SINGLE_SPLIT_DOMAIN, &drug.nsc.to_string()) && drug.mechanism.is_some()
    }) {
        let mechanism = drug.mechanism.as_ref().expect("filtered mechanism");
        for (line, actual) in drug.cns.iter().enumerate() {
            let Some(actual) = actual else { continue };
            eligible += 1;
            let Some(predicted) = mechanism_medians.get(&(mechanism.clone(), line)) else {
                continue;
            };
            let baseline = line_medians
                .get(&line)
                .context("single-agent calibration line has no observations")?;
            predictor_errors.push(actual.abs_diff(*predicted));
            baseline_errors.push(actual.abs_diff(*baseline));
            evaluated_compounds.insert(drug.nsc);
        }
    }
    let predictor_mae = mean_u64(&predictor_errors)?;
    let baseline_mae = mean_u64(&baseline_errors)?;
    Ok(PredictorAssessment {
        predictor: "Median calibration response for the same declared mechanism and CNS cell line; whole NSC compounds are held out.".to_owned(),
        baseline: "Median calibration response for the CNS cell line without compound or mechanism information.".to_owned(),
        minimum_calibration_group_support: MIN_MECHANISM_SUPPORT,
        eligible_held_out_observation_count: eligible,
        evaluated_held_out_observation_count: predictor_errors.len(),
        evaluated_held_out_compound_count: evaluated_compounds.len(),
        coverage_parts_per_million: ratio_ppm(predictor_errors.len(), eligible),
        predictor_mean_absolute_error_z_milli: predictor_mae,
        baseline_mean_absolute_error_z_milli: baseline_mae,
        relative_mae_improvement_parts_per_million: relative_improvement_ppm(
            predictor_mae,
            baseline_mae,
        ),
    })
}

fn assess_combinations(pairs: &[DrugPair]) -> Result<ComboPredictorAssessment> {
    let mut by_mechanism_line: BTreeMap<((String, String), usize), Vec<i64>> = BTreeMap::new();
    let mut by_line: BTreeMap<usize, Vec<i64>> = BTreeMap::new();
    for pair in pairs
        .iter()
        .filter(|pair| !is_held_out(COMBO_SPLIT_DOMAIN, &pair_id(pair.nsc_1, pair.nsc_2)))
    {
        for (line, value) in pair.cns.iter().enumerate() {
            if let Some(value) = value {
                by_line.entry(line).or_default().push(*value);
                if let Some(mechanisms) = mechanism_pair(pair) {
                    by_mechanism_line
                        .entry((mechanisms, line))
                        .or_default()
                        .push(*value);
                }
            }
        }
    }
    let line_medians = medians_by_key(&by_line)?;
    let mechanism_medians = supported_medians(&by_mechanism_line, MIN_MECHANISM_SUPPORT);
    let mut eligible = 0usize;
    let mut predictor_errors = Vec::new();
    let mut zero_errors = Vec::new();
    let mut line_errors = Vec::new();
    let mut evaluated_pairs = BTreeSet::new();
    for pair in pairs.iter().filter(|pair| {
        is_held_out(COMBO_SPLIT_DOMAIN, &pair_id(pair.nsc_1, pair.nsc_2))
            && mechanism_pair(pair).is_some()
    }) {
        let mechanisms = mechanism_pair(pair).expect("filtered mechanism pair");
        for (line, actual) in pair.cns.iter().enumerate() {
            let Some(actual) = actual else { continue };
            eligible += 1;
            let Some(predicted) = mechanism_medians.get(&(mechanisms.clone(), line)) else {
                continue;
            };
            let line_baseline = line_medians
                .get(&line)
                .context("combination calibration line has no observations")?;
            predictor_errors.push(actual.abs_diff(*predicted));
            zero_errors.push(actual.unsigned_abs());
            line_errors.push(actual.abs_diff(*line_baseline));
            evaluated_pairs.insert(pair_id(pair.nsc_1, pair.nsc_2));
        }
    }
    Ok(ComboPredictorAssessment {
        predictor: "Median calibration ComboScore for the same canonical mechanism pair and CNS cell line; whole drug pairs are held out.".to_owned(),
        baselines: vec![
            "Zero ComboScore (no greater-than-additive interaction signal).".to_owned(),
            "Median calibration ComboScore for the CNS cell line without drug or mechanism information.".to_owned(),
        ],
        minimum_calibration_group_support: MIN_MECHANISM_SUPPORT,
        eligible_held_out_observation_count: eligible,
        evaluated_held_out_observation_count: predictor_errors.len(),
        evaluated_held_out_pair_count: evaluated_pairs.len(),
        coverage_parts_per_million: ratio_ppm(predictor_errors.len(), eligible),
        predictor_mean_absolute_error_score_milli: mean_u64(&predictor_errors)?,
        no_interaction_zero_mean_absolute_error_score_milli: mean_u64(&zero_errors)?,
        calibration_line_median_mean_absolute_error_score_milli: mean_u64(&line_errors)?,
    })
}

fn read_single_drugs(bytes: &[u8]) -> Result<(WorkbookMetadata, Vec<SingleDrug>)> {
    let mut workbook = Xlsx::new(Cursor::new(bytes)).context("open CellMiner NCI-60 workbook")?;
    let range = workbook
        .worksheet_range("all")
        .context("read CellMiner NCI-60 all sheet")?;
    let rows = range.rows().collect::<Vec<_>>();
    let metadata = metadata_from_rows(&rows)?;
    let headers = header_indices(rows.get(8).context("NCI-60 header row missing")?)?;
    let nsc = required_header(&headers, "NSC # b")?;
    let name = required_header(&headers, "Drug name")?;
    let fda_status = required_header(&headers, "FDA status")?;
    let mechanism = required_header(&headers, "Mechanism of action c")?;
    let cns_indices = cns_indices(&headers)?;
    let mut seen = BTreeSet::new();
    let mut drugs = Vec::new();
    for row in rows.iter().skip(9) {
        let Some(nsc) = cell_u64(row.get(nsc))? else {
            continue;
        };
        if !seen.insert(nsc) {
            bail!("CellMiner NCI-60 workbook repeats NSC {nsc}");
        }
        let name = required_cell(row.get(name), "NCI-60 drug name")?;
        let status = cell_text(row.get(fda_status)).unwrap_or_default();
        let mechanism = normalize_mechanism(cell_text(row.get(mechanism)));
        let mut cns = [None; 6];
        for (target, source) in cns.iter_mut().zip(cns_indices) {
            *target = scaled_cell(row.get(source), 1_000)?;
        }
        drugs.push(SingleDrug {
            nsc,
            name,
            fda_approved: status.eq_ignore_ascii_case("FDA approved"),
            mechanism,
            cns,
        });
    }
    if drugs.len() < 20_000 {
        bail!("CellMiner NCI-60 workbook has implausibly few compounds");
    }
    Ok((metadata, drugs))
}

fn read_drug_pairs(bytes: &[u8]) -> Result<(WorkbookMetadata, Vec<DrugPair>)> {
    let mut workbook = Xlsx::new(Cursor::new(bytes)).context("open CellMiner ALMANAC workbook")?;
    let range = workbook
        .worksheet_range("all")
        .context("read CellMiner ALMANAC all sheet")?;
    let rows = range.rows().collect::<Vec<_>>();
    let metadata = metadata_from_rows(&rows)?;
    let headers = header_indices(rows.get(8).context("ALMANAC header row missing")?)?;
    let nsc_1 = required_header(&headers, "NSC #1 b")?;
    let name_1 = nth_header(rows[8], "Drug name", 0)?;
    let mechanism_1 = nth_header(rows[8], "Mechanism of action c", 0)?;
    let nsc_2 = required_header(&headers, "NSC #2 b")?;
    let name_2 = nth_header(rows[8], "Drug name", 1)?;
    let mechanism_2 = nth_header(rows[8], "Mechanism of action c", 1)?;
    let cns_indices = cns_indices(&headers)?;
    let mut accumulators: BTreeMap<(u64, u64), DrugPairAccumulator> = BTreeMap::new();
    for row in rows.iter().skip(9) {
        let Some(mut left_nsc) = cell_u64(row.get(nsc_1))? else {
            continue;
        };
        let Some(mut right_nsc) = cell_u64(row.get(nsc_2))? else {
            bail!("ALMANAC row has only one NSC identifier");
        };
        let mut left_name = required_cell(row.get(name_1), "ALMANAC first drug name")?;
        let mut right_name = required_cell(row.get(name_2), "ALMANAC second drug name")?;
        let mut left_mechanism = normalize_mechanism(cell_text(row.get(mechanism_1)));
        let mut right_mechanism = normalize_mechanism(cell_text(row.get(mechanism_2)));
        if right_nsc < left_nsc {
            std::mem::swap(&mut left_nsc, &mut right_nsc);
            std::mem::swap(&mut left_name, &mut right_name);
            std::mem::swap(&mut left_mechanism, &mut right_mechanism);
        }
        if left_nsc == right_nsc {
            bail!("ALMANAC workbook contains a self-pair");
        }
        let mut cns = [None; 6];
        for (target, source) in cns.iter_mut().zip(cns_indices) {
            *target = scaled_cell(row.get(source), 1_000)?;
        }
        let accumulator = accumulators
            .entry((left_nsc, right_nsc))
            .or_insert_with(|| DrugPairAccumulator {
                name_1: left_name.clone(),
                mechanism_1: left_mechanism.clone(),
                name_2: right_name.clone(),
                mechanism_2: right_mechanism.clone(),
                cns: std::array::from_fn(|_| Vec::new()),
                source_record_count: 0,
            });
        if accumulator.name_1 != left_name
            || accumulator.name_2 != right_name
            || accumulator.mechanism_1 != left_mechanism
            || accumulator.mechanism_2 != right_mechanism
        {
            bail!("ALMANAC repeated pair has conflicting compound metadata");
        }
        for (values, value) in accumulator.cns.iter_mut().zip(cns) {
            if let Some(value) = value {
                values.push(value);
            }
        }
        accumulator.source_record_count += 1;
    }
    let pairs = accumulators
        .into_iter()
        .map(|((nsc_1, nsc_2), accumulator)| DrugPair {
            nsc_1,
            name_1: accumulator.name_1,
            mechanism_1: accumulator.mechanism_1,
            nsc_2,
            name_2: accumulator.name_2,
            mechanism_2: accumulator.mechanism_2,
            cns: accumulator
                .cns
                .map(|values| (!values.is_empty()).then(|| median_i64(&values))),
            source_record_count: accumulator.source_record_count,
        })
        .collect::<Vec<_>>();
    if pairs.len() < 5_000 {
        bail!("CellMiner ALMANAC workbook has implausibly few drug pairs");
    }
    Ok((metadata, pairs))
}

fn read_workbook_metadata(bytes: &[u8]) -> Result<WorkbookMetadata> {
    let mut workbook = Xlsx::new(Cursor::new(bytes)).context("open CellMiner workbook")?;
    let range = workbook
        .worksheet_range("all")
        .context("read CellMiner all sheet")?;
    let rows = range.rows().take(5).collect::<Vec<_>>();
    metadata_from_rows(&rows)
}

fn metadata_from_rows(rows: &[&[Data]]) -> Result<WorkbookMetadata> {
    if required_cell(
        rows.first().and_then(|row| row.first()),
        "CellMiner metadata label",
    )? != "CellMiner Address:"
        || required_cell(
            rows.get(2).and_then(|row| row.first()),
            "CellMiner version label",
        )? != "CellMiner Database Version:"
        || required_cell(
            rows.get(4).and_then(|row| row.first()),
            "CellMiner date label",
        )? != "Date:"
    {
        bail!("CellMiner workbook metadata labels changed");
    }
    Ok(WorkbookMetadata {
        database_version: required_cell(
            rows.get(2).and_then(|row| row.get(1)),
            "CellMiner database version",
        )?,
        export_date: required_cell(
            rows.get(4).and_then(|row| row.get(1)),
            "CellMiner export date",
        )?,
    })
}

fn header_indices(row: &[Data]) -> Result<BTreeMap<String, usize>> {
    let mut headers = BTreeMap::new();
    for (index, cell) in row.iter().enumerate() {
        let Some(value) = cell_text(Some(cell)) else {
            continue;
        };
        if headers.insert(value.clone(), index).is_some() {
            // Repeated metadata columns are resolved explicitly with nth_header.
            if value != "Drug name" && value != "FDA status" && value != "Mechanism of action c" {
                bail!("CellMiner workbook repeats header {value:?}");
            }
        }
    }
    Ok(headers)
}

fn nth_header(row: &[Data], expected: &str, occurrence: usize) -> Result<usize> {
    row.iter()
        .enumerate()
        .filter(|(_, cell)| cell_text(Some(cell)).as_deref() == Some(expected))
        .nth(occurrence)
        .map(|(index, _)| index)
        .with_context(|| {
            format!("CellMiner workbook missing header {expected:?} occurrence {occurrence}")
        })
}

fn required_header(headers: &BTreeMap<String, usize>, name: &str) -> Result<usize> {
    headers
        .get(name)
        .copied()
        .with_context(|| format!("CellMiner workbook missing header {name:?}"))
}

fn cns_indices(headers: &BTreeMap<String, usize>) -> Result<[usize; 6]> {
    Ok([
        required_header(headers, CNS_LINES[0])?,
        required_header(headers, CNS_LINES[1])?,
        required_header(headers, CNS_LINES[2])?,
        required_header(headers, CNS_LINES[3])?,
        required_header(headers, CNS_LINES[4])?,
        required_header(headers, CNS_LINES[5])?,
    ])
}

fn cell_text(cell: Option<&Data>) -> Option<String> {
    let value = cell?;
    if matches!(value, Data::Empty) {
        return None;
    }
    let text = value.to_string();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn required_cell(cell: Option<&Data>, context: &str) -> Result<String> {
    cell_text(cell).with_context(|| format!("{context} is missing"))
}

fn cell_u64(cell: Option<&Data>) -> Result<Option<u64>> {
    let Some(cell) = cell else { return Ok(None) };
    match cell {
        Data::Empty => Ok(None),
        Data::Int(value) => Ok(Some(u64::try_from(*value)?)),
        Data::Float(value)
            if value.is_finite()
                && value.fract() == 0.0
                && *value >= 0.0
                && *value <= u64::MAX as f64 =>
        {
            Ok(Some(*value as u64))
        }
        _ => Ok(Some(
            cell.to_string()
                .trim()
                .parse::<u64>()
                .context("parse CellMiner integer identifier")?,
        )),
    }
}

fn scaled_cell(cell: Option<&Data>, scale: i64) -> Result<Option<i64>> {
    let Some(text) = cell_text(cell) else {
        return Ok(None);
    };
    if text.eq_ignore_ascii_case("na") {
        return Ok(None);
    }
    let value = text
        .parse::<f64>()
        .with_context(|| format!("parse CellMiner response {text:?}"))?;
    if !value.is_finite() {
        bail!("CellMiner response is non-finite");
    }
    let scaled = value * scale as f64;
    let rounded = scaled.round();
    if (scaled - rounded).abs() > 1e-6 || rounded < i64::MIN as f64 || rounded > i64::MAX as f64 {
        bail!("CellMiner response exceeds fixed-point precision");
    }
    Ok(Some(rounded as i64))
}

fn normalize_mechanism(value: Option<String>) -> Option<String> {
    value.filter(|value| value != "-" && !value.eq_ignore_ascii_case("na"))
}

fn mechanism_pair(pair: &DrugPair) -> Option<(String, String)> {
    let mut left = pair.mechanism_1.clone()?;
    let mut right = pair.mechanism_2.clone()?;
    if right < left {
        std::mem::swap(&mut left, &mut right);
    }
    Some((left, right))
}

fn split_summary(
    unit: &str,
    domain: &str,
    calibration: &[String],
    held_out: &[String],
) -> SplitSummary {
    SplitSummary {
        unit: unit.to_owned(),
        derivation_domain: domain.to_owned(),
        rule: "sha256(domain || 0x00 || canonical unit id), first eight bytes modulo 5; zero is held out".to_owned(),
        calibration_unit_count: calibration.len(),
        held_out_unit_count: held_out.len(),
        calibration_set_commitment: string_set_commitment(domain, calibration),
        held_out_set_commitment: string_set_commitment(domain, held_out),
    }
}

fn is_held_out(domain: &str, id: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(id.as_bytes());
    let bytes: [u8; 32] = hasher.finalize().into();
    u64::from_be_bytes(bytes[..8].try_into().expect("SHA-256 prefix")) % 5 == 0
}

fn string_set_commitment(domain: &str, ids: &[String]) -> Digest {
    let mut ids = ids.to_vec();
    ids.sort();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(domain.as_bytes());
    bytes.push(0);
    for id in ids {
        let length = u32::try_from(id.len()).expect("bounded CellMiner id");
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(id.as_bytes());
    }
    Digest::sha256(&bytes)
}

fn pair_id(left: u64, right: u64) -> String {
    if left <= right {
        format!("{left}|{right}")
    } else {
        format!("{right}|{left}")
    }
}

fn supported_medians<K: Clone + Ord>(
    values: &BTreeMap<K, Vec<i64>>,
    minimum_support: usize,
) -> BTreeMap<K, i64> {
    values
        .iter()
        .filter(|(_, values)| values.len() >= minimum_support)
        .map(|(key, values)| (key.clone(), median_i64(values)))
        .collect()
}

fn medians_by_key<K: Clone + Ord>(values: &BTreeMap<K, Vec<i64>>) -> Result<BTreeMap<K, i64>> {
    if values.is_empty() {
        bail!("cannot calculate medians for an empty calibration set");
    }
    Ok(values
        .iter()
        .map(|(key, values)| (key.clone(), median_i64(values)))
        .collect())
}

fn median_i64(values: &[i64]) -> i64 {
    let mut values = values.to_vec();
    values.sort_unstable();
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        values[middle - 1].saturating_add(values[middle]) / 2
    } else {
        values[middle]
    }
}

fn mean_i64(values: &[i64]) -> i64 {
    let sum = values.iter().map(|value| i128::from(*value)).sum::<i128>();
    i64::try_from(sum / i128::try_from(values.len()).expect("nonempty bounded length"))
        .expect("CellMiner mean fits i64")
}

fn mean_u64(values: &[u64]) -> Result<u64> {
    if values.is_empty() {
        bail!("held-out assessment has no supported observations");
    }
    let sum = values.iter().map(|value| u128::from(*value)).sum::<u128>();
    let count = u128::try_from(values.len())?;
    Ok(u64::try_from((sum + count / 2) / count)?)
}

fn ratio_ppm(numerator: usize, denominator: usize) -> u32 {
    if denominator == 0 {
        return 0;
    }
    u32::try_from(
        (u128::try_from(numerator).expect("bounded count") * 1_000_000
            + u128::try_from(denominator).expect("bounded count") / 2)
            / u128::try_from(denominator).expect("bounded count"),
    )
    .expect("ratio is at most one million")
}

fn relative_improvement_ppm(predictor: u64, baseline: u64) -> i64 {
    if baseline == 0 {
        return 0;
    }
    let difference = i128::from(baseline) - i128::from(predictor);
    i64::try_from((difference * 1_000_000) / i128::from(baseline))
        .expect("relative improvement fits i64")
}

fn workbook_bytes(outer: &[u8], inner_name: &str) -> Result<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(outer)).context("open CellMiner ZIP")?;
    let mut entry = archive
        .by_name(inner_name)
        .with_context(|| format!("CellMiner ZIP missing {inner_name}"))?;
    if entry.size() > MAX_DOWNLOAD_BYTES {
        bail!("CellMiner workbook exceeds bounded size");
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size())?);
    entry
        .read_to_end(&mut bytes)
        .with_context(|| format!("read CellMiner workbook {inner_name}"))?;
    Ok(bytes)
}

async fn download_limited(client: &Client, url: &str) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("request {url}"))?
        .error_for_status()
        .with_context(|| format!("CellMiner rejected {url}"))?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_DOWNLOAD_BYTES)
    {
        bail!("CellMiner download exceeds bounded size");
    }
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("read {url}"))?;
    if u64::try_from(bytes.len())? > MAX_DOWNLOAD_BYTES {
        bail!("CellMiner download exceeds bounded size");
    }
    Ok(bytes.to_vec())
}

fn verify_manifest(directory: &Path, manifest: &AcquisitionManifest) -> Result<()> {
    if manifest.schema_version != 1
        || manifest.source != "National Cancer Institute CellMiner"
        || manifest.artifacts.len() != ARTIFACTS.len()
        || artifact_set_hash(&manifest.artifacts) != manifest.source_set_hash
    {
        bail!("unsupported or inconsistent CellMiner acquisition manifest");
    }
    for spec in ARTIFACTS {
        let artifact = manifest
            .artifacts
            .iter()
            .find(|artifact| artifact.artifact_id == spec.artifact_id)
            .with_context(|| format!("manifest missing {}", spec.artifact_id))?;
        if artifact.file_name != spec.file_name
            || artifact.inner_workbook != spec.inner_name
            || artifact.url != format!("{CELLMINER_ROOT}{}", spec.url_path)
        {
            bail!("CellMiner artifact identity changed");
        }
        let bytes = fs::read(directory.join(spec.file_name))
            .with_context(|| format!("read CellMiner artifact {}", spec.file_name))?;
        if u64::try_from(bytes.len())? != artifact.byte_length
            || Digest::sha256(&bytes) != artifact.sha256
        {
            bail!(
                "CellMiner artifact {} differs from manifest",
                spec.file_name
            );
        }
        let workbook = workbook_bytes(&bytes, spec.inner_name)?;
        let metadata = read_workbook_metadata(&workbook)?;
        if metadata.database_version != manifest.cellminer_database_version
            || metadata.export_date != manifest.export_date
        {
            bail!("CellMiner artifact metadata differs from manifest");
        }
    }
    Ok(())
}

fn verified_artifact(
    directory: &Path,
    manifest: &AcquisitionManifest,
    artifact_id: &str,
) -> Result<Vec<u8>> {
    let artifact = manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.artifact_id == artifact_id)
        .with_context(|| format!("manifest missing artifact {artifact_id}"))?;
    let bytes = fs::read(directory.join(&artifact.file_name))?;
    if u64::try_from(bytes.len())? != artifact.byte_length
        || Digest::sha256(&bytes) != artifact.sha256
    {
        bail!("CellMiner artifact {artifact_id} failed verification");
    }
    Ok(bytes)
}

fn artifact_set_hash(artifacts: &[AcquiredArtifact]) -> Digest {
    let mut entries = artifacts
        .iter()
        .map(|artifact| {
            format!(
                "{}\0{}\0{}\0{}",
                artifact.artifact_id, artifact.file_name, artifact.byte_length, artifact.sha256
            )
        })
        .collect::<Vec<_>>();
    entries.sort();
    Digest::sha256(entries.join("\n").as_bytes())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("decode {}", path.display()))
}

fn write_json_new<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = pretty_json_bytes(value)?;
    write_new(path, &bytes)
}

fn pretty_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value).context("encode canonical JSON artifact")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_is_stable_and_pair_order_is_canonical() {
        assert_eq!(pair_id(752, 740), "740|752");
        assert_eq!(
            is_held_out(SINGLE_SPLIT_DOMAIN, "740"),
            is_held_out(SINGLE_SPLIT_DOMAIN, "740")
        );
        assert_ne!(
            string_set_commitment(SINGLE_SPLIT_DOMAIN, &["1".to_owned()]),
            string_set_commitment(SINGLE_SPLIT_DOMAIN, &["2".to_owned()])
        );
    }

    #[test]
    fn integer_metrics_are_deterministic() {
        assert_eq!(median_i64(&[3, 1, 2]), 2);
        assert_eq!(median_i64(&[4, 2]), 3);
        assert_eq!(mean_i64(&[1_000, 2_000, 3_000]), 2_000);
        assert_eq!(mean_u64(&[1, 2]).expect("mean"), 2);
        assert_eq!(ratio_ppm(1, 4), 250_000);
        assert_eq!(relative_improvement_ppm(75, 100), 250_000);
    }

    #[test]
    fn challenge_ranks_and_identifiers_are_deterministic() {
        assert_eq!(single_challenge_id(740), "nci60-cns-single-nsc-740");
        assert_eq!(
            combination_challenge_id(752, 740),
            "nci-almanac-cns-combination-nsc-740-752"
        );
        assert_eq!(
            descending_ranks(&[200, 100, 200, -50, 0, 50]).expect("ranks"),
            [1, 3, 1, 6, 5, 4]
        );
        assert!(!has_informative_rank(&[250; 6]));
        assert!(has_informative_rank(&[250, 250, 250, 250, 250, 251]));
        assert_eq!(interaction_direction(-1), InteractionDirection::Negative);
        assert_eq!(interaction_direction(0), InteractionDirection::Zero);
        assert_eq!(interaction_direction(1), InteractionDirection::Positive);
    }

    #[test]
    fn challenge_builders_exclude_all_tied_profiles_for_both_measurement_classes() {
        let single_drugs = vec![
            SingleDrug {
                nsc: 100_071,
                name: "all tied".to_owned(),
                fda_approved: false,
                mechanism: None,
                cns: [Some(250); 6],
            },
            SingleDrug {
                nsc: 10_010,
                name: "informative".to_owned(),
                fda_approved: false,
                mechanism: None,
                cns: [
                    Some(250),
                    Some(250),
                    Some(250),
                    Some(250),
                    Some(250),
                    Some(251),
                ],
            },
        ];
        let (single_candidates, single_answers) =
            build_single_agent_challenges(&single_drugs).expect("single challenges");
        assert_eq!(single_candidates.len(), 1);
        assert_eq!(single_answers.len(), 1);
        assert_eq!(single_candidates[0].compound.nsc, 10_010);

        let pairs = vec![
            DrugPair {
                nsc_1: 102_816,
                name_1: "first".to_owned(),
                mechanism_1: None,
                nsc_2: 105_014,
                name_2: "all tied".to_owned(),
                mechanism_2: None,
                cns: [Some(0); 6],
                source_record_count: 1,
            },
            DrugPair {
                nsc_1: 102_816,
                name_1: "first".to_owned(),
                mechanism_1: None,
                nsc_2: 109_724,
                name_2: "informative".to_owned(),
                mechanism_2: None,
                cns: [Some(0), Some(0), Some(0), Some(0), Some(0), Some(1)],
                source_record_count: 1,
            },
        ];
        let (combination_candidates, combination_answers) =
            build_combination_challenges(&[], &pairs).expect("combination challenges");
        assert_eq!(combination_candidates.len(), 1);
        assert_eq!(combination_answers.len(), 1);
        assert_eq!(combination_candidates[0].second.nsc, 109_724);
    }

    #[test]
    fn answer_commitment_changes_without_exposing_labels_in_catalogue_shape() {
        let digest = Digest::sha256(b"test");
        let catalogue = CellminerChallengeCatalogue {
            schema_version: 1,
            catalogue_id: "catalogue-test".to_owned(),
            evidence_class: "test".to_owned(),
            intended_use: "test".to_owned(),
            source_registry_hash: digest,
            source: BaselineSource {
                custodian: "test".to_owned(),
                cellminer_database_version: "test".to_owned(),
                export_date: "test".to_owned(),
                source_set_hash: digest,
                artifacts: Vec::new(),
            },
            cns_cell_lines: CNS_LINES.iter().map(|line| (*line).to_owned()).collect(),
            single_agent_partition: ChallengePartition {
                source_id: SINGLE_SOURCE_ID.to_owned(),
                split: split_summary(
                    "NSC compound",
                    SINGLE_SPLIT_DOMAIN,
                    &[],
                    &["740".to_owned()],
                ),
                eligibility_rule: "test".to_owned(),
                candidate_count: 1,
                candidate_set_commitment: digest,
            },
            combination_partition: ChallengePartition {
                source_id: COMBO_SOURCE_ID.to_owned(),
                split: split_summary("canonical NSC drug pair", COMBO_SPLIT_DOMAIN, &[], &[]),
                eligibility_rule: "test".to_owned(),
                candidate_count: 0,
                candidate_set_commitment: digest,
            },
            single_agent_candidates: vec![SingleAgentCandidate {
                challenge_id: single_challenge_id(740),
                compound: ChallengeCompound {
                    nsc: 740,
                    drug_name: "test".to_owned(),
                    mechanism: Some("test".to_owned()),
                    fda_approved: Some(false),
                },
            }],
            combination_candidates: Vec::new(),
            leakage_boundary: CatalogueLeakageBoundary {
                access_class: "prompt_safe_candidate_metadata".to_owned(),
                allowed_in_model_context: true,
                contains_observed_response_values: false,
                contains_derived_rank_labels: false,
            },
            limitations: Vec::new(),
        };
        verify_catalogue_has_no_response_labels(&catalogue).expect("catalogue is label-free");

        let mut first = vec![SingleAgentAnswer {
            challenge_id: single_challenge_id(740),
            nsc: 740,
            observations: vec![SingleAgentObservation {
                cell_line: CNS_LINES[0].to_owned(),
                activity_z_milli: 100,
                descending_response_rank: 1,
            }],
        }];
        let first_commitment = answer_payload_commitment(&first, &[]).expect("commitment");
        first[0].observations[0].activity_z_milli = 101;
        let changed_commitment = answer_payload_commitment(&first, &[]).expect("commitment");
        assert_ne!(first_commitment, changed_commitment);
    }
}
