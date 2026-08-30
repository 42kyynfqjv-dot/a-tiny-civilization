use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use world_domain::{
    CANCER_RESEARCH_NOVELTY_AUDIT_SCHEMA_VERSION, CANCER_RESEARCH_NOVELTY_METHOD_VERSION,
    CancerResearchContractError, CancerResearchContribution, CancerResearchNoveltyAudit,
    CancerResearchNoveltyMatch, CancerResearchNoveltyStatus, Digest,
    MAX_CANCER_RESEARCH_NOVELTY_MATCHES, MAX_CANCER_RESEARCH_NOVELTY_QUERY_TERMS, WorldId,
};

/// A successful artifact awaiting observer-side overlap analysis. Prior artifacts
/// are bounded by the store and never include external literature in blind-world
/// prompts; this type belongs only to the read-side evidence worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancerResearchNoveltyCandidate {
    pub world_id: WorldId,
    pub request_id: uuid::Uuid,
    pub ordinal: u32,
    pub artifact_hash: Digest,
    pub contribution: CancerResearchContribution,
    pub prior_contributions: Vec<CancerResearchContribution>,
}

/// Transient metadata used to calculate an overlap score. Abstract text is
/// intentionally never part of the durable audit, avoiding a second copy of
/// externally copyrighted text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CancerResearchNoveltySource {
    pub source_id: String,
    pub title: String,
    pub published_on: Option<String>,
    pub abstract_text: String,
}

#[must_use]
pub fn cancer_research_novelty_query_terms(
    contribution: &CancerResearchContribution,
) -> Vec<String> {
    let mut weights = BTreeMap::<String, u16>::new();
    add_weighted_terms(&mut weights, &contribution.title, 5);
    add_weighted_terms(&mut weights, &contribution.abstract_text, 2);
    for claim in &contribution.claims {
        add_weighted_terms(&mut weights, &claim.statement, 3);
        add_weighted_terms(&mut weights, &claim.testable_prediction, 1);
    }
    let mut ranked: Vec<_> = weights.into_iter().collect();
    ranked.sort_by(|(left_term, left_weight), (right_term, right_weight)| {
        right_weight
            .cmp(left_weight)
            .then_with(|| {
                scientific_specificity(right_term).cmp(&scientific_specificity(left_term))
            })
            .then_with(|| left_term.cmp(right_term))
    });
    let mut selected: Vec<String> = ranked
        .into_iter()
        .map(|(term, _)| term)
        .take(MAX_CANCER_RESEARCH_NOVELTY_QUERY_TERMS)
        .collect();
    if selected.is_empty() {
        selected.push("glioblastoma".to_owned());
    }
    selected.sort();
    selected
}

pub fn calculate_cancer_research_novelty(
    candidate: &CancerResearchNoveltyCandidate,
    sources: &[CancerResearchNoveltySource],
) -> Result<CancerResearchNoveltyAudit, CancerResearchContractError> {
    if candidate.request_id != candidate.contribution.request_id
        || candidate.artifact_hash != candidate.contribution.canonical_hash()?
    {
        return Err(CancerResearchContractError::InvalidNoveltyAudit);
    }
    let query_terms = cancer_research_novelty_query_terms(&candidate.contribution);
    let artifact_terms = significant_terms(&contribution_text(&candidate.contribution), 32);
    let mut scored_sources: Vec<(CancerResearchNoveltyMatch, BTreeSet<String>)> = sources
        .iter()
        .filter_map(|source| {
            let source_terms =
                significant_terms(&format!("{} {}", source.title, source.abstract_text), 256);
            let (score, intersection) = overlap_score(&artifact_terms, &source_terms);
            if intersection < 2 || score == 0 {
                return None;
            }
            let novelty_match = CancerResearchNoveltyMatch {
                source_id: source.source_id.trim().to_owned(),
                title: source.title.trim().to_owned(),
                published_on: source.published_on.clone(),
                overlap_per_mille: score,
            };
            novelty_match
                .validate()
                .ok()
                .map(|()| (novelty_match, source_terms))
        })
        .collect();
    // External indexes can return the same source more than once with different
    // metadata. Group by identity before ranking: ranking first only made equal
    // IDs adjacent when their scores also happened to be equal, allowing one
    // malformed response to poison every subsequent audit pass.
    scored_sources.sort_by(|(left, left_terms), (right, right_terms)| {
        left.source_id
            .cmp(&right.source_id)
            .then_with(|| right.overlap_per_mille.cmp(&left.overlap_per_mille))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.published_on.cmp(&right.published_on))
            .then_with(|| left_terms.cmp(right_terms))
    });
    scored_sources.dedup_by(|(left, _), (right, _)| left.source_id == right.source_id);
    scored_sources.sort_by(|(left, _), (right, _)| {
        right
            .overlap_per_mille
            .cmp(&left.overlap_per_mille)
            .then_with(|| left.source_id.cmp(&right.source_id))
    });
    scored_sources.truncate(MAX_CANCER_RESEARCH_NOVELTY_MATCHES);

    let literature_overlap_per_mille = scored_sources
        .first()
        .map_or(0, |(source, _)| source.overlap_per_mille);
    let prior_world_overlap_per_mille = candidate
        .prior_contributions
        .iter()
        .map(|prior| {
            let prior_terms = significant_terms(&contribution_text(prior), 64);
            overlap_score(&artifact_terms, &prior_terms).0
        })
        .max()
        .unwrap_or(0);
    let warnings = scientific_sanity_warnings(&candidate.contribution);
    let covered_terms: BTreeSet<_> = scored_sources
        .iter()
        .flat_map(|(_, terms)| artifact_terms.intersection(terms).cloned())
        .collect();
    let collective_coverage = ratio_per_mille(covered_terms.len(), artifact_terms.len());
    let contributing_sources = scored_sources
        .iter()
        .filter(|(source, _)| source.overlap_per_mille >= 160)
        .count();
    let status = if !warnings.is_empty() {
        CancerResearchNoveltyStatus::PossibleError
    } else if literature_overlap_per_mille >= 520 || prior_world_overlap_per_mille >= 820 {
        CancerResearchNoveltyStatus::KnownOverlap
    } else if contributing_sources >= 2 && collective_coverage >= 620 {
        CancerResearchNoveltyStatus::NewCombination
    } else {
        CancerResearchNoveltyStatus::NoCloseMatchFound
    };
    let audit = CancerResearchNoveltyAudit {
        schema_version: CANCER_RESEARCH_NOVELTY_AUDIT_SCHEMA_VERSION,
        method_version: CANCER_RESEARCH_NOVELTY_METHOD_VERSION,
        audit_id: CancerResearchNoveltyAudit::deterministic_id(
            candidate.request_id,
            CANCER_RESEARCH_NOVELTY_METHOD_VERSION,
        ),
        world_id: candidate.world_id,
        request_id: candidate.request_id,
        artifact_hash: candidate.artifact_hash,
        query_terms,
        status,
        literature_overlap_per_mille,
        prior_world_overlap_per_mille,
        matches: scored_sources
            .into_iter()
            .map(|(source, _)| source)
            .collect(),
        warnings,
    };
    audit.validate()?;
    Ok(audit)
}

fn contribution_text(contribution: &CancerResearchContribution) -> String {
    let mut text = format!("{} {}", contribution.title, contribution.abstract_text);
    for claim in &contribution.claims {
        text.push(' ');
        text.push_str(&claim.statement);
        text.push(' ');
        text.push_str(&claim.testable_prediction);
    }
    text
}

fn add_weighted_terms(weights: &mut BTreeMap<String, u16>, text: &str, weight: u16) {
    for term in tokenize(text) {
        *weights.entry(term).or_default() = weights
            .get(&term)
            .copied()
            .unwrap_or_default()
            .saturating_add(weight);
    }
}

fn significant_terms(text: &str, limit: usize) -> BTreeSet<String> {
    tokenize(text).into_iter().take(limit).collect()
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter_map(|raw| {
            let term = normalize_term(raw);
            (!term.is_empty() && term.len() <= 64 && !STOP_TERMS.contains(&term.as_str()))
                .then_some(term)
        })
        .collect()
}

fn normalize_term(raw: &str) -> String {
    let mut term = raw.to_owned();
    for suffix in [
        "ization", "ations", "ation", "ingly", "ments", "ment", "ing", "ies", "ed", "es", "s",
    ] {
        if term.len() > suffix.len() + 4 && term.ends_with(suffix) {
            term.truncate(term.len() - suffix.len());
            break;
        }
    }
    term
}

fn overlap_score(left: &BTreeSet<String>, right: &BTreeSet<String>) -> (u16, usize) {
    if left.is_empty() || right.is_empty() {
        return (0, 0);
    }
    let intersection = left.intersection(right).count();
    let union = left.union(right).count();
    let coverage = ratio_per_mille(intersection, left.len());
    let jaccard = ratio_per_mille(intersection, union);
    let score = (u32::from(coverage) * 85 + u32::from(jaccard) * 15) / 100;
    (
        u16::try_from(score).unwrap_or(1_000).min(1_000),
        intersection,
    )
}

fn ratio_per_mille(numerator: usize, denominator: usize) -> u16 {
    if denominator == 0 {
        return 0;
    }
    u16::try_from(numerator.saturating_mul(1_000) / denominator)
        .unwrap_or(1_000)
        .min(1_000)
}

fn scientific_specificity(term: &str) -> u8 {
    u8::from(term.chars().any(|character| character.is_ascii_digit())) * 2
        + u8::from(term.len() >= 8)
}

fn scientific_sanity_warnings(contribution: &CancerResearchContribution) -> Vec<String> {
    let text = contribution_text(contribution).to_ascii_lowercase();
    let mct4_uptake_claim = text.contains("mct4")
        && text.contains("lactate")
        && (text.contains("uptake") || text.contains("import"))
        && !text.contains("mct1");
    if mct4_uptake_claim {
        return vec![
            "Possible transporter-direction mismatch: MCT4 is usually discussed as a lactate-export route; check whether MCT1 or another importer was intended."
                .to_owned(),
        ];
    }
    Vec::new()
}

const STOP_TERMS: &[&str] = &[
    "a",
    "about",
    "adult",
    "after",
    "against",
    "all",
    "also",
    "an",
    "and",
    "are",
    "as",
    "at",
    "be",
    "because",
    "been",
    "before",
    "between",
    "by",
    "can",
    "cancer",
    "cell",
    "cells",
    "could",
    "data",
    "during",
    "effect",
    "for",
    "from",
    "glioblastoma",
    "has",
    "have",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "may",
    "model",
    "more",
    "not",
    "of",
    "on",
    "or",
    "our",
    "result",
    "should",
    "study",
    "such",
    "than",
    "that",
    "the",
    "their",
    "then",
    "these",
    "this",
    "those",
    "to",
    "tumor",
    "using",
    "was",
    "we",
    "were",
    "when",
    "which",
    "with",
    "would",
];

#[cfg(test)]
mod tests {
    use super::*;
    use world_domain::{
        CancerResearchArtifactKind, CancerResearchClaim, CancerResearchInferenceTier,
        CancerResearchProfile, CancerResearchStage, CancerResearchTarget, CancerResearchTask,
        CancerResearchTurnSelection, EntityId, SimTick, WorldSeed,
    };

    fn contribution(title: &str, abstract_text: &str) -> (WorldId, CancerResearchContribution) {
        let world_id = WorldId::from_uuid(uuid::Uuid::from_u128(900));
        let resident_id = EntityId::deterministic(world_id, b"novelty-test");
        let selection = CancerResearchTurnSelection::new(
            world_id,
            resident_id,
            SimTick::new(1),
            SimTick::new(2),
            1,
            CancerResearchTarget::AdultGlioblastoma,
            CancerResearchStage::BlindDiscovery,
            CancerResearchTask::GenerateMechanisticHypothesis,
            CancerResearchInferenceTier::Exploration,
            CancerResearchProfile::seeded(WorldSeed::new(1), resident_id).expect("profile"),
            Vec::new(),
            None,
            512,
        )
        .expect("selection");
        let contribution = CancerResearchContribution::new(
            &selection,
            CancerResearchArtifactKind::Hypothesis,
            title,
            abstract_text,
            vec![CancerResearchClaim {
                statement: abstract_text.to_owned(),
                testable_prediction: "The proposed mechanism produces a measurable response."
                    .to_owned(),
                falsification_test: "The response is absent in a controlled comparison.".to_owned(),
                citation_hashes: Vec::new(),
            }],
        )
        .expect("contribution");
        (world_id, contribution)
    }

    fn candidate(
        world_id: WorldId,
        contribution: CancerResearchContribution,
    ) -> CancerResearchNoveltyCandidate {
        CancerResearchNoveltyCandidate {
            world_id,
            request_id: contribution.request_id,
            ordinal: 1,
            artifact_hash: contribution.canonical_hash().expect("hash"),
            contribution,
            prior_contributions: Vec::new(),
        }
    }

    #[test]
    fn close_literature_is_known_overlap() {
        let (world_id, contribution) = contribution(
            "P2X7 extracellular ATP signaling drives glioblastoma invasion",
            "Extracellular ATP activates P2X7 purinergic signaling and promotes invasive migration through calcium signaling.",
        );
        let audit = calculate_cancer_research_novelty(
            &candidate(world_id, contribution),
            &[CancerResearchNoveltySource {
                source_id: "https://europepmc.org/article/MED/1".to_owned(),
                title: "P2X7 purinergic receptor signaling in glioblastoma invasion".to_owned(),
                published_on: Some("2025-01-01".to_owned()),
                abstract_text: "Extracellular ATP activates P2X7 signaling and calcium dependent invasive migration in glioblastoma cells.".to_owned(),
            }],
        )
        .expect("audit");
        assert_eq!(audit.status, CancerResearchNoveltyStatus::KnownOverlap);
    }

    #[test]
    fn mct4_import_direction_is_flagged_for_review() {
        let (world_id, contribution) = contribution(
            "MCT4-high clones import lactate",
            "MCT4-high clones increase lactate uptake to sustain oxidative metabolism.",
        );
        let audit = calculate_cancer_research_novelty(&candidate(world_id, contribution), &[])
            .expect("audit");
        assert_eq!(audit.status, CancerResearchNoveltyStatus::PossibleError);
        assert_eq!(audit.warnings.len(), 1);
    }

    #[test]
    fn unmatched_work_is_not_overclaimed_as_proven_novelty() {
        let (world_id, contribution) = contribution(
            "Phase coupled acoustic lattice perturbation",
            "A phase coupled acoustic lattice perturbs spatial clone boundaries under intermittent pressure gradients.",
        );
        let audit = calculate_cancer_research_novelty(&candidate(world_id, contribution), &[])
            .expect("audit");
        assert_eq!(audit.status, CancerResearchNoveltyStatus::NoCloseMatchFound);
    }

    #[test]
    fn duplicate_external_source_ids_cannot_poison_an_audit_batch() {
        let (world_id, contribution) = contribution(
            "P2X7 extracellular ATP signaling drives glioblastoma invasion",
            "Extracellular ATP activates P2X7 purinergic signaling and promotes invasive migration through calcium signaling.",
        );
        let sources = [
            CancerResearchNoveltySource {
                source_id: "https://europepmc.org/article/MED/duplicate".to_owned(),
                title: "P2X7 purinergic receptor signaling in glioblastoma invasion".to_owned(),
                published_on: Some("2025-01-01".to_owned()),
                abstract_text: "Extracellular ATP activates P2X7 signaling and calcium dependent invasive migration.".to_owned(),
            },
            CancerResearchNoveltySource {
                source_id: "https://europepmc.org/article/MED/distinct".to_owned(),
                title: "ATP signaling in invasive brain tumors".to_owned(),
                published_on: Some("2024-01-01".to_owned()),
                abstract_text: "Purinergic signaling supports invasive migration.".to_owned(),
            },
            CancerResearchNoveltySource {
                source_id: "https://europepmc.org/article/MED/duplicate".to_owned(),
                title: "ATP receptor study".to_owned(),
                published_on: Some("2025-01-01".to_owned()),
                abstract_text: "ATP signaling in cancer.".to_owned(),
            },
        ];
        let audit = calculate_cancer_research_novelty(&candidate(world_id, contribution), &sources)
            .expect("duplicate external rows are normalized");
        assert_eq!(
            audit
                .matches
                .iter()
                .filter(|source| source.source_id.ends_with("/duplicate"))
                .count(),
            1
        );
    }

    #[test]
    fn malformed_external_metadata_is_ignored_instead_of_stalling_the_worker() {
        let (world_id, contribution) = contribution(
            "P2X7 extracellular ATP signaling drives glioblastoma invasion",
            "Extracellular ATP activates P2X7 purinergic signaling and promotes invasive migration through calcium signaling.",
        );
        let audit = calculate_cancer_research_novelty(
            &candidate(world_id, contribution),
            &[CancerResearchNoveltySource {
                source_id: "https://europepmc.org/article/MED/oversized".to_owned(),
                title: "x".repeat(257),
                published_on: Some("not-a-date".to_owned()),
                abstract_text: "Extracellular ATP activates P2X7 signaling and calcium dependent invasive migration.".to_owned(),
            }],
        )
        .expect("malformed external metadata is not a batch poison pill");
        assert!(audit.matches.is_empty());
    }

    #[test]
    fn duplicate_selection_is_independent_of_external_response_order() {
        let (world_id, contribution) = contribution(
            "P2X7 extracellular ATP signaling drives glioblastoma invasion",
            "Extracellular ATP activates P2X7 purinergic signaling and promotes invasive migration through calcium signaling.",
        );
        let earlier = CancerResearchNoveltySource {
            source_id: "https://europepmc.org/article/MED/permutation".to_owned(),
            title: "P2X7 purinergic receptor signaling in glioblastoma invasion".to_owned(),
            published_on: Some("2024-01-01".to_owned()),
            abstract_text:
                "Extracellular ATP activates P2X7 signaling and calcium dependent invasive migration."
                    .to_owned(),
        };
        let later = CancerResearchNoveltySource {
            published_on: Some("2025-01-01".to_owned()),
            ..earlier.clone()
        };
        let forward = calculate_cancer_research_novelty(
            &candidate(world_id, contribution.clone()),
            &[later.clone(), earlier.clone()],
        )
        .expect("forward audit");
        let reverse = calculate_cancer_research_novelty(
            &candidate(world_id, contribution),
            &[earlier, later],
        )
        .expect("reverse audit");
        assert_eq!(forward, reverse);
        assert_eq!(forward.method_version, 2);
        assert_eq!(
            forward.matches[0].published_on.as_deref(),
            Some("2024-01-01")
        );
    }

    #[test]
    fn historical_method_one_audits_remain_valid() {
        let (world_id, contribution) = contribution(
            "Phase coupled acoustic lattice perturbation",
            "A phase coupled acoustic lattice perturbs spatial clone boundaries under intermittent pressure gradients.",
        );
        let mut historical =
            calculate_cancer_research_novelty(&candidate(world_id, contribution), &[])
                .expect("current audit");
        assert_eq!(historical.method_version, 2);
        historical.method_version = 1;
        historical.audit_id =
            CancerResearchNoveltyAudit::deterministic_id(historical.request_id, 1);
        historical.validate().expect("historical method one audit");
    }

    #[test]
    fn oversized_model_tokens_cannot_poison_a_novelty_audit() {
        let (world_id, contribution) = contribution(
            &"x".repeat(65),
            "The bounded comparison contains ordinary searchable mechanism terms.",
        );
        let audit = calculate_cancer_research_novelty(&candidate(world_id, contribution), &[])
            .expect("oversized terms are excluded");
        assert!(audit.query_terms.iter().all(|term| term.len() <= 64));
    }
}
