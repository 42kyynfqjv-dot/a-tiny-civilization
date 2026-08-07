use std::collections::BTreeMap;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sim_engine::{
    EngineState, InitialOrganism, RULESET_VERSION, Snapshot, replay, replay_from_snapshot,
};
use world_domain::{
    BirthCategory, DeathCause, Digest, EntityId, EventBatch, EventSequence, OrganismRole, SimTick,
    SpeciesIdentity, WorldId, WorldManifest, WorldSeed, WorldStatus,
};

pub const VERIFICATION_BUNDLE_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ExpectedOutcome {
    pub through_sequence: EventSequence,
    pub tick: SimTick,
    pub status: WorldStatus,
    pub last_event_hash: Digest,
    pub state_hash: Digest,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VerificationBundle {
    pub bundle_schema_version: u16,
    pub description: String,
    pub manifest: WorldManifest,
    pub event_batches: Vec<EventBatch>,
    pub snapshot: Snapshot,
    pub expected: ExpectedOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VerificationReport {
    pub world_id: WorldId,
    pub event_batches: usize,
    pub through_sequence: EventSequence,
    pub tick: SimTick,
    pub status: WorldStatus,
    pub last_event_hash: Digest,
    pub state_hash: Digest,
    pub genesis_replay_matches_snapshot_tail: bool,
}

impl VerificationBundle {
    pub fn deterministic_demo() -> Result<Self> {
        let world_id = WorldId::from_uuid(uuid::Uuid::from_u128(
            0x019f_d4a9_b7f9_7891_ab51_cdf7_1d2b_7688,
        ));
        let mut manifest = WorldManifest::new(
            world_id,
            WorldSeed::new(7640891576956012809),
            RULESET_VERSION,
        );
        manifest.scientific_datasets = BTreeMap::from([(
            "gbif_taxon_2436436".to_owned(),
            "accessed_2026-08-06".to_owned(),
        )]);

        let person_id = EntityId::deterministic(world_id, b"verification-person-1");
        let initial = EngineState::new(manifest.clone());
        let genesis_events = initial.plan_genesis(vec![InitialOrganism {
            organism_id: person_id,
            species: SpeciesIdentity::new(
                "gbif",
                "2436436",
                "Homo sapiens",
                "https://www.gbif.org/species/2436436",
            )?,
            role: OrganismRole::Person,
            birth_category: BirthCategory::new("female")?,
            initial_age_ticks: 0,
            location_id: None,
            embodied_patch: None,
            metabolic_rate: None,
            physiological_regulation: None,
            reproductive_physiology: None,
            heritable_disposition_profile: None,
        }])?;
        let (running, genesis_batch) =
            initial.commit(EventSequence::new(1), Digest::ZERO, genesis_events)?;

        let tick_events = running.plan_next_tick()?;
        let (after_tick, tick_batch) =
            running.commit(EventSequence::new(2), genesis_batch.batch_hash, tick_events)?;
        let snapshot = Snapshot::new(
            after_tick.clone(),
            tick_batch.sequence,
            tick_batch.batch_hash,
        )?;

        let death_events = after_tick.plan_death(
            person_id,
            DeathCause {
                mechanism: "verification_fixture".to_owned(),
            },
        )?;
        let (archived, death_batch) =
            after_tick.commit(EventSequence::new(3), tick_batch.batch_hash, death_events)?;
        let state_hash = archived.state_hash()?;

        Ok(Self {
            bundle_schema_version: VERIFICATION_BUNDLE_SCHEMA_VERSION,
            description: "Non-production deterministic proof using the real GBIF Homo sapiens taxon; it is not a public-world seed or biome.".to_owned(),
            manifest,
            event_batches: vec![genesis_batch, tick_batch, death_batch.clone()],
            snapshot,
            expected: ExpectedOutcome {
                through_sequence: death_batch.sequence,
                tick: archived.tick(),
                status: archived.status(),
                last_event_hash: death_batch.batch_hash,
                state_hash,
            },
        })
    }

    pub fn verify(&self) -> Result<VerificationReport> {
        ensure!(
            self.bundle_schema_version == VERIFICATION_BUNDLE_SCHEMA_VERSION,
            "unsupported verification bundle schema {}",
            self.bundle_schema_version
        );
        ensure!(
            !self.event_batches.is_empty(),
            "bundle has no event batches"
        );
        self.snapshot
            .verify_integrity()
            .context("verify embedded snapshot")?;

        let complete = replay(self.manifest.clone(), &self.event_batches)
            .context("replay complete history from genesis")?;
        let prefix_len = self
            .event_batches
            .iter()
            .position(|batch| batch.sequence > self.snapshot.through_sequence)
            .unwrap_or(self.event_batches.len());
        let prefix = replay(self.manifest.clone(), &self.event_batches[..prefix_len])
            .context("replay history through snapshot sequence")?;
        ensure!(
            prefix.through_sequence == self.snapshot.through_sequence
                && prefix.last_event_hash == self.snapshot.last_event_hash
                && prefix.state == self.snapshot.state,
            "snapshot does not equal replayed prefix"
        );

        let from_snapshot = replay_from_snapshot(&self.snapshot, &self.event_batches[prefix_len..])
            .context("replay snapshot tail")?;
        ensure!(
            from_snapshot == complete,
            "snapshot-plus-tail differs from genesis replay"
        );
        let state_hash = complete.state.state_hash()?;
        ensure!(
            complete.through_sequence == self.expected.through_sequence
                && complete.state.tick() == self.expected.tick
                && complete.state.status() == self.expected.status
                && complete.last_event_hash == self.expected.last_event_hash
                && state_hash == self.expected.state_hash,
            "replayed outcome differs from bundle commitment"
        );

        Ok(VerificationReport {
            world_id: self.manifest.world_id,
            event_batches: self.event_batches.len(),
            through_sequence: complete.through_sequence,
            tick: complete.state.tick(),
            status: complete.state.status(),
            last_event_hash: complete.last_event_hash,
            state_hash,
            genesis_replay_matches_snapshot_tail: true,
        })
    }

    pub fn to_pretty_json(&self) -> Result<String> {
        let mut encoded = serde_json::to_string_pretty(self)?;
        encoded.push('\n');
        Ok(encoded)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).context("decode verification bundle JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_bundle_round_trips_and_verifies_offline() -> Result<()> {
        let bundle = VerificationBundle::deterministic_demo()?;
        let encoded = bundle.to_pretty_json()?;
        let decoded = VerificationBundle::from_json(encoded.as_bytes())?;
        let report = decoded.verify()?;

        assert_eq!(decoded, bundle);
        assert_eq!(report.status, WorldStatus::Archived);
        assert!(report.genesis_replay_matches_snapshot_tail);
        Ok(())
    }

    #[test]
    fn tampering_is_reported() -> Result<()> {
        let mut bundle = VerificationBundle::deterministic_demo()?;
        bundle.event_batches[1].tick = SimTick::new(99);
        assert!(bundle.verify().is_err());
        Ok(())
    }
}
