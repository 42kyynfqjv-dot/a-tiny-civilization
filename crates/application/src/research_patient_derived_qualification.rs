use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use thiserror::Error;
use uuid::Uuid;
use world_domain::{
    CANCER_PATIENT_DERIVED_MOLECULAR_QUALIFICATION_METHOD_VERSION,
    CANCER_PATIENT_DERIVED_MOLECULAR_QUALIFICATION_SCHEMA_VERSION, CancerMolecularTarget,
    CancerPatientDerivedMolecularQualification, CancerPatientDerivedTargetObservation,
    CancerPatientDerivedTargetStatus, CancerResearchContractError, CancerResearchEvidenceKind,
    CancerResearchEvidenceReference, Digest,
};

use crate::CancerPatientDerivedMolecularCandidate;

const STUDY_ID: &str = "PDC000711";
const STUDY_VERSION_ID: &str = "ec0e442b-a0b8-4dc7-a4ba-6b5409fc68de";
const SOURCE_FILE_ID: &str = "86e9b7f6-0776-4cb7-b761-dee14321b318";
const SOURCE_FILE_MD5: &str = "333eef379eaea258efca326d579eef21";
const DERIVED_FILE_NAME: &str = "pdc000711-gbm-proteome.tsv";
const MANIFEST_ID: &str = "pdc000711-hcmi-gbm-proteome-source-v1";
const EXPECTED_ROWS: usize = 12_342;
const EXPECTED_MODEL_COLUMNS: usize = 30;
const EXPECTED_ANNOTATION_COLUMNS: [&str; 4] =
    ["T: Index", "T: NumberPSM", "T: ProteinID", "T: MaxPepProb"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
    dimensions: Dimensions,
    join_provenance: Vec<JoinProvenance>,
    limitations: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transformation {
    model_selection: String,
    column_order: String,
    missing_value_policy: String,
    annotation_columns_preserved: Vec<String>,
    numeric_values_reparsed: bool,
    imputation_applied: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Dimensions {
    data_rows: usize,
    model_columns: usize,
    annotation_columns: usize,
    total_columns: usize,
    observed_model_cells: u64,
    missing_model_cells: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Default)]
struct TargetAggregate {
    observed_models: BTreeSet<usize>,
    protein_ids: BTreeSet<String>,
}

/// Validated, immutable index over the exact PDC000711 patient-derived GBM
/// proteome artifact. The index never performs aliases, fuzzy matching, or
/// correction of source labels such as Excel-corrupted gene symbols.
pub struct CancerPdc000711Qualifier {
    source: CancerResearchEvidenceReference,
    study_version_id: Uuid,
    source_file_id: Uuid,
    targets: BTreeMap<String, TargetAggregate>,
}

impl CancerPdc000711Qualifier {
    pub fn new(
        metadata_bytes: &[u8],
        matrix_bytes: &[u8],
    ) -> Result<Self, CancerPatientDerivedQualificationError> {
        let metadata: DerivedMetadata = serde_json::from_slice(metadata_bytes)?;
        let matrix_hash = Digest::sha256(matrix_bytes);
        validate_metadata(&metadata, matrix_bytes, matrix_hash)?;
        let targets = parse_matrix(&metadata, matrix_bytes)?;
        let source = CancerResearchEvidenceReference {
            kind: CancerResearchEvidenceKind::RawDataset,
            source_id: format!(
                "pdc://{STUDY_ID}/{SOURCE_FILE_ID}/{DERIVED_FILE_NAME}/sha256/{matrix_hash}"
            ),
            content_hash: matrix_hash,
        };
        Ok(Self {
            source,
            study_version_id: Uuid::parse_str(STUDY_VERSION_ID)
                .map_err(|_| invalid("pinned PDC study-version UUID is malformed"))?,
            source_file_id: Uuid::parse_str(SOURCE_FILE_ID)
                .map_err(|_| invalid("pinned PDC source-file UUID is malformed"))?,
            targets,
        })
    }

    pub fn qualify(
        &self,
        candidate: &CancerPatientDerivedMolecularCandidate,
    ) -> Result<CancerPatientDerivedMolecularQualification, CancerPatientDerivedQualificationError>
    {
        if candidate.request_id != candidate.contribution.request_id
            || candidate.artifact_hash != candidate.contribution.canonical_hash()?
            || candidate.contribution.molecular_targets.is_empty()
        {
            return Err(invalid(
                "patient-derived qualification candidate disagrees with its contribution",
            ));
        }
        let target_observations = lookup_targets(
            &self.targets,
            &candidate.contribution.molecular_targets,
            EXPECTED_MODEL_COLUMNS,
        )?;
        let qualification = CancerPatientDerivedMolecularQualification {
            schema_version: CANCER_PATIENT_DERIVED_MOLECULAR_QUALIFICATION_SCHEMA_VERSION,
            method_version: CANCER_PATIENT_DERIVED_MOLECULAR_QUALIFICATION_METHOD_VERSION,
            qualification_id: CancerPatientDerivedMolecularQualification::deterministic_id(
                candidate.request_id,
                CANCER_PATIENT_DERIVED_MOLECULAR_QUALIFICATION_METHOD_VERSION,
            ),
            world_id: candidate.world_id,
            request_id: candidate.request_id,
            artifact_hash: candidate.artifact_hash,
            source: self.source.clone(),
            pdc_study_id: STUDY_ID.to_owned(),
            study_version_id: self.study_version_id,
            source_file_id: self.source_file_id,
            source_file_md5: SOURCE_FILE_MD5.to_owned(),
            cohort_model_count: u16::try_from(EXPECTED_MODEL_COLUMNS)
                .map_err(|_| invalid("PDC model count exceeds the qualification schema"))?,
            target_observations,
            limitations: vec![
                "This is an exact-symbol molecular presence check in patient-derived GBM models, not treatment-response evidence.".to_owned(),
                "Reported proteomics values are relative measurements; this qualification records coverage only and does not compare abundance with NCI-60.".to_owned(),
                "Blank source fields remain missing data and are never converted to zero, imputed, or treated as biological absence.".to_owned(),
                "Source T: Index labels are matched exactly; aliases and spreadsheet-corrupted labels are neither inferred nor silently repaired.".to_owned(),
                "These are laboratory cancer models, not treated patients; the result cannot establish mechanism, efficacy, safety, clinical benefit, or a cure.".to_owned(),
            ],
        };
        qualification.validate_against(&candidate.contribution)?;
        Ok(qualification)
    }
}

fn validate_metadata(
    metadata: &DerivedMetadata,
    matrix_bytes: &[u8],
    matrix_hash: Digest,
) -> Result<(), CancerPatientDerivedQualificationError> {
    let expected_cells = u64::try_from(EXPECTED_ROWS * EXPECTED_MODEL_COLUMNS)
        .map_err(|_| invalid("PDC expected cell count overflowed"))?;
    let annotations = EXPECTED_ANNOTATION_COLUMNS
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    if metadata.schema_version != 1
        || metadata.artifact_id != format!("pdc000711-hcmi-gbm-proteome:{matrix_hash}")
        || metadata.artifact_file_name != DERIVED_FILE_NAME
        || metadata.media_type != "text/tab-separated-values; charset=utf-8"
        || metadata.artifact_content_address != format!("sha256:{matrix_hash}")
        || metadata.artifact_sha256 != matrix_hash
        || metadata.artifact_byte_length
            != u64::try_from(matrix_bytes.len())
                .map_err(|_| invalid("PDC artifact byte length overflowed"))?
        || metadata.source.manifest_id != MANIFEST_ID
        || metadata.source.pdc_study_id != STUDY_ID
        || metadata.source.study_version_uuid != STUDY_VERSION_ID
        || metadata.source.file_id != SOURCE_FILE_ID
        || metadata.source.manifest_sha256 == Digest::ZERO
        || metadata.source.source_set_sha256 == Digest::ZERO
        || metadata.source.source_file_sha256 == Digest::ZERO
        || metadata.source.biospecimen_metadata_sha256 == Digest::ZERO
        || metadata.transformation.model_selection.trim().is_empty()
        || metadata.transformation.column_order.trim().is_empty()
        || metadata
            .transformation
            .missing_value_policy
            .trim()
            .is_empty()
        || metadata.transformation.annotation_columns_preserved != annotations
        || metadata.transformation.numeric_values_reparsed
        || metadata.transformation.imputation_applied
        || metadata.dimensions.data_rows != EXPECTED_ROWS
        || metadata.dimensions.model_columns != EXPECTED_MODEL_COLUMNS
        || metadata.dimensions.annotation_columns != EXPECTED_ANNOTATION_COLUMNS.len()
        || metadata.dimensions.total_columns
            != EXPECTED_MODEL_COLUMNS + EXPECTED_ANNOTATION_COLUMNS.len()
        || metadata.dimensions.observed_model_cells + metadata.dimensions.missing_model_cells
            != expected_cells
        || metadata.limitations.len() != 4
        || metadata
            .limitations
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > 1_024)
    {
        return Err(invalid(
            "PDC000711 derived metadata disagrees with the pinned artifact contract",
        ));
    }
    validate_join_provenance(&metadata.join_provenance)
}

fn validate_join_provenance(
    joins: &[JoinProvenance],
) -> Result<(), CancerPatientDerivedQualificationError> {
    let mut source_columns = BTreeSet::new();
    let mut headers = BTreeSet::new();
    for (expected_index, join) in joins.iter().enumerate() {
        if join.derived_column_index != expected_index
            || join.source_column_index >= 75
            || !source_columns.insert(join.source_column_index)
            || !headers.insert(join.matrix_header.as_str())
            || join.join_field != "case_submitter_id"
            || join.matrix_header != join.case_submitter_id
            || join.matrix_header.trim().is_empty()
            || join.disease_type != "Glioblastoma"
            || join.primary_site != "Brain"
            || !matches!(
                join.sample_type.as_str(),
                "Next Generation Cancer Model" | "Expanded Next Generation Cancer Model"
            )
            || Uuid::parse_str(&join.case_id).is_err()
            || Uuid::parse_str(&join.sample_id).is_err()
            || Uuid::parse_str(&join.aliquot_id).is_err()
            || join.sample_submitter_id.trim().is_empty()
            || join.aliquot_submitter_id.trim().is_empty()
        {
            return Err(invalid(
                "PDC000711 biospecimen join provenance is incomplete or inconsistent",
            ));
        }
    }
    if joins.len() != EXPECTED_MODEL_COLUMNS
        || joins
            .windows(2)
            .any(|pair| pair[0].source_column_index >= pair[1].source_column_index)
    {
        return Err(invalid(
            "PDC000711 join provenance does not retain 30 source-ordered models",
        ));
    }
    Ok(())
}

fn parse_matrix(
    metadata: &DerivedMetadata,
    matrix_bytes: &[u8],
) -> Result<BTreeMap<String, TargetAggregate>, CancerPatientDerivedQualificationError> {
    let text = std::str::from_utf8(matrix_bytes)?;
    if !text.ends_with('\n') || text.contains('\r') {
        return Err(invalid(
            "PDC000711 matrix is not canonical LF-delimited TSV",
        ));
    }
    let mut lines = text.split_terminator('\n');
    let header = lines
        .next()
        .ok_or_else(|| invalid("PDC000711 matrix omitted its header"))?
        .split('\t')
        .collect::<Vec<_>>();
    let expected_columns = metadata.dimensions.total_columns;
    if header.len() != expected_columns
        || header[..metadata.dimensions.model_columns]
            != metadata
                .join_provenance
                .iter()
                .map(|join| join.matrix_header.as_str())
                .collect::<Vec<_>>()
        || header[metadata.dimensions.model_columns..] != EXPECTED_ANNOTATION_COLUMNS
    {
        return Err(invalid(
            "PDC000711 matrix header disagrees with its join provenance",
        ));
    }
    let index_column = metadata.dimensions.model_columns;
    let protein_column = index_column + 2;
    let mut targets = BTreeMap::<String, TargetAggregate>::new();
    let mut row_count = 0_usize;
    let mut observed_cells = 0_u64;
    let mut missing_cells = 0_u64;
    for line in lines {
        if line.is_empty() {
            return Err(invalid("PDC000711 matrix contains an empty data row"));
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != expected_columns {
            return Err(invalid("PDC000711 matrix row width changed"));
        }
        for value in &fields[..metadata.dimensions.model_columns] {
            if value.is_empty() {
                missing_cells += 1;
            } else {
                observed_cells += 1;
            }
        }
        let source_label = fields[index_column];
        if !source_label.is_empty() {
            let aggregate = targets.entry(source_label.to_owned()).or_default();
            if !fields[protein_column].is_empty() {
                aggregate
                    .protein_ids
                    .insert(fields[protein_column].to_owned());
            }
            for (model_index, value) in fields[..metadata.dimensions.model_columns]
                .iter()
                .enumerate()
            {
                if !value.is_empty() {
                    aggregate.observed_models.insert(model_index);
                }
            }
        }
        row_count += 1;
    }
    if row_count != metadata.dimensions.data_rows
        || observed_cells != metadata.dimensions.observed_model_cells
        || missing_cells != metadata.dimensions.missing_model_cells
    {
        return Err(invalid(
            "PDC000711 matrix dimensions or missingness disagree with metadata",
        ));
    }
    Ok(targets)
}

fn lookup_targets(
    index: &BTreeMap<String, TargetAggregate>,
    targets: &[CancerMolecularTarget],
    model_count: usize,
) -> Result<Vec<CancerPatientDerivedTargetObservation>, CancerPatientDerivedQualificationError> {
    let model_count = u16::try_from(model_count)
        .map_err(|_| invalid("PDC model count exceeds the qualification schema"))?;
    targets
        .iter()
        .map(|target| match index.get(&target.gene_symbol) {
            None => Ok(CancerPatientDerivedTargetObservation {
                target: target.clone(),
                protein_ids: Vec::new(),
                assayed_model_count: 0,
                observed_model_count: 0,
                status: CancerPatientDerivedTargetStatus::Unresolved,
            }),
            Some(aggregate) if aggregate.protein_ids.is_empty() => Err(invalid(
                "an exact PDC target row omitted its ProteinID provenance",
            )),
            Some(aggregate) => {
                let observed_model_count = u16::try_from(aggregate.observed_models.len())
                    .map_err(|_| invalid("PDC target coverage exceeds the cohort size"))?;
                if observed_model_count > model_count {
                    return Err(invalid("PDC target coverage exceeds the cohort size"));
                }
                let protein_ids = aggregate.protein_ids.iter().cloned().collect::<Vec<_>>();
                if protein_ids.len() > 32
                    || protein_ids
                        .iter()
                        .any(|value| value.len() > 128 || value.trim() != value)
                {
                    return Err(invalid(
                        "PDC target ProteinID provenance exceeds the bounded schema",
                    ));
                }
                Ok(CancerPatientDerivedTargetObservation {
                    target: target.clone(),
                    protein_ids,
                    assayed_model_count: model_count,
                    observed_model_count,
                    status: if observed_model_count == 0 {
                        CancerPatientDerivedTargetStatus::NotDetected
                    } else {
                        CancerPatientDerivedTargetStatus::Observed
                    },
                })
            }
        })
        .collect()
}

fn invalid(message: impl Into<String>) -> CancerPatientDerivedQualificationError {
    CancerPatientDerivedQualificationError::InvalidArtifact(message.into())
}

#[derive(Debug, Error)]
pub enum CancerPatientDerivedQualificationError {
    #[error("decode PDC000711 derived metadata: {0}")]
    MetadataJson(#[from] serde_json::Error),
    #[error("decode PDC000711 matrix as UTF-8: {0}")]
    MatrixUtf8(#[from] std::str::Utf8Error),
    #[error("invalid PDC000711 qualification artifact: {0}")]
    InvalidArtifact(String),
    #[error(transparent)]
    Contract(#[from] CancerResearchContractError),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_lookup_unions_rows_without_alias_inference() {
        let mut index = BTreeMap::new();
        index.insert(
            "EGFR".to_owned(),
            TargetAggregate {
                observed_models: BTreeSet::from([0, 2]),
                protein_ids: BTreeSet::from(["P00533".to_owned(), "Q9TEST".to_owned()]),
            },
        );
        index.insert(
            "1-Mar".to_owned(),
            TargetAggregate {
                observed_models: BTreeSet::from([1]),
                protein_ids: BTreeSet::from(["P11111".to_owned()]),
            },
        );
        let observations = lookup_targets(
            &index,
            &[
                CancerMolecularTarget {
                    gene_symbol: "EGFR".to_owned(),
                },
                CancerMolecularTarget {
                    gene_symbol: "MARCH1".to_owned(),
                },
            ],
            3,
        )
        .expect("exact target lookup");
        assert_eq!(
            observations[0].status,
            CancerPatientDerivedTargetStatus::Observed
        );
        assert_eq!(observations[0].observed_model_count, 2);
        assert_eq!(observations[0].protein_ids, ["P00533", "Q9TEST"]);
        assert_eq!(
            observations[1].status,
            CancerPatientDerivedTargetStatus::Unresolved
        );
        assert_eq!(observations[1].assayed_model_count, 0);
    }

    #[test]
    fn exact_assay_row_with_no_measurement_is_not_detected() {
        let index = BTreeMap::from([(
            "PTEN".to_owned(),
            TargetAggregate {
                observed_models: BTreeSet::new(),
                protein_ids: BTreeSet::from(["P60484".to_owned()]),
            },
        )]);
        let observations = lookup_targets(
            &index,
            &[CancerMolecularTarget {
                gene_symbol: "PTEN".to_owned(),
            }],
            30,
        )
        .expect("exact target lookup");
        assert_eq!(
            observations[0].status,
            CancerPatientDerivedTargetStatus::NotDetected
        );
        assert_eq!(observations[0].assayed_model_count, 30);
        assert_eq!(observations[0].observed_model_count, 0);
    }

    #[test]
    #[ignore = "requires the locally acquired and derived PDC000711 proteome"]
    fn real_pdc000711_artifact_validates_and_indexes_exact_targets() {
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let directory = repository.join("data/derived-cache/pdc000711-hcmi-gbm-proteome");
        let metadata = std::fs::read(directory.join("pdc000711-gbm-proteome.metadata.json"))
            .expect("derived metadata");
        let matrix =
            std::fs::read(directory.join("pdc000711-gbm-proteome.tsv")).expect("derived matrix");
        let qualifier =
            CancerPdc000711Qualifier::new(&metadata, &matrix).expect("validated real qualifier");

        assert_eq!(
            qualifier.source.content_hash.to_string(),
            "469f82d518f7b351f002ff671ec139ae97a8e389e0f296a644d40872935ebeda"
        );
        for (symbol, protein_id) in [
            ("EGFR", "NP_005219"),
            ("PTEN", "NP_001291646"),
            ("TP53", "NP_000537"),
        ] {
            let target = qualifier.targets.get(symbol).expect("exact target");
            assert_eq!(target.observed_models.len(), EXPECTED_MODEL_COLUMNS);
            assert!(target.protein_ids.contains(protein_id));
        }
        assert!(!qualifier.targets.contains_key("MARCH1"));
        assert!(qualifier.targets.contains_key("1-Mar"));
    }
}
