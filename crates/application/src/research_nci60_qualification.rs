use serde::{Deserialize, Serialize};
use thiserror::Error;
use world_domain::{
    CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION,
    CANCER_NCI60_RESPONSE_QUALIFICATION_SCHEMA_VERSION, CancerNci60CnsLine,
    CancerNci60ObservedRank, CancerNci60ResponseQualification, CancerNciInterventionIdentity,
    CancerResearchContractError, CancerResearchEvidenceKind, CancerResearchEvidenceReference,
    Digest,
};

use crate::CancerNci60QualificationCandidate;

const EXPECTED_CATALOGUE_ID: &str = "nci-cellminer-2-15-cns-challenge-catalogue-v1";
const EXPECTED_CATALOGUE_SHA256: &str =
    "ab9f8087135aeb6a62c1d351d088a492b3dafb1c01dd4c37af0d0659be5362a5";
const EXPECTED_ANSWER_KEY_SHA256: &str =
    "559d52f45f18901d3ce8fb844f99cd88045ccd3fbd0c99cb7e8139b85e59f4ce";
const CHALLENGE_ANSWER_DOMAIN: &str =
    "a-tiny-civilization/nci-cellminer/cns-response-challenge-answers/v1";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AnswerKey {
    schema_version: u16,
    answer_key_id: String,
    evidence_class: String,
    intended_use: String,
    source_registry_hash: Digest,
    source: serde_json::Value,
    cns_cell_lines: Vec<String>,
    ranking_rule: String,
    single_agent_response_measure: String,
    combination_response_measure: String,
    catalogue_reference: CatalogueReference,
    single_agent_answers: Vec<SingleAnswer>,
    combination_answers: Vec<CombinationAnswer>,
    leakage_boundary: LeakageBoundary,
    limitations: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CatalogueReference {
    catalogue_id: String,
    catalogue_artifact_sha256: Digest,
    answer_payload_commitment: Digest,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LeakageBoundary {
    access_class: String,
    allowed_in_model_context: bool,
    contains_observed_response_values: bool,
    contains_derived_rank_labels: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SingleAnswer {
    challenge_id: String,
    nsc: u64,
    observations: Vec<SingleObservation>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SingleObservation {
    cell_line: String,
    activity_z_milli: i64,
    descending_response_rank: u8,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CombinationAnswer {
    challenge_id: String,
    nsc_1: u64,
    nsc_2: u64,
    observations: Vec<CombinationObservation>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CombinationObservation {
    cell_line: String,
    combo_score_milli: i64,
    descending_interaction_rank: u8,
    interaction_direction: InteractionDirection,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum InteractionDirection {
    Negative,
    Zero,
    Positive,
}

#[derive(Serialize)]
struct ChallengeAnswerPayload<'a> {
    domain: &'static str,
    single_agent_answers: &'a [SingleAnswer],
    combination_answers: &'a [CombinationAnswer],
}

#[derive(Clone)]
struct PreparedAnswer {
    source_candidate_id: String,
    observed_ranks: Vec<CancerNci60ObservedRank>,
    expected_prompt_challenge: serde_json::Value,
}

/// Fully validates and indexes the multi-megabyte runtime-isolated answer key
/// once per worker batch instead of once per prediction.
pub struct CancerNci60Qualifier {
    answer_key_hash: Digest,
    single_answers: std::collections::BTreeMap<u64, PreparedAnswer>,
    combination_answers: std::collections::BTreeMap<(u64, u64), PreparedAnswer>,
}

impl CancerNci60Qualifier {
    pub fn new(
        catalogue_bytes: &[u8],
        answer_key_bytes: &[u8],
    ) -> Result<Self, CancerNci60QualificationError> {
        let answer_key: AnswerKey = serde_json::from_slice(answer_key_bytes)?;
        validate_answer_key(&answer_key, catalogue_bytes, answer_key_bytes)?;
        let catalogue: serde_json::Value = serde_json::from_slice(catalogue_bytes)?;
        let prompt_challenges = prepare_prompt_challenges(&catalogue)?;
        let mut single_answers = std::collections::BTreeMap::new();
        for answer in &answer_key.single_agent_answers {
            validate_single_observations(&answer.observations)?;
            let observed_ranks = answer
                .observations
                .iter()
                .map(|observation| {
                    Ok::<_, CancerNci60QualificationError>(CancerNci60ObservedRank {
                        cell_line: parse_line(&observation.cell_line)?,
                        rank: observation.descending_response_rank,
                    })
                })
                .collect::<Result<_, _>>()?;
            let (catalogue_candidate_id, expected_prompt_challenge) = prompt_challenges
                .0
                .get(&answer.nsc)
                .ok_or(CancerNci60QualificationError::InvalidAnswerKey)?;
            if catalogue_candidate_id != &answer.challenge_id
                || single_answers
                    .insert(
                        answer.nsc,
                        PreparedAnswer {
                            source_candidate_id: answer.challenge_id.clone(),
                            observed_ranks,
                            expected_prompt_challenge: expected_prompt_challenge.clone(),
                        },
                    )
                    .is_some()
            {
                return Err(CancerNci60QualificationError::InvalidAnswerKey);
            }
        }
        let mut combination_answers = std::collections::BTreeMap::new();
        for answer in &answer_key.combination_answers {
            validate_combination_observations(&answer.observations)?;
            let observed_ranks = answer
                .observations
                .iter()
                .map(|observation| {
                    Ok::<_, CancerNci60QualificationError>(CancerNci60ObservedRank {
                        cell_line: parse_line(&observation.cell_line)?,
                        rank: observation.descending_interaction_rank,
                    })
                })
                .collect::<Result<_, _>>()?;
            let (catalogue_candidate_id, expected_prompt_challenge) = prompt_challenges
                .1
                .get(&(answer.nsc_1, answer.nsc_2))
                .ok_or(CancerNci60QualificationError::InvalidAnswerKey)?;
            if catalogue_candidate_id != &answer.challenge_id
                || combination_answers
                    .insert(
                        (answer.nsc_1, answer.nsc_2),
                        PreparedAnswer {
                            source_candidate_id: answer.challenge_id.clone(),
                            observed_ranks,
                            expected_prompt_challenge: expected_prompt_challenge.clone(),
                        },
                    )
                    .is_some()
            {
                return Err(CancerNci60QualificationError::InvalidAnswerKey);
            }
        }
        Ok(Self {
            answer_key_hash: Digest::sha256(answer_key_bytes),
            single_answers,
            combination_answers,
        })
    }

    pub fn qualify(
        &self,
        candidate: &CancerNci60QualificationCandidate,
    ) -> Result<CancerNci60ResponseQualification, CancerNci60QualificationError> {
        qualify_with_prepared_key(candidate, self)
    }
}

type PromptChallengeMap<K> = std::collections::BTreeMap<K, (String, serde_json::Value)>;
type PreparedPromptChallenges = (PromptChallengeMap<u64>, PromptChallengeMap<(u64, u64)>);

fn prepare_prompt_challenges(
    catalogue: &serde_json::Value,
) -> Result<PreparedPromptChallenges, CancerNci60QualificationError> {
    let common = |candidate: serde_json::Value, field: &str| {
        let mut document = serde_json::json!({
            "schema_version": 1,
            "catalogue_id": catalogue["catalogue_id"].clone(),
            "source_candidate_id": candidate["challenge_id"].clone(),
            "evidence_class": catalogue["evidence_class"].clone(),
            "intended_use": catalogue["intended_use"].clone(),
            "cns_cell_lines": catalogue["cns_cell_lines"].clone(),
            "limitations": catalogue["limitations"].clone(),
        });
        document
            .as_object_mut()
            .expect("prompt challenge is an object")
            .insert(field.to_owned(), candidate);
        document
    };
    let single_candidates = catalogue["single_agent_candidates"]
        .as_array()
        .ok_or(CancerNci60QualificationError::InvalidAnswerKey)?;
    let mut singles = PromptChallengeMap::new();
    for candidate in single_candidates {
        let nsc = candidate["compound"]["nsc"]
            .as_u64()
            .filter(|nsc| *nsc > 0)
            .ok_or(CancerNci60QualificationError::InvalidAnswerKey)?;
        let candidate_id = candidate["challenge_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(CancerNci60QualificationError::InvalidAnswerKey)?;
        if singles
            .insert(
                nsc,
                (
                    candidate_id.to_owned(),
                    common(candidate.clone(), "single_agent"),
                ),
            )
            .is_some()
        {
            return Err(CancerNci60QualificationError::InvalidAnswerKey);
        }
    }
    let combination_candidates = catalogue["combination_candidates"]
        .as_array()
        .ok_or(CancerNci60QualificationError::InvalidAnswerKey)?;
    let mut combinations = PromptChallengeMap::new();
    for candidate in combination_candidates {
        let nsc_1 = candidate["first"]["nsc"]
            .as_u64()
            .filter(|nsc| *nsc > 0)
            .ok_or(CancerNci60QualificationError::InvalidAnswerKey)?;
        let nsc_2 = candidate["second"]["nsc"]
            .as_u64()
            .filter(|nsc| *nsc > nsc_1)
            .ok_or(CancerNci60QualificationError::InvalidAnswerKey)?;
        let candidate_id = candidate["challenge_id"]
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or(CancerNci60QualificationError::InvalidAnswerKey)?;
        if combinations
            .insert(
                (nsc_1, nsc_2),
                (
                    candidate_id.to_owned(),
                    common(candidate.clone(), "combination"),
                ),
            )
            .is_some()
        {
            return Err(CancerNci60QualificationError::InvalidAnswerKey);
        }
    }
    Ok((singles, combinations))
}

/// Opens exactly one preregistered prediction against the qualification-only
/// answer key. The raw answer values are consumed solely to verify ordering and
/// are not copied into the durable result.
pub fn qualify_cancer_nci60_prediction(
    candidate: &CancerNci60QualificationCandidate,
    catalogue_bytes: &[u8],
    answer_key_bytes: &[u8],
) -> Result<CancerNci60ResponseQualification, CancerNci60QualificationError> {
    CancerNci60Qualifier::new(catalogue_bytes, answer_key_bytes)?.qualify(candidate)
}

fn qualify_with_prepared_key(
    candidate: &CancerNci60QualificationCandidate,
    qualifier: &CancerNci60Qualifier,
) -> Result<CancerNci60ResponseQualification, CancerNci60QualificationError> {
    if candidate.request_id != candidate.contribution.request_id
        || candidate.artifact_hash != candidate.contribution.canonical_hash()?
    {
        return Err(CancerNci60QualificationError::InvalidCandidate);
    }
    let prediction = candidate
        .contribution
        .nci60_response_prediction
        .as_ref()
        .ok_or(CancerNci60QualificationError::InvalidCandidate)?;
    prediction.validate()?;
    let answer = match prediction.intervention {
        CancerNciInterventionIdentity::SingleAgent { nsc } => qualifier.single_answers.get(&nsc),
        CancerNciInterventionIdentity::Combination { nsc_1, nsc_2 } => {
            qualifier.combination_answers.get(&(nsc_1, nsc_2))
        }
    }
    .ok_or(CancerNci60QualificationError::MissingAnswer)?;
    if candidate.challenge_document.reference.kind != CancerResearchEvidenceKind::ResponseChallenge
        || candidate.challenge_document.reference.source_id != prediction.challenge_source_id()
        || candidate.challenge_document.reference.content_hash
            != Digest::sha256(candidate.challenge_document.content.as_bytes())
    {
        return Err(CancerNci60QualificationError::InvalidCandidate);
    }
    let persisted_challenge: serde_json::Value =
        serde_json::from_str(&candidate.challenge_document.content)?;
    if persisted_challenge != answer.expected_prompt_challenge {
        return Err(CancerNci60QualificationError::InvalidCandidate);
    }
    let challenge_document = candidate
        .contribution
        .nci60_response_prediction
        .as_ref()
        .expect("validated prediction");
    let answer_key_reference = CancerResearchEvidenceReference {
        kind: CancerResearchEvidenceKind::AssayObservation,
        source_id: match prediction.intervention {
            CancerNciInterventionIdentity::SingleAgent { .. } => {
                format!(
                    "nci-cellminer-2.15-nci60:runtime-isolated-answer-v1:{}",
                    answer.source_candidate_id
                )
            }
            CancerNciInterventionIdentity::Combination { .. } => format!(
                "nci-cellminer-2.15-almanac:runtime-isolated-answer-v1:{}",
                answer.source_candidate_id
            ),
        },
        content_hash: qualifier.answer_key_hash,
    };
    let mut result = CancerNci60ResponseQualification {
        schema_version: CANCER_NCI60_RESPONSE_QUALIFICATION_SCHEMA_VERSION,
        method_version: CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION,
        qualification_id: CancerNci60ResponseQualification::deterministic_id(
            candidate.request_id,
            CANCER_NCI60_RESPONSE_QUALIFICATION_METHOD_VERSION,
        ),
        world_id: candidate.world_id,
        request_id: candidate.request_id,
        artifact_hash: candidate.artifact_hash,
        prediction_hash: Digest::canonical(challenge_document)?,
        challenge_id: challenge_document.challenge_id,
        intervention: challenge_document.intervention,
        observed_response_ranks: answer.observed_ranks.clone(),
        pairwise_comparison_count: 0,
        concordant_pair_count: 0,
        pairwise_concordance_per_mille: None,
        most_responsive_line_correct: None,
        least_responsive_line_correct: None,
        answer_key: answer_key_reference,
        limitations: vec![
            "This is rank agreement in six immortalized two-dimensional CNS cell lines, not patient efficacy."
                .to_owned(),
            "The qualification does not establish safety, dose, exposure, mechanism, animal benefit, or clinical benefit."
                .to_owned(),
            "One public held-out assay profile is a falsification check, not independent biological replication."
                .to_owned(),
            "Observed assay magnitudes remain isolated from model prompts and are not copied into this result."
                .to_owned(),
            "The public NCI measurements may have appeared in model pretraining; runtime isolation does not prove clean out-of-sample generalization."
                .to_owned(),
        ],
    };
    let (comparisons, concordant, per_mille, top_correct, bottom_correct) = rank_concordance(
        &challenge_document.predicted_response_order,
        &result.observed_response_ranks,
    );
    result.pairwise_comparison_count = comparisons;
    result.concordant_pair_count = concordant;
    result.pairwise_concordance_per_mille = per_mille;
    result.most_responsive_line_correct = top_correct;
    result.least_responsive_line_correct = bottom_correct;
    result.validate_against(&candidate.contribution)?;
    Ok(result)
}

fn validate_answer_key(
    answer_key: &AnswerKey,
    catalogue_bytes: &[u8],
    answer_key_bytes: &[u8],
) -> Result<(), CancerNci60QualificationError> {
    let catalogue_hash = Digest::sha256(catalogue_bytes);
    let answer_key_hash = Digest::sha256(answer_key_bytes);
    let answer_payload_commitment = Digest::sha256(&serde_json::to_vec(&ChallengeAnswerPayload {
        domain: CHALLENGE_ANSWER_DOMAIN,
        single_agent_answers: &answer_key.single_agent_answers,
        combination_answers: &answer_key.combination_answers,
    })?);
    if answer_key.schema_version != 1
        || answer_key.answer_key_id != "nci-cellminer-2-15-cns-challenge-answer-key-v1"
        || answer_key.evidence_class
            != "qualification_only_in_vitro_immortalized_cell_line_response"
        || answer_key.intended_use.trim().is_empty()
        || answer_key.source_registry_hash == Digest::ZERO
        || !answer_key.source.is_object()
        || answer_key.cns_cell_lines.len() != 6
        || answer_key.ranking_rule.trim().is_empty()
        || answer_key.single_agent_response_measure.trim().is_empty()
        || answer_key.combination_response_measure.trim().is_empty()
        || answer_key.catalogue_reference.catalogue_id != EXPECTED_CATALOGUE_ID
        || catalogue_hash.to_string() != EXPECTED_CATALOGUE_SHA256
        || answer_key_hash.to_string() != EXPECTED_ANSWER_KEY_SHA256
        || answer_key.catalogue_reference.catalogue_artifact_sha256 != catalogue_hash
        || answer_key.catalogue_reference.answer_payload_commitment != answer_payload_commitment
        || answer_key.leakage_boundary.access_class != "qualification_worker_only"
        || answer_key.leakage_boundary.allowed_in_model_context
        || !answer_key
            .leakage_boundary
            .contains_observed_response_values
        || !answer_key.leakage_boundary.contains_derived_rank_labels
        || answer_key.single_agent_answers.is_empty()
        || answer_key.combination_answers.is_empty()
        || answer_key.limitations.len() < 3
    {
        return Err(CancerNci60QualificationError::InvalidAnswerKey);
    }
    Ok(())
}

fn validate_single_observations(
    observations: &[SingleObservation],
) -> Result<(), CancerNci60QualificationError> {
    if observations.len() != 6
        || observations.windows(2).any(|pair| {
            pair[0].activity_z_milli < pair[1].activity_z_milli
                || (pair[0].activity_z_milli == pair[1].activity_z_milli
                    && pair[0].cell_line >= pair[1].cell_line)
        })
        || observations.iter().enumerate().any(|(index, observation)| {
            observation.descending_response_rank == 0
                || usize::from(observation.descending_response_rank) > index + 1
        })
    {
        return Err(CancerNci60QualificationError::InvalidAnswerKey);
    }
    Ok(())
}

fn validate_combination_observations(
    observations: &[CombinationObservation],
) -> Result<(), CancerNci60QualificationError> {
    if observations.len() != 6
        || observations.windows(2).any(|pair| {
            pair[0].combo_score_milli < pair[1].combo_score_milli
                || (pair[0].combo_score_milli == pair[1].combo_score_milli
                    && pair[0].cell_line >= pair[1].cell_line)
        })
        || observations.iter().enumerate().any(|(index, observation)| {
            let direction_matches = matches!(
                (
                    observation.combo_score_milli.cmp(&0),
                    observation.interaction_direction
                ),
                (std::cmp::Ordering::Less, InteractionDirection::Negative)
                    | (std::cmp::Ordering::Equal, InteractionDirection::Zero)
                    | (std::cmp::Ordering::Greater, InteractionDirection::Positive)
            );
            observation.descending_interaction_rank == 0
                || usize::from(observation.descending_interaction_rank) > index + 1
                || !direction_matches
        })
    {
        return Err(CancerNci60QualificationError::InvalidAnswerKey);
    }
    Ok(())
}

fn parse_line(value: &str) -> Result<CancerNci60CnsLine, CancerNci60QualificationError> {
    match value {
        "CNS:SF-268" => Ok(CancerNci60CnsLine::Sf268),
        "CNS:SF-295" => Ok(CancerNci60CnsLine::Sf295),
        "CNS:SF-539" => Ok(CancerNci60CnsLine::Sf539),
        "CNS:SNB-19" => Ok(CancerNci60CnsLine::Snb19),
        "CNS:SNB-75" => Ok(CancerNci60CnsLine::Snb75),
        "CNS:U251" => Ok(CancerNci60CnsLine::U251),
        _ => Err(CancerNci60QualificationError::InvalidAnswerKey),
    }
}

fn rank_concordance(
    predicted: &[CancerNci60CnsLine],
    observed: &[CancerNci60ObservedRank],
) -> (u8, u8, Option<u16>, Option<bool>, Option<bool>) {
    let positions = predicted
        .iter()
        .enumerate()
        .map(|(index, line)| (*line, index))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut comparisons = 0_u8;
    let mut concordant = 0_u8;
    for left in 0..observed.len() {
        for right in (left + 1)..observed.len() {
            if observed[left].rank == observed[right].rank {
                continue;
            }
            comparisons += 1;
            if positions[&observed[left].cell_line] < positions[&observed[right].cell_line] {
                concordant += 1;
            }
        }
    }
    let per_mille = (comparisons > 0)
        .then(|| u16::try_from(u32::from(concordant) * 1_000 / u32::from(comparisons)).ok())
        .flatten();
    let top_rank = observed.first().map(|observation| observation.rank);
    let bottom_rank = observed.last().map(|observation| observation.rank);
    let informative = top_rank != bottom_rank;
    let top_correct = informative.then(|| {
        predicted.first().is_some_and(|line| {
            observed.iter().any(|observation| {
                Some(observation.rank) == top_rank && observation.cell_line == *line
            })
        })
    });
    let bottom_correct = informative.then(|| {
        predicted.last().is_some_and(|line| {
            observed.iter().any(|observation| {
                Some(observation.rank) == bottom_rank && observation.cell_line == *line
            })
        })
    });
    (
        comparisons,
        concordant,
        per_mille,
        top_correct,
        bottom_correct,
    )
}

#[derive(Debug, Error)]
pub enum CancerNci60QualificationError {
    #[error(transparent)]
    Contract(#[from] CancerResearchContractError),
    #[error(transparent)]
    Decode(#[from] serde_json::Error),
    #[error(transparent)]
    Hash(#[from] world_domain::CanonicalHashError),
    #[error("NCI-60 qualification candidate crossed its immutable provenance")]
    InvalidCandidate,
    #[error("NCI-60 qualification answer key failed its isolation or provenance checks")]
    InvalidAnswerKey,
    #[error("NCI-60 qualification answer key has no exact intervention match")]
    MissingAnswer,
}

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{
        CANCER_NCI60_RESPONSE_PREDICTION_SCHEMA_VERSION,
        CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION, CancerNci60ResponsePrediction,
        CancerResearchArtifactKind, CancerResearchClaim, CancerResearchContribution,
        CancerResearchStage, EntityId, WorldId,
    };

    #[test]
    #[ignore = "requires the locally derived isolated NCI-60 answer key"]
    fn real_isolated_key_opens_one_catalogue_bound_prediction_deterministically() {
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalogue = std::fs::read(
            repository
                .join("data/cancer-research/nci-cellminer-2-15-cns-challenge-catalogue-v1.json"),
        )
        .expect("catalogue");
        let answer_key = std::fs::read(repository.join(
            "data/source-cache/nci-cellminer-2026-08-12/nci-cellminer-2-15-cns-challenge-answer-key-v1.json",
        ))
        .expect("answer key");
        let decoded: AnswerKey = serde_json::from_slice(&answer_key).expect("answer key");
        let answer = decoded
            .single_agent_answers
            .first()
            .expect("single-agent answer");
        let world_id = WorldId::from_uuid(uuid::Uuid::from_u128(0x6060));
        let request_id = uuid::Uuid::from_u128(0x7070);
        let prediction = CancerNci60ResponsePrediction {
            schema_version: CANCER_NCI60_RESPONSE_PREDICTION_SCHEMA_VERSION,
            challenge_id: uuid::Uuid::from_u128(0x8080),
            intervention: CancerNciInterventionIdentity::SingleAgent { nsc: answer.nsc },
            predicted_response_order: answer
                .observations
                .iter()
                .map(|observation| parse_line(&observation.cell_line).expect("line"))
                .collect(),
        };
        let contribution = CancerResearchContribution {
            schema_version: CANCER_RESEARCH_CONTRIBUTION_SCHEMA_VERSION,
            contribution_id: uuid::Uuid::from_u128(0x9090),
            request_id,
            selection_hash: Digest::sha256(b"selection"),
            resident_id: EntityId::deterministic(world_id, b"qualification-test"),
            stage: CancerResearchStage::BlindDiscovery,
            artifact_kind: CancerResearchArtifactKind::Hypothesis,
            title: "Frozen NCI-60 rank".to_owned(),
            abstract_text: "One concrete compound response order was frozen before labels opened."
                .to_owned(),
            claims: vec![CancerResearchClaim {
                statement: "The six lines may differ in relative sensitivity.".to_owned(),
                testable_prediction: "The frozen ordering agrees with held-out ranks.".to_owned(),
                falsification_test: "The held-out ordering disagrees.".to_owned(),
                citation_hashes: Vec::new(),
            }],
            molecular_targets: Vec::new(),
            virtual_experiment_plan: None,
            nci60_response_prediction: Some(prediction),
        };
        let artifact_hash = contribution.canonical_hash().expect("artifact hash");
        let catalogue_value: serde_json::Value =
            serde_json::from_slice(&catalogue).expect("catalogue JSON");
        let expected_challenge = prepare_prompt_challenges(&catalogue_value)
            .expect("prompt challenges")
            .0
            .get(&answer.nsc)
            .expect("single challenge")
            .1
            .clone();
        let challenge_content =
            serde_json::to_string(&expected_challenge).expect("challenge content");
        let challenge_document = crate::CancerResearchEvidenceDocument {
            reference: CancerResearchEvidenceReference {
                kind: CancerResearchEvidenceKind::ResponseChallenge,
                source_id: contribution
                    .nci60_response_prediction
                    .as_ref()
                    .expect("prediction")
                    .challenge_source_id(),
                content_hash: Digest::sha256(challenge_content.as_bytes()),
            },
            content: challenge_content,
        };
        let candidate = CancerNci60QualificationCandidate {
            world_id,
            request_id,
            ordinal: 1,
            artifact_hash,
            contribution,
            challenge_document,
        };
        let first = qualify_cancer_nci60_prediction(&candidate, &catalogue, &answer_key)
            .expect("qualification");
        let repeated = qualify_cancer_nci60_prediction(&candidate, &catalogue, &answer_key)
            .expect("repeated qualification");
        assert_eq!(first, repeated);
        assert_eq!(first.pairwise_comparison_count, 15);
        assert_eq!(first.concordant_pair_count, 15);
        assert_eq!(first.pairwise_concordance_per_mille, Some(1_000));

        let mut altered_catalogue = catalogue;
        altered_catalogue.push(b'\n');
        assert!(matches!(
            qualify_cancer_nci60_prediction(&candidate, &altered_catalogue, &answer_key),
            Err(CancerNci60QualificationError::InvalidAnswerKey)
        ));
    }
}
