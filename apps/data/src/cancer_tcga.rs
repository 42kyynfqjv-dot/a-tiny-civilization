use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use md5::Md5;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::task::JoinSet;
use uuid::Uuid;
use world_data::CancerDatasetRegistry;
use world_domain::Digest;

const API_ROOT: &str = "https://api.gdc.cancer.gov";
const PROJECT_ID: &str = "TCGA-GBM";
const SPLIT_DOMAIN: &str = "a-tiny-civilization/tcga-gbm/patient-split/v1";
const CASE_FIELDS: &str = "case_id,submitter_id,project.project_id,demographic.vital_status,demographic.days_to_birth,demographic.days_to_death,diagnoses.age_at_diagnosis,diagnoses.days_to_last_follow_up,diagnoses.primary_diagnosis,diagnoses.tumor_grade";
const FILE_FIELDS: &str = "file_id,file_name,file_size,md5sum,data_category,data_type,data_format,experimental_strategy,analysis.workflow_type,cases.case_id,cases.submitter_id";

#[derive(Debug, Deserialize, Serialize)]
struct GdcStatus {
    commit: String,
    data_release: String,
    data_release_version: GdcReleaseVersion,
    status: String,
    tag: String,
    version: u16,
}

#[derive(Debug, Deserialize, Serialize)]
struct GdcReleaseVersion {
    major: u16,
    minor: u16,
    release_date: String,
}

#[derive(Debug, Deserialize)]
struct GdcEnvelope<T> {
    data: GdcData<T>,
}

#[derive(Debug, Deserialize)]
struct GdcData<T> {
    hits: Vec<T>,
    pagination: GdcPagination,
}

#[derive(Debug, Deserialize)]
struct GdcPagination {
    count: usize,
    total: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct GdcFile {
    file_id: String,
    file_name: String,
    file_size: u64,
    md5sum: String,
    data_type: String,
    data_format: String,
    experimental_strategy: String,
    analysis: GdcAnalysis,
    cases: Vec<GdcFileCase>,
}

#[derive(Clone, Debug, Deserialize)]
struct GdcAnalysis {
    workflow_type: String,
}

#[derive(Clone, Debug, Deserialize)]
struct GdcFileCase {
    case_id: String,
}

#[derive(Debug, Deserialize)]
struct GdcCase {
    case_id: String,
    project: GdcProject,
    demographic: Option<GdcDemographic>,
    #[serde(default)]
    diagnoses: Vec<GdcDiagnosis>,
}

#[derive(Debug, Deserialize)]
struct GdcProject {
    project_id: String,
}

#[derive(Debug, Deserialize)]
struct GdcDemographic {
    vital_status: Option<String>,
    days_to_birth: Option<GdcDay>,
    days_to_death: Option<GdcDay>,
}

#[derive(Debug, Deserialize)]
struct GdcDiagnosis {
    age_at_diagnosis: Option<GdcDay>,
    days_to_last_follow_up: Option<GdcDay>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GdcDay {
    Integer(i64),
    Float(f64),
}

impl GdcDay {
    fn whole(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            Self::Float(value)
                if value.is_finite()
                    && value.fract() == 0.0
                    && *value >= i64::MIN as f64
                    && *value <= i64::MAX as f64 =>
            {
                Some(*value as i64)
            }
            Self::Float(_) => None,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct AcquisitionManifest {
    schema_version: u16,
    source_id: String,
    api_root: String,
    status: GdcStatus,
    case_query: String,
    case_response_hash: Digest,
    case_response_byte_length: u64,
    case_count: usize,
    mutation_file_query: String,
    mutation_catalog_hash: Digest,
    mutation_catalog_byte_length: u64,
    mutation_file_count: usize,
    mutation_file_byte_length: u64,
    mutation_file_set_hash: Digest,
}

#[derive(Debug, Serialize)]
struct TcgaGbmBaseline {
    schema_version: u16,
    baseline_id: String,
    evidence_class: String,
    intended_use: String,
    source_registry_hash: Digest,
    source: BaselineSource,
    split: PatientSplit,
    complete_cohort: CohortSummary,
    calibration_cohort: CohortSummary,
    held_out_validation_cohort: CohortSummary,
    held_out_assessment: HeldOutAssessment,
    limitations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BaselineSource {
    source_id: String,
    data_release: String,
    data_release_date: String,
    api_commit: String,
    api_version: String,
    case_response_hash: Digest,
    mutation_catalog_hash: Digest,
    mutation_file_set_hash: Digest,
    mutation_file_count: usize,
}

#[derive(Debug, Serialize)]
struct PatientSplit {
    derivation_domain: String,
    rule: String,
    calibration_patient_count: usize,
    held_out_validation_patient_count: usize,
    calibration_patient_set_commitment: Digest,
    held_out_validation_patient_set_commitment: Digest,
}

#[derive(Debug, Serialize)]
struct CohortSummary {
    patient_count: usize,
    dead_patient_count: usize,
    alive_patient_count: usize,
    unknown_vital_status_count: usize,
    known_age_at_diagnosis_count: usize,
    median_age_at_diagnosis_days: Option<i64>,
    known_days_to_death_count: usize,
    median_days_to_death_among_observed_deaths: Option<i64>,
    known_alive_follow_up_count: usize,
    median_alive_follow_up_days: Option<i64>,
    molecularly_profiled_patient_count: usize,
    protein_altering_variant_count: usize,
    median_unique_protein_altering_variants_per_profiled_patient: Option<usize>,
    top_protein_altering_gene_prevalence: Vec<GenePrevalence>,
}

#[derive(Debug, Serialize)]
struct GenePrevalence {
    gene: String,
    patients_with_variant: usize,
    profiled_patients: usize,
    prevalence_parts_per_million: u32,
}

#[derive(Debug, Serialize)]
struct HeldOutAssessment {
    predictor: String,
    feature_selection: String,
    evaluated_gene_count: usize,
    mean_absolute_prevalence_error_parts_per_million: u32,
    mean_brier_score_parts_per_million: u32,
    predictions: Vec<GenePrevalencePrediction>,
    interpretation: String,
}

#[derive(Debug, Serialize)]
struct GenePrevalencePrediction {
    gene: String,
    calibration_prevalence_parts_per_million: u32,
    held_out_prevalence_parts_per_million: u32,
    absolute_error_parts_per_million: u32,
}

#[derive(Default)]
struct PatientRecord {
    vital_status: VitalStatus,
    age_at_diagnosis_days: Option<i64>,
    days_to_death: Option<i64>,
    alive_follow_up_days: Option<i64>,
    molecularly_profiled: bool,
    variants: BTreeSet<String>,
    genes: BTreeSet<String>,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum VitalStatus {
    Alive,
    Dead,
    #[default]
    Unknown,
}

pub async fn acquire(output_directory: &Path) -> Result<()> {
    let resuming = output_directory.exists();
    if output_directory.join("acquisition.json").exists() {
        bail!(
            "TCGA-GBM acquisition is already complete: {}",
            output_directory.display()
        );
    }
    if !resuming {
        fs::create_dir_all(output_directory.join("maf")).with_context(|| {
            format!(
                "create TCGA-GBM source directory {}",
                output_directory.display()
            )
        })?;
    } else if !output_directory.join("maf").is_dir() {
        bail!("partial TCGA-GBM acquisition has no MAF directory");
    }
    let client = Client::builder()
        .https_only(true)
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(90))
        .user_agent("a-tiny-civilization-tcga-gbm-acquisition/0.1")
        .build()
        .context("construct GDC client")?;

    let status_bytes = if resuming {
        fs::read(output_directory.join("status.json")).context("resume GDC status response")?
    } else {
        get_limited(&client, &format!("{API_ROOT}/status"), 64 * 1024).await?
    };
    let status: GdcStatus = serde_json::from_slice(&status_bytes).context("decode GDC status")?;
    if status.status != "OK" || status.data_release_version.major == 0 {
        bail!("GDC status is not a released healthy dataset");
    }

    let case_filter = project_filter();
    let case_query =
        format!("filters={case_filter}&fields={CASE_FIELDS}&size=1000&sort=case_id:asc");
    let case_bytes = if resuming {
        fs::read(output_directory.join("cases.json")).context("resume GDC case response")?
    } else {
        get_query(
            &client,
            &format!("{API_ROOT}/cases"),
            &[
                ("filters", case_filter.as_str()),
                ("fields", CASE_FIELDS),
                ("size", "1000"),
                ("sort", "case_id:asc"),
            ],
            16 * 1024 * 1024,
        )
        .await?
    };
    let cases: GdcEnvelope<GdcCase> =
        serde_json::from_slice(&case_bytes).context("decode GDC cases")?;
    require_complete_page(&cases.data.pagination, cases.data.hits.len(), "cases")?;
    if cases.data.hits.iter().any(|case| {
        case.project.project_id != PROJECT_ID || Uuid::parse_str(&case.case_id).is_err()
    }) {
        bail!("GDC case response crossed the requested project boundary");
    }

    let file_filter = mutation_file_filter();
    let file_query =
        format!("filters={file_filter}&fields={FILE_FIELDS}&size=1000&sort=file_id:asc");
    let file_bytes = if resuming {
        fs::read(output_directory.join("mutation-files.json"))
            .context("resume GDC mutation catalog")?
    } else {
        get_query(
            &client,
            &format!("{API_ROOT}/files"),
            &[
                ("filters", file_filter.as_str()),
                ("fields", FILE_FIELDS),
                ("size", "1000"),
                ("sort", "file_id:asc"),
            ],
            16 * 1024 * 1024,
        )
        .await?
    };
    let mut files: GdcEnvelope<GdcFile> =
        serde_json::from_slice(&file_bytes).context("decode GDC mutation catalog")?;
    require_complete_page(
        &files.data.pagination,
        files.data.hits.len(),
        "mutation files",
    )?;
    files
        .data
        .hits
        .sort_by(|left, right| left.file_id.cmp(&right.file_id));
    validate_file_catalog(&files.data.hits)?;

    if !resuming {
        write_new(&output_directory.join("status.json"), &status_bytes)?;
        write_new(&output_directory.join("cases.json"), &case_bytes)?;
        write_new(&output_directory.join("mutation-files.json"), &file_bytes)?;
    }

    let mut downloaded = BTreeMap::new();
    for batch in files.data.hits.chunks(12) {
        let mut tasks = JoinSet::new();
        for file in batch {
            let client = client.clone();
            let file = file.clone();
            let path = output_directory
                .join("maf")
                .join(format!("{}.maf.gz", file.file_id));
            tasks.spawn(async move { download_mutation_file(&client, &file, &path).await });
        }
        while let Some(result) = tasks.join_next().await {
            let (file_id, hash, length) = result.context("GDC download task failed")??;
            downloaded.insert(file_id, (hash, length));
        }
    }
    let (mutation_file_set_hash, mutation_file_byte_length) = mutation_set_commitment(&downloaded)?;
    let manifest = AcquisitionManifest {
        schema_version: 1,
        source_id: "tcga-gbm-2013".to_owned(),
        api_root: API_ROOT.to_owned(),
        status,
        case_query,
        case_response_hash: Digest::sha256(&case_bytes),
        case_response_byte_length: u64::try_from(case_bytes.len())?,
        case_count: cases.data.hits.len(),
        mutation_file_query: file_query,
        mutation_catalog_hash: Digest::sha256(&file_bytes),
        mutation_catalog_byte_length: u64::try_from(file_bytes.len())?,
        mutation_file_count: downloaded.len(),
        mutation_file_byte_length,
        mutation_file_set_hash,
    };
    write_json_new(&output_directory.join("acquisition.json"), &manifest)?;
    println!(
        "acquired {} TCGA-GBM cases and {} open mutation files from {}",
        manifest.case_count, manifest.mutation_file_count, manifest.status.data_release
    );
    Ok(())
}

pub fn derive_baseline(source_directory: &Path, registry_path: &Path, output: &Path) -> Result<()> {
    let registry_bytes = fs::read(registry_path)
        .with_context(|| format!("read registry {}", registry_path.display()))?;
    let registry = CancerDatasetRegistry::from_slice(&registry_bytes)
        .context("validate Cancer World dataset registry")?;
    if !registry
        .sources
        .iter()
        .any(|source| source.source_id == "tcga-gbm-2013")
    {
        bail!("Cancer World registry does not contain tcga-gbm-2013");
    }
    let manifest: AcquisitionManifest = read_json(&source_directory.join("acquisition.json"))?;
    if manifest.schema_version != 1 || manifest.source_id != "tcga-gbm-2013" {
        bail!("unsupported TCGA-GBM acquisition manifest");
    }
    let case_bytes = verified_raw(
        &source_directory.join("cases.json"),
        manifest.case_response_hash,
        manifest.case_response_byte_length,
    )?;
    let file_bytes = verified_raw(
        &source_directory.join("mutation-files.json"),
        manifest.mutation_catalog_hash,
        manifest.mutation_catalog_byte_length,
    )?;
    let cases: GdcEnvelope<GdcCase> = serde_json::from_slice(&case_bytes)?;
    let mut files: GdcEnvelope<GdcFile> = serde_json::from_slice(&file_bytes)?;
    require_complete_page(&cases.data.pagination, cases.data.hits.len(), "cases")?;
    require_complete_page(
        &files.data.pagination,
        files.data.hits.len(),
        "mutation files",
    )?;
    files
        .data
        .hits
        .sort_by(|left, right| left.file_id.cmp(&right.file_id));
    validate_file_catalog(&files.data.hits)?;

    let mut patients = BTreeMap::new();
    for case in &cases.data.hits {
        let record = patient_from_case(case)?;
        if patients.insert(case.case_id.clone(), record).is_some() {
            bail!("duplicate TCGA-GBM patient {}", case.case_id);
        }
    }
    let mut observed_files = BTreeMap::new();
    for file in &files.data.hits {
        let case_id = &file.cases[0].case_id;
        let patient = patients
            .get_mut(case_id)
            .with_context(|| format!("mutation file references unknown patient {case_id}"))?;
        let path = source_directory
            .join("maf")
            .join(format!("{}.maf.gz", file.file_id));
        let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
        verify_mutation_bytes(file, &bytes)?;
        read_maf(&bytes, case_id, patient)
            .with_context(|| format!("normalize mutation file {}", file.file_id))?;
        observed_files.insert(
            file.file_id.clone(),
            (Digest::sha256(&bytes), u64::try_from(bytes.len())?),
        );
    }
    let (set_hash, total_bytes) = mutation_set_commitment(&observed_files)?;
    if set_hash != manifest.mutation_file_set_hash
        || total_bytes != manifest.mutation_file_byte_length
        || observed_files.len() != manifest.mutation_file_count
    {
        bail!("TCGA-GBM mutation file set differs from acquisition manifest");
    }

    let mut calibration_ids = Vec::new();
    let mut validation_ids = Vec::new();
    for case_id in patients.keys() {
        if held_out(case_id) {
            validation_ids.push(case_id.clone());
        } else {
            calibration_ids.push(case_id.clone());
        }
    }
    let complete_ids = patients.keys().cloned().collect::<Vec<_>>();
    let baseline = TcgaGbmBaseline {
        schema_version: 1,
        baseline_id: format!(
            "tcga-gbm-dr{}-patient-baseline-v1",
            manifest.status.data_release_version.major
        ),
        evidence_class: "retrospective_observational_aggregate".to_owned(),
        intended_use: "Population and somatic-variant baseline checks for Cancer World; not intervention-response calibration.".to_owned(),
        source_registry_hash: registry.content_digest()?,
        source: BaselineSource {
            source_id: manifest.source_id,
            data_release: manifest.status.data_release,
            data_release_date: manifest.status.data_release_version.release_date,
            api_commit: manifest.status.commit,
            api_version: manifest.status.tag,
            case_response_hash: manifest.case_response_hash,
            mutation_catalog_hash: manifest.mutation_catalog_hash,
            mutation_file_set_hash: manifest.mutation_file_set_hash,
            mutation_file_count: manifest.mutation_file_count,
        },
        split: PatientSplit {
            derivation_domain: SPLIT_DOMAIN.to_owned(),
            rule: "sha256(domain || 0x00 || GDC case UUID), first eight bytes modulo 5; zero is held out".to_owned(),
            calibration_patient_count: calibration_ids.len(),
            held_out_validation_patient_count: validation_ids.len(),
            calibration_patient_set_commitment: patient_set_commitment(&calibration_ids),
            held_out_validation_patient_set_commitment: patient_set_commitment(&validation_ids),
        },
        complete_cohort: summarize(&patients, &complete_ids)?,
        calibration_cohort: summarize(&patients, &calibration_ids)?,
        held_out_validation_cohort: summarize(&patients, &validation_ids)?,
        held_out_assessment: assess_holdout(&patients, &calibration_ids, &validation_ids)?,
        limitations: vec![
            "The split is a retrospective validation scaffold, not a prospective clinical trial.".to_owned(),
            "Days-to-death is summarized only among observed deaths; censoring is not treated as an event.".to_owned(),
            "Multiple open masked-MAF specimens for one patient are unioned before patient-level prevalence is calculated.".to_owned(),
            "Masked somatic variants do not capture expression, epigenetics, spatial state, treatment exposure, pharmacology, or causal response.".to_owned(),
            "No patient identifier or patient-level row is emitted by this aggregate artifact.".to_owned(),
        ],
    };
    write_json_new(output, &baseline)?;
    println!(
        "derived {} patients: {} calibration, {} held out; {} molecular profiles",
        baseline.complete_cohort.patient_count,
        baseline.calibration_cohort.patient_count,
        baseline.held_out_validation_cohort.patient_count,
        baseline.complete_cohort.molecularly_profiled_patient_count
    );
    Ok(())
}

fn patient_from_case(case: &GdcCase) -> Result<PatientRecord> {
    if case.project.project_id != PROJECT_ID || Uuid::parse_str(&case.case_id).is_err() {
        bail!("invalid TCGA-GBM case identity");
    }
    let demographic = case.demographic.as_ref();
    let vital_status = match demographic.and_then(|value| value.vital_status.as_deref()) {
        Some("Alive") => VitalStatus::Alive,
        Some("Dead") => VitalStatus::Dead,
        _ => VitalStatus::Unknown,
    };
    let age_at_diagnosis_days = demographic
        .and_then(|value| value.days_to_birth.as_ref())
        .and_then(GdcDay::whole)
        .and_then(i64::checked_abs)
        .or_else(|| {
            case.diagnoses
                .iter()
                .filter_map(|diagnosis| diagnosis.age_at_diagnosis.as_ref())
                .filter_map(GdcDay::whole)
                .filter(|days| *days >= 0)
                .min()
        });
    let days_to_death = demographic
        .and_then(|value| value.days_to_death.as_ref())
        .and_then(GdcDay::whole)
        .filter(|days| *days >= 0);
    let alive_follow_up_days = (vital_status == VitalStatus::Alive)
        .then(|| {
            case.diagnoses
                .iter()
                .filter_map(|diagnosis| diagnosis.days_to_last_follow_up.as_ref())
                .filter_map(GdcDay::whole)
                .filter(|days| *days >= 0)
                .max()
        })
        .flatten();
    Ok(PatientRecord {
        vital_status,
        age_at_diagnosis_days,
        days_to_death,
        alive_follow_up_days,
        ..PatientRecord::default()
    })
}

fn read_maf(bytes: &[u8], expected_case_id: &str, patient: &mut PatientRecord) -> Result<()> {
    patient.molecularly_profiled = true;
    let reader = BufReader::new(GzDecoder::new(bytes));
    let mut header = None;
    for line in reader.lines() {
        let line = line.context("read gzip MAF line")?;
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if header.is_none() {
            header = Some(MafColumns::from_header(&line)?);
            continue;
        }
        let columns = header.as_ref().context("MAF header missing")?;
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() <= columns.maximum_index {
            bail!("MAF record has fewer columns than its header");
        }
        if fields[columns.case_id] != expected_case_id {
            bail!("MAF record crosses its catalog patient boundary");
        }
        let filter = fields[columns.gdc_filter];
        if !filter.is_empty() && filter != "PASS" {
            continue;
        }
        let classification = fields[columns.variant_classification];
        if !protein_altering(classification) {
            continue;
        }
        let gene = fields[columns.gene].trim();
        if gene.is_empty() || gene == "Unknown" {
            continue;
        }
        patient.genes.insert(gene.to_owned());
        patient.variants.insert(format!(
            "{gene}|{}|{}|{}|{}",
            fields[columns.chromosome],
            fields[columns.start_position],
            fields[columns.reference_allele],
            fields[columns.tumor_allele]
        ));
    }
    if header.is_none() {
        bail!("MAF header missing");
    }
    Ok(())
}

struct MafColumns {
    gene: usize,
    chromosome: usize,
    start_position: usize,
    variant_classification: usize,
    reference_allele: usize,
    tumor_allele: usize,
    case_id: usize,
    gdc_filter: usize,
    maximum_index: usize,
}

impl MafColumns {
    fn from_header(header: &str) -> Result<Self> {
        let fields = header.split('\t').collect::<Vec<_>>();
        let find = |name: &str| {
            fields
                .iter()
                .position(|field| *field == name)
                .with_context(|| format!("MAF header lacks {name}"))
        };
        let values = [
            find("Hugo_Symbol")?,
            find("Chromosome")?,
            find("Start_Position")?,
            find("Variant_Classification")?,
            find("Reference_Allele")?,
            find("Tumor_Seq_Allele2")?,
            find("case_id")?,
            find("GDC_FILTER")?,
        ];
        Ok(Self {
            gene: values[0],
            chromosome: values[1],
            start_position: values[2],
            variant_classification: values[3],
            reference_allele: values[4],
            tumor_allele: values[5],
            case_id: values[6],
            gdc_filter: values[7],
            maximum_index: values.into_iter().max().context("empty MAF header")?,
        })
    }
}

fn protein_altering(classification: &str) -> bool {
    matches!(
        classification,
        "Frame_Shift_Del"
            | "Frame_Shift_Ins"
            | "In_Frame_Del"
            | "In_Frame_Ins"
            | "Missense_Mutation"
            | "Nonsense_Mutation"
            | "Nonstop_Mutation"
            | "Splice_Site"
            | "Translation_Start_Site"
    )
}

fn summarize(patients: &BTreeMap<String, PatientRecord>, ids: &[String]) -> Result<CohortSummary> {
    let selected = ids
        .iter()
        .map(|id| {
            patients
                .get(id)
                .context("patient split references missing case")
        })
        .collect::<Result<Vec<_>>>()?;
    let molecular = selected
        .iter()
        .copied()
        .filter(|patient| patient.molecularly_profiled)
        .collect::<Vec<_>>();
    let gene_counts = gene_counts(&molecular);
    let mut genes = gene_counts.into_iter().collect::<Vec<_>>();
    genes.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    genes.truncate(25);
    let profiled = molecular.len();
    let top_protein_altering_gene_prevalence = genes
        .into_iter()
        .map(|(gene, count)| GenePrevalence {
            gene,
            patients_with_variant: count,
            profiled_patients: profiled,
            prevalence_parts_per_million: prevalence_ppm(count, profiled),
        })
        .collect();
    let ages = selected
        .iter()
        .filter_map(|patient| patient.age_at_diagnosis_days)
        .collect::<Vec<_>>();
    let death_days = selected
        .iter()
        .filter_map(|patient| patient.days_to_death)
        .collect::<Vec<_>>();
    let alive_follow_up = selected
        .iter()
        .filter_map(|patient| patient.alive_follow_up_days)
        .collect::<Vec<_>>();
    let burdens = molecular
        .iter()
        .map(|patient| patient.variants.len())
        .collect::<Vec<_>>();
    Ok(CohortSummary {
        patient_count: selected.len(),
        dead_patient_count: selected
            .iter()
            .filter(|patient| patient.vital_status == VitalStatus::Dead)
            .count(),
        alive_patient_count: selected
            .iter()
            .filter(|patient| patient.vital_status == VitalStatus::Alive)
            .count(),
        unknown_vital_status_count: selected
            .iter()
            .filter(|patient| patient.vital_status == VitalStatus::Unknown)
            .count(),
        known_age_at_diagnosis_count: ages.len(),
        median_age_at_diagnosis_days: median(ages),
        known_days_to_death_count: death_days.len(),
        median_days_to_death_among_observed_deaths: median(death_days),
        known_alive_follow_up_count: alive_follow_up.len(),
        median_alive_follow_up_days: median(alive_follow_up),
        molecularly_profiled_patient_count: profiled,
        protein_altering_variant_count: molecular
            .iter()
            .map(|patient| patient.variants.len())
            .sum(),
        median_unique_protein_altering_variants_per_profiled_patient: median(burdens),
        top_protein_altering_gene_prevalence,
    })
}

fn assess_holdout(
    patients: &BTreeMap<String, PatientRecord>,
    calibration_ids: &[String],
    validation_ids: &[String],
) -> Result<HeldOutAssessment> {
    let calibration = profiled_patients(patients, calibration_ids)?;
    let validation = profiled_patients(patients, validation_ids)?;
    if calibration.is_empty() || validation.is_empty() {
        bail!("TCGA-GBM holdout assessment requires molecular profiles in both cohorts");
    }
    let calibration_counts = gene_counts(&calibration);
    let validation_counts = gene_counts(&validation);
    let mut selected = calibration_counts.iter().collect::<Vec<_>>();
    selected.sort_by(|left, right| right.1.cmp(left.1).then_with(|| left.0.cmp(right.0)));
    selected.truncate(25);
    let mut absolute_error_sum = 0_u64;
    let mut brier_sum = 0_u128;
    let predictions = selected
        .into_iter()
        .map(|(gene, calibration_count)| {
            let calibration_ppm = prevalence_ppm(*calibration_count, calibration.len());
            let held_out_ppm = prevalence_ppm(
                validation_counts.get(gene).copied().unwrap_or(0),
                validation.len(),
            );
            let absolute_error = calibration_ppm.abs_diff(held_out_ppm);
            absolute_error_sum += u64::from(absolute_error);
            let probability = u128::from(calibration_ppm);
            let observed_rate = u128::from(held_out_ppm);
            let complement = 1_000_000_u128 - probability;
            brier_sum += (observed_rate * complement * complement
                + (1_000_000_u128 - observed_rate) * probability * probability)
                / 1_000_000_000_000_u128;
            GenePrevalencePrediction {
                gene: gene.clone(),
                calibration_prevalence_parts_per_million: calibration_ppm,
                held_out_prevalence_parts_per_million: held_out_ppm,
                absolute_error_parts_per_million: absolute_error,
            }
        })
        .collect::<Vec<_>>();
    let count = u64::try_from(predictions.len())?;
    let mean_absolute = absolute_error_sum
        .checked_div(count)
        .and_then(|value| u32::try_from(value).ok())
        .context("holdout absolute-error mean overflow")?;
    let mean_brier = brier_sum
        .checked_div(u128::from(count))
        .and_then(|value| u32::try_from(value).ok())
        .context("holdout Brier-score mean overflow")?;
    Ok(HeldOutAssessment {
        predictor: "calibration_cohort_empirical_gene_prevalence".to_owned(),
        feature_selection: "top 25 protein-altering genes selected using calibration patients only"
            .to_owned(),
        evaluated_gene_count: predictions.len(),
        mean_absolute_prevalence_error_parts_per_million: mean_absolute,
        mean_brier_score_parts_per_million: mean_brier,
        predictions,
        interpretation: "This is the simple out-of-sample molecular baseline a future Cancer World genomic model must beat; it is not treatment-response validation.".to_owned(),
    })
}

fn profiled_patients<'a>(
    patients: &'a BTreeMap<String, PatientRecord>,
    ids: &[String],
) -> Result<Vec<&'a PatientRecord>> {
    let selected = ids
        .iter()
        .map(|id| {
            patients
                .get(id)
                .context("patient split references missing case")
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(selected
        .into_iter()
        .filter(|patient| patient.molecularly_profiled)
        .collect())
}

fn gene_counts(patients: &[&PatientRecord]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for patient in patients {
        for gene in &patient.genes {
            *counts.entry(gene.clone()).or_default() += 1;
        }
    }
    counts
}

fn prevalence_ppm(count: usize, cohort: usize) -> u32 {
    count
        .checked_mul(1_000_000)
        .and_then(|numerator| numerator.checked_div(cohort))
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(0)
}

fn median<T: Ord + Copy>(mut values: Vec<T>) -> Option<T> {
    values.sort_unstable();
    values.get(values.len() / 2).copied()
}

fn held_out(case_id: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(SPLIT_DOMAIN.as_bytes());
    hasher.update([0]);
    hasher.update(case_id.as_bytes());
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[..8].try_into().unwrap_or([1; 8])) % 5 == 0
}

fn patient_set_commitment(ids: &[String]) -> Digest {
    let mut hasher = Sha256::new();
    hasher.update(b"a-tiny-civilization/tcga-gbm/patient-set/v1");
    for id in ids {
        hasher.update([0]);
        hasher.update(id.as_bytes());
    }
    Digest::from_bytes(hasher.finalize().into())
}

async fn download_mutation_file(
    client: &Client,
    file: &GdcFile,
    path: &Path,
) -> Result<(String, Digest, u64)> {
    if path.exists() {
        let bytes = fs::read(path).with_context(|| format!("read partial {}", path.display()))?;
        verify_mutation_bytes(file, &bytes)?;
        return Ok((
            file.file_id.clone(),
            Digest::sha256(&bytes),
            u64::try_from(bytes.len())?,
        ));
    }
    let bytes = get_limited(
        client,
        &format!("{API_ROOT}/data/{}", file.file_id),
        64 * 1024 * 1024,
    )
    .await?;
    verify_mutation_bytes(file, &bytes)?;
    write_new(path, &bytes)?;
    Ok((
        file.file_id.clone(),
        Digest::sha256(&bytes),
        u64::try_from(bytes.len())?,
    ))
}

fn verify_mutation_bytes(file: &GdcFile, bytes: &[u8]) -> Result<()> {
    if u64::try_from(bytes.len())? != file.file_size {
        bail!("GDC mutation file {} has wrong byte length", file.file_id);
    }
    let actual_md5 = hex::encode(Md5::digest(bytes));
    if actual_md5 != file.md5sum {
        bail!("GDC mutation file {} failed MD5", file.file_id);
    }
    Ok(())
}

fn validate_file_catalog(files: &[GdcFile]) -> Result<()> {
    if files.is_empty() {
        bail!("GDC returned no open TCGA-GBM mutation files");
    }
    let mut ids = BTreeSet::new();
    for file in files {
        if Uuid::parse_str(&file.file_id).is_err()
            || !ids.insert(&file.file_id)
            || file.cases.len() != 1
            || Uuid::parse_str(&file.cases[0].case_id).is_err()
            || file.file_size == 0
            || file.md5sum.len() != 32
            || !file.md5sum.bytes().all(|byte| byte.is_ascii_hexdigit())
            || file.data_type != "Masked Somatic Mutation"
            || file.data_format != "MAF"
            || file.experimental_strategy != "WXS"
            || file.analysis.workflow_type != "Aliquot Ensemble Somatic Variant Merging and Masking"
            || !file.file_name.ends_with(".maf.gz")
        {
            bail!("GDC mutation catalog contains an unexpected file");
        }
    }
    Ok(())
}

fn mutation_set_commitment(files: &BTreeMap<String, (Digest, u64)>) -> Result<(Digest, u64)> {
    let mut hasher = Sha256::new();
    hasher.update(b"a-tiny-civilization/tcga-gbm/mutation-file-set/v1");
    let mut total = 0_u64;
    for (file_id, (digest, length)) in files {
        hasher.update([0]);
        hasher.update(file_id.as_bytes());
        hasher.update(digest.as_bytes());
        hasher.update(length.to_be_bytes());
        total = total
            .checked_add(*length)
            .context("mutation byte total overflow")?;
    }
    Ok((Digest::from_bytes(hasher.finalize().into()), total))
}

fn require_complete_page(pagination: &GdcPagination, hits: usize, label: &str) -> Result<()> {
    if pagination.count != hits || pagination.total != hits || hits == 0 || hits > 1000 {
        bail!("GDC {label} response is empty or paginated");
    }
    Ok(())
}

async fn get_query(
    client: &Client,
    url: &str,
    query: &[(&str, &str)],
    limit: usize,
) -> Result<Vec<u8>> {
    let mut request_url = reqwest::Url::parse(url).with_context(|| format!("parse {url}"))?;
    request_url
        .query_pairs_mut()
        .extend_pairs(query.iter().copied());
    let mut last_error = None;
    for attempt in 0..4_u64 {
        match client.get(request_url.clone()).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(response) => match response.bytes().await {
                    Ok(bytes) if bytes.len() <= limit => return Ok(bytes.to_vec()),
                    Ok(_) => bail!("GDC response exceeded byte limit"),
                    Err(error) => last_error = Some(error.to_string()),
                },
                Err(error) => last_error = Some(error.to_string()),
            },
            Err(error) => last_error = Some(error.to_string()),
        }
        tokio::time::sleep(std::time::Duration::from_millis(250 * (attempt + 1))).await;
    }
    bail!(
        "request {url} failed after retries: {}",
        last_error.unwrap_or_else(|| "unknown transport failure".to_owned())
    )
}

async fn get_limited(client: &Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    get_query(client, url, &[], limit).await
}

fn project_filter() -> String {
    format!(
        "{{\"op\":\"in\",\"content\":{{\"field\":\"project.project_id\",\"value\":[\"{PROJECT_ID}\"]}}}}"
    )
}

fn mutation_file_filter() -> String {
    format!(
        "{{\"op\":\"and\",\"content\":[{{\"op\":\"in\",\"content\":{{\"field\":\"cases.project.project_id\",\"value\":[\"{PROJECT_ID}\"]}}}},{{\"op\":\"in\",\"content\":{{\"field\":\"access\",\"value\":[\"open\"]}}}},{{\"op\":\"in\",\"content\":{{\"field\":\"data_type\",\"value\":[\"Masked Somatic Mutation\"]}}}}]}}"
    )
}

fn verified_raw(path: &Path, expected_hash: Digest, expected_length: u64) -> Result<Vec<u8>> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    if Digest::sha256(&bytes) != expected_hash || u64::try_from(bytes.len())? != expected_length {
        bail!(
            "source artifact {} failed its acquisition commitment",
            path.display()
        );
    }
    Ok(bytes)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("decode {}", path.display()))
}

fn write_json_new(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_new(path, &bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create new artifact {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("write artifact {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("sync artifact {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patient_split_is_stable_and_nontrivial() {
        let ids = (0..100)
            .map(|index| format!("00000000-0000-4000-8000-{index:012}"))
            .collect::<Vec<_>>();
        let held_out_count = ids.iter().filter(|id| held_out(id)).count();
        assert!((10..=30).contains(&held_out_count));
        assert_eq!(held_out(&ids[10]), held_out(&ids[10]));
    }

    #[test]
    fn only_declared_protein_altering_classes_enter_the_baseline() {
        assert!(protein_altering("Missense_Mutation"));
        assert!(protein_altering("Frame_Shift_Del"));
        assert!(!protein_altering("Silent"));
        assert!(!protein_altering("Intron"));
    }

    #[test]
    fn median_uses_the_observed_middle_without_inventing_interpolation() {
        assert_eq!(median(vec![9, 1, 4]), Some(4));
        assert_eq!(median(vec![1, 2, 8, 9]), Some(8));
        assert_eq!(median::<u32>(Vec::new()), None);
    }
}
