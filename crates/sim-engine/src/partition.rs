//! Deterministic ordering and barrier semantics for full-Earth partition execution.
//!
//! The engine stores the canonical schedule directly in state and snapshots. Worker
//! count and arrival order remain operational details and cannot affect resolved order.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use thiserror::Error;
use world_domain::{
    Digest, EntityId, MAX_S2_LEVEL, S2CellId, S2CellIdError, SimTick, TimeOverflow,
};

/// Version for the private, strict persisted scheduler checkpoint envelope.
pub(super) const PARTITION_SCHEDULE_SCHEMA_VERSION: u16 = 1;

const CAPACITY_PROBE_MAX_POPULATION: u32 = 1_000_000;
const CAPACITY_PROBE_MAX_TICKS: u32 = 10_000;

/// One execution partition at the configured planetary S2 level.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct PartitionId(S2CellId);

impl PartitionId {
    pub fn new(cell: S2CellId, partition_level: u8) -> Result<Self, SchedulerError> {
        if partition_level > MAX_S2_LEVEL {
            return Err(SchedulerError::InvalidPartitionLevel(partition_level));
        }
        if cell.level() != partition_level {
            return Err(SchedulerError::PartitionLevelMismatch {
                expected: partition_level,
                actual: cell.level(),
            });
        }
        Ok(Self(cell))
    }

    pub fn route(cell: S2CellId, partition_level: u8) -> Result<Self, SchedulerError> {
        Self::new(cell.ancestor(partition_level)?, partition_level)
    }

    #[must_use]
    pub const fn cell(self) -> S2CellId {
        self.0
    }
}

impl Ord for PartitionId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.get().cmp(&other.0.get())
    }
}

impl PartialOrd for PartitionId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Stable, ruleset-owned identity of the state subject receiving work.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct SubjectKey([u8; 16]);

impl SubjectKey {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub fn from_entity(entity_id: EntityId) -> Self {
        Self(*entity_id.as_uuid().as_bytes())
    }

    #[must_use]
    pub const fn into_bytes(self) -> [u8; 16] {
        self.0
    }
}

impl Ord for SubjectKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.as_slice().cmp(other.0.as_slice())
    }
}

impl PartialOrd for SubjectKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Explicit same-tick ordering key.
///
/// Phase and process codes are versioned by the ruleset that uses the kernel. Their
/// numeric values, not enum declaration order or debug text, define canonical order.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct WorkKey {
    phase_code: u16,
    subject: SubjectKey,
    process_code: u16,
    occurrence: u32,
}

impl WorkKey {
    pub fn new(
        phase_code: u16,
        subject: SubjectKey,
        process_code: u16,
        occurrence: u32,
    ) -> Result<Self, SchedulerError> {
        if phase_code == 0 {
            return Err(SchedulerError::ZeroPhaseCode);
        }
        if process_code == 0 {
            return Err(SchedulerError::ZeroProcessCode);
        }
        Ok(Self {
            phase_code,
            subject,
            process_code,
            occurrence,
        })
    }

    fn validate(self) -> Result<(), SchedulerError> {
        if self.phase_code == 0 {
            return Err(SchedulerError::ZeroPhaseCode);
        }
        if self.process_code == 0 {
            return Err(SchedulerError::ZeroProcessCode);
        }
        Ok(())
    }

    #[must_use]
    pub const fn subject(self) -> SubjectKey {
        self.subject
    }
}

impl Ord for WorkKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.phase_code
            .cmp(&other.phase_code)
            .then_with(|| self.subject.cmp(&other.subject))
            .then_with(|| self.process_code.cmp(&other.process_code))
            .then_with(|| self.occurrence.cmp(&other.occurrence))
    }
}

impl PartialOrd for WorkKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// One future causal work item routed to its execution partition.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ScheduledWork {
    due_tick: SimTick,
    partition: PartitionId,
    key: WorkKey,
}

impl ScheduledWork {
    #[must_use]
    pub const fn new(due_tick: SimTick, partition: PartitionId, key: WorkKey) -> Self {
        Self {
            due_tick,
            partition,
            key,
        }
    }

    pub fn routed(
        due_tick: SimTick,
        destination_cell: S2CellId,
        partition_level: u8,
        key: WorkKey,
    ) -> Result<Self, SchedulerError> {
        Ok(Self::new(
            due_tick,
            PartitionId::route(destination_cell, partition_level)?,
            key,
        ))
    }

    #[must_use]
    pub const fn due_tick(self) -> SimTick {
        self.due_tick
    }

    #[must_use]
    pub const fn partition(self) -> PartitionId {
        self.partition
    }

    #[must_use]
    pub const fn key(self) -> WorkKey {
        self.key
    }
}

impl Ord for ScheduledWork {
    fn cmp(&self, other: &Self) -> Ordering {
        self.due_tick
            .cmp(&other.due_tick)
            .then_with(|| self.partition.cmp(&other.partition))
            .then_with(|| self.key.cmp(&other.key))
    }
}

impl PartialOrd for ScheduledWork {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Canonically sorted future work. Planning borrows this value and cannot mutate it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionSchedule {
    partition_level: u8,
    entries: Vec<ScheduledWork>,
}

impl PartitionSchedule {
    pub fn new(
        partition_level: u8,
        mut entries: Vec<ScheduledWork>,
    ) -> Result<Self, SchedulerError> {
        if partition_level > MAX_S2_LEVEL {
            return Err(SchedulerError::InvalidPartitionLevel(partition_level));
        }
        for entry in &entries {
            entry.key.validate()?;
            if entry.partition.cell().level() != partition_level {
                return Err(SchedulerError::PartitionLevelMismatch {
                    expected: partition_level,
                    actual: entry.partition.cell().level(),
                });
            }
        }

        entries.sort_unstable();
        let mut identities = BTreeSet::new();
        for entry in &entries {
            if !identities.insert((entry.due_tick, entry.key)) {
                return Err(SchedulerError::DuplicateWorkKey {
                    due_tick: entry.due_tick,
                });
            }
        }

        Ok(Self {
            partition_level,
            entries,
        })
    }

    #[must_use]
    pub fn entries(&self) -> &[ScheduledWork] {
        &self.entries
    }

    #[must_use]
    pub const fn partition_level(&self) -> u8 {
        self.partition_level
    }

    pub fn plan_next_tick(&self, current_tick: SimTick) -> Result<TickPlan, SchedulerError> {
        let target_tick = current_tick.checked_next()?;
        if let Some(overdue) = self
            .entries
            .iter()
            .find(|entry| entry.due_tick < target_tick)
        {
            return Err(SchedulerError::OverdueWork {
                due_tick: overdue.due_tick,
                target_tick,
            });
        }

        let mut grouped = BTreeMap::<PartitionId, Vec<ScheduledWork>>::new();
        let mut remaining = Vec::new();
        for entry in &self.entries {
            if entry.due_tick == target_tick {
                grouped.entry(entry.partition).or_default().push(*entry);
            } else {
                remaining.push(*entry);
            }
        }
        let partitions = grouped
            .into_iter()
            .map(|(partition, work)| PartitionWork { partition, work })
            .collect();

        Ok(TickPlan {
            from_tick: current_tick,
            to_tick: target_tick,
            partition_level: self.partition_level,
            partitions,
            remaining,
        })
    }
}

#[derive(Deserialize, Serialize)]
struct PartitionScheduleWire {
    partition_level: u8,
    entries: Vec<ScheduledWork>,
}

impl Serialize for PartitionSchedule {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PartitionScheduleWire {
            partition_level: self.partition_level,
            entries: self.entries.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PartitionSchedule {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PartitionScheduleWire::deserialize(deserializer)?;
        Self::new(wire.partition_level, wire.entries).map_err(de::Error::custom)
    }
}

/// A versioned, strict wire envelope for one future-work queue.
///
/// Its digest is suitable for binding a later durable queue to a state hash. The
/// current engine deliberately does not yet store this envelope: no canonical event
/// or snapshot can claim scheduler durability before embodied processes own the work
/// codes and lifecycle rules.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct PartitionScheduleCheckpoint {
    schedule_schema_version: u16,
    schedule: PartitionSchedule,
}

impl PartitionScheduleCheckpoint {
    pub(super) fn new(schedule: PartitionSchedule) -> Self {
        Self {
            schedule_schema_version: PARTITION_SCHEDULE_SCHEMA_VERSION,
            schedule,
        }
    }

    pub(super) fn canonical_bytes(&self) -> Result<Vec<u8>, SchedulerError> {
        serde_json::to_vec(self).map_err(|error| SchedulerError::Encoding(error.to_string()))
    }

    pub(super) fn content_digest(&self) -> Result<Digest, SchedulerError> {
        Ok(Digest::sha256(&self.canonical_bytes()?))
    }

    pub(super) fn from_canonical_slice(bytes: &[u8]) -> Result<Self, SchedulerError> {
        #[derive(Deserialize)]
        struct Wire {
            schedule_schema_version: u16,
            schedule: PartitionSchedule,
        }

        let wire: Wire = serde_json::from_slice(bytes)
            .map_err(|error| SchedulerError::Decode(error.to_string()))?;
        if wire.schedule_schema_version != PARTITION_SCHEDULE_SCHEMA_VERSION {
            return Err(SchedulerError::UnsupportedScheduleSchema(
                wire.schedule_schema_version,
            ));
        }
        let checkpoint = Self::new(wire.schedule);
        if checkpoint.canonical_bytes()? != bytes {
            return Err(SchedulerError::NonCanonicalCheckpointEncoding);
        }
        Ok(checkpoint)
    }

    #[must_use]
    pub(super) const fn schedule(&self) -> &PartitionSchedule {
        &self.schedule
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionWork {
    partition: PartitionId,
    work: Vec<ScheduledWork>,
}

impl PartitionWork {
    #[must_use]
    pub const fn partition(&self) -> PartitionId {
        self.partition
    }

    #[must_use]
    pub fn work(&self) -> &[ScheduledWork] {
        &self.work
    }
}

/// Immutable event proposal emitted by one due work item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Emission<E> {
    destination: PartitionId,
    origin_work: WorkKey,
    emission_index: u32,
    event: E,
}

impl<E> Emission<E> {
    #[must_use]
    pub const fn new(
        destination: PartitionId,
        origin_work: WorkKey,
        emission_index: u32,
        event: E,
    ) -> Self {
        Self {
            destination,
            origin_work,
            emission_index,
            event,
        }
    }

    #[must_use]
    pub const fn destination(&self) -> PartitionId {
        self.destination
    }

    #[must_use]
    pub const fn origin_work(&self) -> WorkKey {
        self.origin_work
    }

    #[must_use]
    pub const fn emission_index(&self) -> u32 {
        self.emission_index
    }

    #[must_use]
    pub const fn event(&self) -> &E {
        &self.event
    }

    fn canonical_cmp(&self, other: &Self) -> Ordering {
        self.destination
            .cmp(&other.destination)
            .then_with(|| self.origin_work.cmp(&other.origin_work))
            .then_with(|| self.emission_index.cmp(&other.emission_index))
    }
}

/// Future work proposed at the barrier. It may not be due in the tick being resolved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeferredWork {
    origin_work: WorkKey,
    emission_index: u32,
    work: ScheduledWork,
}

impl DeferredWork {
    #[must_use]
    pub const fn new(origin_work: WorkKey, emission_index: u32, work: ScheduledWork) -> Self {
        Self {
            origin_work,
            emission_index,
            work,
        }
    }
}

/// One explicit result for one due work item. Empty outcomes are represented by empty
/// vectors rather than by omitting the result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkOutput<E> {
    key: WorkKey,
    emissions: Vec<Emission<E>>,
    deferred_work: Vec<DeferredWork>,
}

impl<E> WorkOutput<E> {
    #[must_use]
    pub fn new(
        key: WorkKey,
        emissions: Vec<Emission<E>>,
        deferred_work: Vec<DeferredWork>,
    ) -> Self {
        Self {
            key,
            emissions,
            deferred_work,
        }
    }
}

/// One worker's immutable result for exactly one active source partition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PartitionOutput<E> {
    source_partition: PartitionId,
    work_outputs: Vec<WorkOutput<E>>,
}

impl<E> PartitionOutput<E> {
    #[must_use]
    pub fn new(source_partition: PartitionId, work_outputs: Vec<WorkOutput<E>>) -> Self {
        Self {
            source_partition,
            work_outputs,
        }
    }
}

/// Immutable start-of-tick plan. Completion creates a new schedule only after every
/// output and per-partition budget validates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TickPlan {
    from_tick: SimTick,
    to_tick: SimTick,
    partition_level: u8,
    partitions: Vec<PartitionWork>,
    remaining: Vec<ScheduledWork>,
}

impl TickPlan {
    #[must_use]
    pub const fn start_tick(&self) -> SimTick {
        self.from_tick
    }

    #[must_use]
    pub const fn to_tick(&self) -> SimTick {
        self.to_tick
    }

    #[must_use]
    pub fn partitions(&self) -> &[PartitionWork] {
        &self.partitions
    }

    pub fn complete<E>(
        self,
        mut outputs: Vec<PartitionOutput<E>>,
        max_events_per_partition: u32,
    ) -> Result<ResolvedTick<E>, SchedulerError> {
        if max_events_per_partition == 0 {
            return Err(SchedulerError::ZeroEventBudget);
        }
        outputs.sort_by_key(|output| output.source_partition);
        if outputs
            .windows(2)
            .any(|pair| pair[0].source_partition == pair[1].source_partition)
        {
            return Err(SchedulerError::DuplicatePartitionOutput);
        }
        if outputs.len() != self.partitions.len()
            || outputs
                .iter()
                .zip(&self.partitions)
                .any(|(output, planned)| output.source_partition != planned.partition)
        {
            return Err(SchedulerError::PartitionOutputSetMismatch);
        }

        let mut emissions = Vec::new();
        let mut future_work = self.remaining;
        for (mut output, planned) in outputs.into_iter().zip(&self.partitions) {
            output
                .work_outputs
                .sort_by_key(|work_output| work_output.key);
            if output
                .work_outputs
                .windows(2)
                .any(|pair| pair[0].key == pair[1].key)
            {
                return Err(SchedulerError::DuplicateWorkOutput);
            }
            if output.work_outputs.len() != planned.work.len()
                || output
                    .work_outputs
                    .iter()
                    .zip(&planned.work)
                    .any(|(work_output, scheduled)| work_output.key != scheduled.key)
            {
                return Err(SchedulerError::WorkOutputSetMismatch);
            }

            let actual = output.work_outputs.iter().try_fold(0_u32, |total, work| {
                let count = u32::try_from(work.emissions.len())
                    .map_err(|_| SchedulerError::EventCountOverflow)?;
                total
                    .checked_add(count)
                    .ok_or(SchedulerError::EventCountOverflow)
            })?;
            if actual > max_events_per_partition {
                return Err(SchedulerError::EventBudgetExceeded {
                    partition: output.source_partition,
                    actual,
                    maximum: max_events_per_partition,
                });
            }

            for work_output in output.work_outputs {
                if work_output
                    .emissions
                    .iter()
                    .any(|emission| emission.origin_work != work_output.key)
                    || work_output
                        .deferred_work
                        .iter()
                        .any(|deferred| deferred.origin_work != work_output.key)
                {
                    return Err(SchedulerError::UnknownEmissionOrigin);
                }
                validate_emission_indices(
                    work_output
                        .emissions
                        .iter()
                        .map(|emission| emission.emission_index),
                )?;
                validate_emission_indices(
                    work_output
                        .deferred_work
                        .iter()
                        .map(|deferred| deferred.emission_index),
                )?;

                for emission in &work_output.emissions {
                    if emission.destination.cell().level() != self.partition_level {
                        return Err(SchedulerError::PartitionLevelMismatch {
                            expected: self.partition_level,
                            actual: emission.destination.cell().level(),
                        });
                    }
                }
                emissions.extend(work_output.emissions);

                for deferred in work_output.deferred_work {
                    if deferred.work.due_tick <= self.to_tick {
                        return Err(SchedulerError::SameTickGeneratedWork {
                            due_tick: deferred.work.due_tick,
                            resolved_tick: self.to_tick,
                        });
                    }
                    if deferred.work.partition.cell().level() != self.partition_level {
                        return Err(SchedulerError::PartitionLevelMismatch {
                            expected: self.partition_level,
                            actual: deferred.work.partition.cell().level(),
                        });
                    }
                    future_work.push(deferred.work);
                }
            }
        }

        emissions.sort_by(Emission::canonical_cmp);
        if emissions
            .windows(2)
            .any(|pair| pair[0].canonical_cmp(&pair[1]) == Ordering::Equal)
        {
            return Err(SchedulerError::DuplicateEmissionKey);
        }
        let next_schedule = PartitionSchedule::new(self.partition_level, future_work)?;
        Ok(ResolvedTick {
            from_tick: self.from_tick,
            to_tick: self.to_tick,
            emissions,
            next_schedule,
        })
    }
}

fn validate_emission_indices(values: impl Iterator<Item = u32>) -> Result<(), SchedulerError> {
    let mut indices = values.collect::<Vec<_>>();
    indices.sort_unstable();
    for (expected, actual) in indices.into_iter().enumerate() {
        let expected = u32::try_from(expected).map_err(|_| SchedulerError::EventCountOverflow)?;
        if actual != expected {
            return Err(SchedulerError::NonCanonicalEmissionIndices);
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedTick<E> {
    from_tick: SimTick,
    to_tick: SimTick,
    emissions: Vec<Emission<E>>,
    next_schedule: PartitionSchedule,
}

impl<E> ResolvedTick<E> {
    #[must_use]
    pub const fn start_tick(&self) -> SimTick {
        self.from_tick
    }

    #[must_use]
    pub const fn to_tick(&self) -> SimTick {
        self.to_tick
    }

    #[must_use]
    pub fn emissions(&self) -> &[Emission<E>] {
        &self.emissions
    }

    #[must_use]
    pub const fn next_schedule(&self) -> &PartitionSchedule {
        &self.next_schedule
    }
}

/// Deterministic output from the operational partition-capacity workload.
///
/// This is not canonical world history. It exercises the same queue, routing, result
/// validation, barrier ordering, checkpoint, and event-budget code with a caller-sized
/// population so release builds can publish a reproducible scheduler envelope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PartitionCapacityProbe {
    pub population: u32,
    pub active_percent: u8,
    pub active_population: u32,
    pub ticks: u32,
    pub emitted_events: u64,
    pub canonical_event_bytes: u64,
    pub event_stream_digest: Digest,
    pub final_schedule_digest: Digest,
}

#[derive(Clone, Debug)]
struct CapacityProbeSubject {
    id: EntityId,
    location: S2CellId,
    next_due: SimTick,
    completed_steps: u32,
}

#[derive(Serialize)]
struct CapacityProbeEvent {
    tick: SimTick,
    subject_id: EntityId,
    location: S2CellId,
    completed_steps: u32,
    next_due: SimTick,
}

fn capacity_probe_work(subject: &CapacityProbeSubject) -> Result<ScheduledWork, SchedulerError> {
    ScheduledWork::routed(
        subject.next_due,
        subject.location,
        10,
        WorkKey::new(
            10,
            SubjectKey::from_entity(subject.id),
            20,
            subject.completed_steps,
        )?,
    )
}

/// Run a database-free deterministic population/active-fraction scheduler sample.
/// Wall-clock measurement deliberately belongs to the caller and cannot enter this
/// result or any simulation state.
pub fn run_partition_capacity_probe(
    population: u32,
    active_percent: u8,
    ticks: u32,
) -> Result<PartitionCapacityProbe, SchedulerError> {
    if population == 0 || population > CAPACITY_PROBE_MAX_POPULATION {
        return Err(SchedulerError::InvalidCapacityProbePopulation(population));
    }
    if !(1..=100).contains(&active_percent) {
        return Err(SchedulerError::InvalidCapacityProbeActivePercent(
            active_percent,
        ));
    }
    if ticks == 0 || ticks > CAPACITY_PROBE_MAX_TICKS {
        return Err(SchedulerError::InvalidCapacityProbeTicks(ticks));
    }

    let active_population =
        u32::try_from((u64::from(population) * u64::from(active_percent)).div_ceil(100))
            .map_err(|_| SchedulerError::EventCountOverflow)?;
    let inactive_due = SimTick::new(u64::from(ticks) + 1);
    let locations = [
        "0000000100000000",
        "2000000100000000",
        "4000000100000000",
        "6000000100000000",
        "8000000100000000",
        "a000000100000000",
    ]
    .map(str::parse::<S2CellId>)
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let mut subjects = (0..population)
        .map(|index| {
            let id = EntityId::from_uuid(uuid::Uuid::from_u128(u128::from(index) + 1));
            let location_index = usize::try_from(index % 6).expect("index modulo six fits usize");
            let subject = CapacityProbeSubject {
                id,
                location: locations[location_index],
                next_due: if index < active_population {
                    SimTick::new(1)
                } else {
                    inactive_due
                },
                completed_steps: 0,
            };
            (SubjectKey::from_entity(id), subject)
        })
        .collect::<BTreeMap<_, _>>();
    let mut schedule = PartitionSchedule::new(
        10,
        subjects
            .values()
            .map(capacity_probe_work)
            .collect::<Result<Vec<_>, _>>()?,
    )?;
    let mut current_tick = SimTick::ZERO;
    let mut emitted_events = 0_u64;
    let mut canonical_event_bytes = 0_u64;
    let mut event_stream_digest = Digest::ZERO;

    for _ in 0..ticks {
        let plan = schedule.plan_next_tick(current_tick)?;
        let outputs = plan
            .partitions()
            .iter()
            .map(|partition| {
                let work_outputs = partition
                    .work()
                    .iter()
                    .map(|work| {
                        let subject = subjects
                            .get(&work.key().subject())
                            .expect("scheduled capacity subject exists");
                        let completed_steps = subject
                            .completed_steps
                            .checked_add(1)
                            .ok_or(SchedulerError::EventCountOverflow)?;
                        let next_due = plan.to_tick().checked_next()?;
                        let next_subject = CapacityProbeSubject {
                            id: subject.id,
                            location: subject.location,
                            next_due,
                            completed_steps,
                        };
                        let event = CapacityProbeEvent {
                            tick: plan.to_tick(),
                            subject_id: subject.id,
                            location: subject.location,
                            completed_steps,
                            next_due,
                        };
                        Ok(WorkOutput::new(
                            work.key(),
                            vec![Emission::new(work.partition(), work.key(), 0, event)],
                            vec![DeferredWork::new(
                                work.key(),
                                0,
                                capacity_probe_work(&next_subject)?,
                            )],
                        ))
                    })
                    .collect::<Result<Vec<_>, SchedulerError>>()?;
                Ok(PartitionOutput::new(partition.partition(), work_outputs))
            })
            .collect::<Result<Vec<_>, SchedulerError>>()?;
        let resolved = plan.complete(outputs, population)?;
        for emission in resolved.emissions() {
            let event = emission.event();
            let bytes = serde_json::to_vec(event)
                .map_err(|error| SchedulerError::Encoding(error.to_string()))?;
            canonical_event_bytes = canonical_event_bytes
                .checked_add(
                    u64::try_from(bytes.len()).map_err(|_| SchedulerError::EventCountOverflow)?,
                )
                .ok_or(SchedulerError::EventCountOverflow)?;
            emitted_events = emitted_events
                .checked_add(1)
                .ok_or(SchedulerError::EventCountOverflow)?;
            let mut digest_input = Vec::with_capacity(40 + bytes.len());
            digest_input.extend_from_slice(event_stream_digest.as_bytes());
            digest_input.extend_from_slice(
                &u64::try_from(bytes.len())
                    .map_err(|_| SchedulerError::EventCountOverflow)?
                    .to_be_bytes(),
            );
            digest_input.extend_from_slice(&bytes);
            event_stream_digest = Digest::sha256(&digest_input);
            let subject = subjects
                .get_mut(&emission.origin_work().subject())
                .expect("resolved capacity subject exists");
            subject.next_due = event.next_due;
            subject.completed_steps = event.completed_steps;
        }
        current_tick = resolved.to_tick();
        schedule = resolved.next_schedule().clone();
    }
    let expected_events = u64::from(active_population) * u64::from(ticks);
    if emitted_events != expected_events {
        return Err(SchedulerError::CapacityProbeEventCountMismatch {
            expected: expected_events,
            actual: emitted_events,
        });
    }
    let final_schedule_digest = PartitionScheduleCheckpoint::new(schedule).content_digest()?;
    Ok(PartitionCapacityProbe {
        population,
        active_percent,
        active_population,
        ticks,
        emitted_events,
        canonical_event_bytes,
        event_stream_digest,
        final_schedule_digest,
    })
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum SchedulerError {
    #[error("partition S2 level {0} is outside 0 through 30")]
    InvalidPartitionLevel(u8),
    #[error("partition has S2 level {actual}; configured level is {expected}")]
    PartitionLevelMismatch { expected: u8, actual: u8 },
    #[error("scheduler phase code zero is reserved")]
    ZeroPhaseCode,
    #[error("scheduler process code zero is reserved")]
    ZeroProcessCode,
    #[error("work key is duplicated at tick {due_tick}")]
    DuplicateWorkKey { due_tick: SimTick },
    #[error("work due at {due_tick} is overdue when planning tick {target_tick}")]
    OverdueWork {
        due_tick: SimTick,
        target_tick: SimTick,
    },
    #[error("maximum events per partition must be greater than zero")]
    ZeroEventBudget,
    #[error("a partition returned more than one output")]
    DuplicatePartitionOutput,
    #[error("worker outputs do not exactly match the planned active partitions")]
    PartitionOutputSetMismatch,
    #[error("one due work key returned more than one explicit result")]
    DuplicateWorkOutput,
    #[error("work results do not exactly match every due key in their source partition")]
    WorkOutputSetMismatch,
    #[error("partition {partition:?} emitted {actual} events; configured maximum is {maximum}")]
    EventBudgetExceeded {
        partition: PartitionId,
        actual: u32,
        maximum: u32,
    },
    #[error("partition event count exceeds u32::MAX")]
    EventCountOverflow,
    #[error("an emission does not name work from its source partition plan")]
    UnknownEmissionOrigin,
    #[error("emission indices for one work item must be contiguous from zero")]
    NonCanonicalEmissionIndices,
    #[error("two resolved emissions have the same canonical key")]
    DuplicateEmissionKey,
    #[error("work generated while resolving tick {resolved_tick} is due at {due_tick}")]
    SameTickGeneratedWork {
        due_tick: SimTick,
        resolved_tick: SimTick,
    },
    #[error("scheduler checkpoint schema {0} is unsupported")]
    UnsupportedScheduleSchema(u16),
    #[error("scheduler checkpoint bytes are not canonical")]
    NonCanonicalCheckpointEncoding,
    #[error("could not encode scheduler checkpoint: {0}")]
    Encoding(String),
    #[error("could not decode scheduler checkpoint: {0}")]
    Decode(String),
    #[error("capacity probe population {0} is outside 1 through 1000000")]
    InvalidCapacityProbePopulation(u32),
    #[error("capacity probe active percent {0} is outside 1 through 100")]
    InvalidCapacityProbeActivePercent(u8),
    #[error("capacity probe tick count {0} is outside 1 through 10000")]
    InvalidCapacityProbeTicks(u32),
    #[error("capacity probe emitted {actual} events instead of {expected}")]
    CapacityProbeEventCountMismatch { expected: u64, actual: u64 },
    #[error(transparent)]
    S2(#[from] S2CellIdError),
    #[error(transparent)]
    Time(#[from] TimeOverflow),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use serde::Serialize;
    use uuid::Uuid;

    const PARTITION_LEVEL: u8 = 10;

    fn entity(value: u128) -> EntityId {
        EntityId::from_uuid(Uuid::from_u128(value))
    }

    fn key(value: u128, occurrence: u32) -> WorkKey {
        WorkKey::new(10, SubjectKey::from_entity(entity(value)), 20, occurrence)
            .expect("valid work key")
    }

    fn cell(value: &str) -> S2CellId {
        value.parse().expect("valid S2 fixture")
    }

    fn scheduled(
        due_tick: u64,
        destination: &str,
        subject: u128,
        occurrence: u32,
    ) -> ScheduledWork {
        ScheduledWork::routed(
            SimTick::new(due_tick),
            cell(destination),
            PARTITION_LEVEL,
            key(subject, occurrence),
        )
        .expect("valid scheduled work")
    }

    #[test]
    fn routing_and_input_permutation_are_stable_across_all_six_faces() {
        let mut work = Vec::new();
        for face in 0_u64..6 {
            let descendant =
                S2CellId::new((face << 61) | 0x0000_0001_0000_0000).expect("valid face descendant");
            let routed = ScheduledWork::routed(
                SimTick::new(1),
                descendant,
                PARTITION_LEVEL,
                key(u128::from(face) + 1, 0),
            )
            .expect("descendant routes");
            assert_eq!(
                routed.partition.cell().face(),
                u8::try_from(face).expect("face fits")
            );
            assert_eq!(routed.partition.cell().level(), PARTITION_LEVEL);
            work.push(routed);
        }

        let first = PartitionSchedule::new(PARTITION_LEVEL, work.clone()).expect("valid schedule");
        work.reverse();
        let second = PartitionSchedule::new(PARTITION_LEVEL, work).expect("valid schedule");
        assert_eq!(first, second);

        assert!(matches!(
            PartitionId::new(cell("0000000100000000"), PARTITION_LEVEL),
            Err(SchedulerError::PartitionLevelMismatch { .. })
        ));
    }

    #[test]
    fn duplicate_work_keys_are_rejected_even_across_partitions() {
        let shared_key = key(1, 0);
        let left = ScheduledWork::routed(
            SimTick::new(1),
            cell("0000000100000000"),
            PARTITION_LEVEL,
            shared_key,
        )
        .expect("valid left work");
        let right = ScheduledWork::routed(
            SimTick::new(1),
            cell("2000000100000000"),
            PARTITION_LEVEL,
            shared_key,
        )
        .expect("valid right work");

        assert!(matches!(
            PartitionSchedule::new(PARTITION_LEVEL, vec![left, right]),
            Err(SchedulerError::DuplicateWorkKey { .. })
        ));
    }

    #[test]
    fn checkpoint_is_canonical_validated_and_input_order_independent() {
        let left = scheduled(2, "0000000100000000", 1, 0);
        let right = scheduled(1, "2000000100000000", 2, 0);
        let first = PartitionSchedule::new(PARTITION_LEVEL, vec![left, right])
            .expect("valid first schedule");
        let second = PartitionSchedule::new(PARTITION_LEVEL, vec![right, left])
            .expect("valid second schedule");

        let first_checkpoint = PartitionScheduleCheckpoint::new(first);
        let second_checkpoint = PartitionScheduleCheckpoint::new(second);
        let first_bytes = first_checkpoint.canonical_bytes().expect("canonical bytes");
        assert_eq!(
            first_bytes,
            second_checkpoint.canonical_bytes().expect("same bytes")
        );
        assert_eq!(
            first_checkpoint
                .content_digest()
                .expect("checkpoint digest"),
            second_checkpoint.content_digest().expect("same digest")
        );
        assert_eq!(
            PartitionScheduleCheckpoint::from_canonical_slice(&first_bytes)
                .expect("strict checkpoint decoding"),
            first_checkpoint
        );

        let pretty = serde_json::to_vec_pretty(&first_checkpoint).expect("pretty JSON");
        assert!(matches!(
            PartitionScheduleCheckpoint::from_canonical_slice(&pretty),
            Err(SchedulerError::NonCanonicalCheckpointEncoding)
        ));
    }

    #[test]
    fn checkpoint_rejects_invalid_work_and_schema() {
        let schedule = PartitionSchedule::new(
            PARTITION_LEVEL,
            vec![scheduled(1, "0000000100000000", 1, 0)],
        )
        .expect("valid schedule");
        let checkpoint = PartitionScheduleCheckpoint::new(schedule);
        let canonical = String::from_utf8(
            checkpoint
                .canonical_bytes()
                .expect("canonical checkpoint bytes"),
        )
        .expect("checkpoint JSON is UTF-8");

        let invalid_work = canonical.replacen("\"phase_code\":10", "\"phase_code\":0", 1);
        assert!(matches!(
            PartitionScheduleCheckpoint::from_canonical_slice(invalid_work.as_bytes()),
            Err(SchedulerError::Decode(_))
        ));

        let unsupported = canonical.replacen(
            "\"schedule_schema_version\":1",
            "\"schedule_schema_version\":2",
            1,
        );
        assert!(matches!(
            PartitionScheduleCheckpoint::from_canonical_slice(unsupported.as_bytes()),
            Err(SchedulerError::UnsupportedScheduleSchema(2))
        ));
    }

    #[test]
    fn barrier_merge_ignores_worker_arrival_and_defers_cross_partition_work() {
        let left_work = scheduled(1, "0000000100000000", 1, 0);
        let right_work = scheduled(1, "2000000100000000", 2, 0);
        let schedule = PartitionSchedule::new(PARTITION_LEVEL, vec![right_work, left_work])
            .expect("valid schedule");
        let plan = schedule
            .plan_next_tick(SimTick::ZERO)
            .expect("valid tick plan");
        let left_partition = left_work.partition;
        let right_partition = right_work.partition;
        let future = scheduled(2, "2000000100000000", 1, 1);

        let left = PartitionOutput::new(
            left_partition,
            vec![WorkOutput::new(
                left_work.key,
                vec![Emission::new(left_partition, left_work.key, 0, "left")],
                vec![DeferredWork::new(left_work.key, 0, future)],
            )],
        );
        let right = PartitionOutput::new(
            right_partition,
            vec![WorkOutput::new(
                right_work.key,
                vec![Emission::new(left_partition, right_work.key, 0, "right")],
                Vec::new(),
            )],
        );

        let forward = plan
            .clone()
            .complete(vec![left.clone(), right.clone()], 1)
            .expect("forward completion");
        let reverse = plan
            .complete(vec![right, left], 1)
            .expect("reverse completion");

        assert_eq!(forward, reverse);
        assert_eq!(forward.to_tick(), SimTick::new(1));
        assert_eq!(
            forward
                .emissions()
                .iter()
                .map(Emission::event)
                .copied()
                .collect::<Vec<_>>(),
            vec!["left", "right"]
        );
        assert_eq!(forward.next_schedule().entries(), &[future]);

        let same_tick = DeferredWork::new(
            left_work.key,
            0,
            ScheduledWork::new(SimTick::new(1), left_partition, key(1, 1)),
        );
        let invalid = schedule
            .plan_next_tick(SimTick::ZERO)
            .expect("repeatable plan")
            .complete(
                vec![
                    PartitionOutput::new(
                        left_partition,
                        vec![WorkOutput::new(
                            left_work.key,
                            Vec::<Emission<&str>>::new(),
                            vec![same_tick],
                        )],
                    ),
                    PartitionOutput::new(
                        right_partition,
                        vec![WorkOutput::new(right_work.key, Vec::new(), Vec::new())],
                    ),
                ],
                1,
            );
        assert!(matches!(
            invalid,
            Err(SchedulerError::SameTickGeneratedWork { .. })
        ));
    }

    #[test]
    fn budget_failure_leaves_the_original_plan_repeatable() {
        let work = scheduled(1, "0000000100000000", 1, 0);
        let schedule = PartitionSchedule::new(PARTITION_LEVEL, vec![work]).expect("valid schedule");
        let output = PartitionOutput::new(
            work.partition,
            vec![WorkOutput::new(
                work.key,
                vec![
                    Emission::new(work.partition, work.key, 0, "first"),
                    Emission::new(work.partition, work.key, 1, "second"),
                ],
                Vec::new(),
            )],
        );

        let rejected = schedule
            .plan_next_tick(SimTick::ZERO)
            .expect("valid plan")
            .complete(vec![output.clone()], 1);
        assert!(matches!(
            rejected,
            Err(SchedulerError::EventBudgetExceeded {
                actual: 2,
                maximum: 1,
                ..
            })
        ));

        let first_retry = schedule
            .plan_next_tick(SimTick::ZERO)
            .expect("same plan after rejection")
            .complete(vec![output.clone()], 2)
            .expect("exact budget succeeds");
        let second_retry = schedule
            .plan_next_tick(SimTick::ZERO)
            .expect("same plan again")
            .complete(vec![output], 2)
            .expect("same completion again");
        assert_eq!(first_retry, second_retry);
        assert!(first_retry.next_schedule().entries().is_empty());
    }

    #[test]
    fn missing_duplicate_unknown_and_gapped_outputs_fail_closed() {
        let work = scheduled(1, "0000000100000000", 1, 0);
        let schedule = PartitionSchedule::new(PARTITION_LEVEL, vec![work]).expect("valid schedule");
        let plan = schedule.plan_next_tick(SimTick::ZERO).expect("valid plan");

        assert!(matches!(
            plan.clone().complete::<()>(Vec::new(), 1),
            Err(SchedulerError::PartitionOutputSetMismatch)
        ));

        let empty = PartitionOutput::new(work.partition, Vec::<WorkOutput<()>>::new());
        assert!(matches!(
            plan.clone().complete(vec![empty.clone(), empty], 1),
            Err(SchedulerError::DuplicatePartitionOutput)
        ));

        assert!(matches!(
            plan.clone().complete(
                vec![PartitionOutput::<()>::new(work.partition, Vec::new())],
                1,
            ),
            Err(SchedulerError::WorkOutputSetMismatch)
        ));

        let duplicated_work = WorkOutput::new(work.key, Vec::<Emission<()>>::new(), Vec::new());
        assert!(matches!(
            plan.clone().complete(
                vec![PartitionOutput::new(
                    work.partition,
                    vec![duplicated_work.clone(), duplicated_work],
                )],
                1,
            ),
            Err(SchedulerError::DuplicateWorkOutput)
        ));

        let unknown = PartitionOutput::new(
            work.partition,
            vec![WorkOutput::new(
                work.key,
                vec![Emission::new(work.partition, key(2, 0), 0, ())],
                Vec::new(),
            )],
        );
        assert!(matches!(
            plan.clone().complete(vec![unknown], 1),
            Err(SchedulerError::UnknownEmissionOrigin)
        ));

        let gapped = PartitionOutput::new(
            work.partition,
            vec![WorkOutput::new(
                work.key,
                vec![Emission::new(work.partition, work.key, 1, ())],
                Vec::new(),
            )],
        );
        assert!(matches!(
            plan.clone().complete(vec![gapped], 1),
            Err(SchedulerError::NonCanonicalEmissionIndices)
        ));

        let explicit_empty = PartitionOutput::new(
            work.partition,
            vec![WorkOutput::new(
                work.key,
                Vec::<Emission<()>>::new(),
                Vec::new(),
            )],
        );
        assert!(plan.complete(vec![explicit_empty], 1).is_ok());
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct SyntheticPerson {
        id: EntityId,
        location: S2CellId,
        period_ticks: u64,
        next_due: SimTick,
        completed_steps: u32,
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize)]
    struct SyntheticEvent {
        tick: SimTick,
        person_id: EntityId,
        location: S2CellId,
        completed_steps: u32,
        next_due: SimTick,
    }

    fn person_work(person: &SyntheticPerson) -> ScheduledWork {
        ScheduledWork::routed(
            person.next_due,
            person.location,
            PARTITION_LEVEL,
            WorkKey::new(
                10,
                SubjectKey::from_entity(person.id),
                20,
                person.completed_steps,
            )
            .expect("valid person work key"),
        )
        .expect("person location routes")
    }

    fn propose_synthetic_work(
        people: &BTreeMap<SubjectKey, SyntheticPerson>,
        work: ScheduledWork,
        tick: SimTick,
    ) -> WorkOutput<SyntheticEvent> {
        let person = people
            .get(&work.key.subject())
            .expect("scheduled person exists");
        assert_eq!(person.next_due, tick);
        assert_eq!(person_work(person), work);

        let mut person = person.clone();
        person.completed_steps += 1;
        if person.id == entity(1) && person.completed_steps == 1 {
            person.location = cell("2000000100000000");
        }
        person.next_due = SimTick::new(
            tick.get()
                .checked_add(person.period_ticks)
                .expect("small fixture tick"),
        );

        let destination =
            PartitionId::route(person.location, PARTITION_LEVEL).expect("event location routes");
        let event = SyntheticEvent {
            tick,
            person_id: person.id,
            location: person.location,
            completed_steps: person.completed_steps,
            next_due: person.next_due,
        };
        let next = person_work(&person);
        WorkOutput::new(
            work.key,
            vec![Emission::new(destination, work.key, 0, event)],
            vec![DeferredWork::new(work.key, 0, next)],
        )
    }

    fn apply_synthetic_events(
        people: &mut BTreeMap<SubjectKey, SyntheticPerson>,
        emissions: &[Emission<SyntheticEvent>],
    ) {
        for emission in emissions {
            let event = emission.event();
            let person = people
                .get_mut(&emission.origin_work().subject())
                .expect("resolved person exists");
            assert_eq!(person.id, event.person_id);
            person.location = event.location;
            person.completed_steps = event.completed_steps;
            person.next_due = event.next_due;
        }
    }

    fn fixture_people() -> BTreeMap<SubjectKey, SyntheticPerson> {
        [
            SyntheticPerson {
                id: entity(1),
                location: cell("0000000100000000"),
                period_ticks: 1,
                next_due: SimTick::new(1),
                completed_steps: 0,
            },
            SyntheticPerson {
                id: entity(2),
                location: cell("0000000100000000"),
                period_ticks: 2,
                next_due: SimTick::new(1),
                completed_steps: 0,
            },
            SyntheticPerson {
                id: entity(3),
                location: cell("4000000100000000"),
                period_ticks: 2,
                next_due: SimTick::new(2),
                completed_steps: 0,
            },
        ]
        .into_iter()
        .map(|person| (SubjectKey::from_entity(person.id), person))
        .collect()
    }

    fn propose_partition_outputs(
        people: &BTreeMap<SubjectKey, SyntheticPerson>,
        plan: &TickPlan,
    ) -> Vec<PartitionOutput<SyntheticEvent>> {
        plan.partitions()
            .iter()
            .map(|partition| {
                let work_outputs = partition
                    .work()
                    .iter()
                    .map(|work| propose_synthetic_work(people, *work, plan.to_tick()))
                    .collect();
                PartitionOutput::new(partition.partition(), work_outputs)
            })
            .collect()
    }

    #[test]
    fn rejected_barrier_leaves_causal_state_and_schedule_unchanged() {
        let mut people = fixture_people();
        let before = people.clone();
        let schedule =
            PartitionSchedule::new(PARTITION_LEVEL, people.values().map(person_work).collect())
                .expect("valid schedule");
        let plan = schedule
            .plan_next_tick(SimTick::ZERO)
            .expect("valid tick plan");
        let outputs = propose_partition_outputs(&people, &plan);

        assert!(matches!(
            plan.clone().complete(outputs.clone(), 1),
            Err(SchedulerError::EventBudgetExceeded {
                actual: 2,
                maximum: 1,
                ..
            })
        ));
        assert_eq!(people, before);
        assert_eq!(
            schedule
                .plan_next_tick(SimTick::ZERO)
                .expect("rejected plan is repeatable"),
            plan
        );

        let resolved = plan
            .complete(outputs, 2)
            .expect("same proposals pass at exact budget");
        assert_eq!(people, before, "proposals do not mutate causal state");
        apply_synthetic_events(&mut people, resolved.emissions());
        assert_ne!(
            people, before,
            "state changes only after barrier acceptance"
        );
    }

    #[test]
    fn queued_partition_execution_matches_dense_reference_every_tick() {
        let mut dense_people = fixture_people();
        let mut partitioned_people = dense_people.clone();
        let initial_work = partitioned_people
            .values()
            .map(person_work)
            .collect::<Vec<_>>();
        let mut schedule =
            PartitionSchedule::new(PARTITION_LEVEL, initial_work).expect("valid initial schedule");
        let mut current_tick = SimTick::ZERO;

        for iteration in 0..6 {
            let target_tick = current_tick.checked_next().expect("small fixture tick");
            let plan = schedule
                .plan_next_tick(current_tick)
                .expect("partition tick plans");
            let queued_due = plan
                .partitions()
                .iter()
                .flat_map(PartitionWork::work)
                .copied()
                .collect::<Vec<_>>();

            let mut dense_due = dense_people
                .values()
                .filter(|person| person.next_due == target_tick)
                .map(person_work)
                .collect::<Vec<_>>();
            dense_due.sort_unstable();
            assert_eq!(queued_due, dense_due, "due work differs at {target_tick}");

            let dense_work_outputs = dense_due
                .into_iter()
                .map(|work| propose_synthetic_work(&dense_people, work, target_tick))
                .collect::<Vec<_>>();
            let mut dense_emissions = dense_work_outputs
                .iter()
                .flat_map(|output| output.emissions.iter().cloned())
                .collect::<Vec<_>>();
            dense_emissions.sort_by(Emission::canonical_cmp);

            let mut outputs = propose_partition_outputs(&partitioned_people, &plan);
            if iteration % 2 == 1 {
                outputs.reverse();
            }
            let resolved = plan
                .complete(outputs, 10)
                .expect("partition barrier resolves");
            apply_synthetic_events(&mut dense_people, &dense_emissions);
            apply_synthetic_events(&mut partitioned_people, resolved.emissions());

            let partitioned_events = resolved
                .emissions()
                .iter()
                .map(Emission::event)
                .cloned()
                .collect::<Vec<_>>();
            let dense_events = dense_emissions
                .iter()
                .map(Emission::event)
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(partitioned_events, dense_events);
            assert_eq!(
                serde_json::to_vec(&partitioned_events).expect("event bytes serialize"),
                serde_json::to_vec(&dense_events).expect("event bytes serialize")
            );
            assert_eq!(partitioned_people, dense_people);

            let dense_next = PartitionSchedule::new(
                PARTITION_LEVEL,
                dense_people.values().map(person_work).collect(),
            )
            .expect("dense next schedule");
            assert_eq!(resolved.next_schedule(), &dense_next);
            assert_eq!(partitioned_people.len(), 3, "people remain individual");

            current_tick = resolved.to_tick();
            schedule = resolved.next_schedule().clone();
        }
    }

    #[test]
    fn worker_disruption_and_checkpoint_restart_cannot_change_history() {
        let mut reference_people = fixture_people();
        let mut disrupted_people = reference_people.clone();
        let initial_work = reference_people
            .values()
            .map(person_work)
            .collect::<Vec<_>>();
        let mut reference_schedule = PartitionSchedule::new(PARTITION_LEVEL, initial_work.clone())
            .expect("valid reference schedule");
        let mut disrupted_schedule = PartitionSchedule::new(PARTITION_LEVEL, initial_work)
            .expect("valid disrupted schedule");
        let mut current_tick = SimTick::ZERO;

        for iteration in 0..12 {
            // A process restart must recover the exact canonical queue before it
            // can assign any work. Worker identity and ownership are deliberately
            // absent from this checkpoint.
            let checkpoint = PartitionScheduleCheckpoint::new(disrupted_schedule);
            let checkpoint_bytes = checkpoint.canonical_bytes().expect("checkpoint bytes");
            disrupted_schedule =
                PartitionScheduleCheckpoint::from_canonical_slice(&checkpoint_bytes)
                    .expect("checkpoint restart")
                    .schedule()
                    .clone();

            let reference_plan = reference_schedule
                .plan_next_tick(current_tick)
                .expect("reference tick plan");
            let disrupted_plan = disrupted_schedule
                .plan_next_tick(current_tick)
                .expect("disrupted tick plan");
            assert_eq!(reference_plan, disrupted_plan);

            let reference_outputs = propose_partition_outputs(&reference_people, &reference_plan);

            // Recompute all immutable proposals to model a timed-out attempt being
            // retried on different workers. Then perturb arrival order to model
            // delay and reassignment. Only the final complete result set reaches
            // the barrier; duplicate or partial sets are rejected by separate tests.
            let mut disrupted_outputs =
                propose_partition_outputs(&disrupted_people, &disrupted_plan);
            if iteration % 2 == 0 {
                disrupted_outputs.reverse();
            } else if disrupted_outputs.len() > 1 {
                disrupted_outputs.rotate_left(1);
            }

            let reference_resolved = reference_plan
                .complete(reference_outputs, 10)
                .expect("reference barrier resolves");
            let disrupted_resolved = disrupted_plan
                .complete(disrupted_outputs, 10)
                .expect("disrupted barrier resolves");

            assert_eq!(
                serde_json::to_vec(
                    &reference_resolved
                        .emissions()
                        .iter()
                        .map(Emission::event)
                        .collect::<Vec<_>>(),
                )
                .expect("reference event bytes"),
                serde_json::to_vec(
                    &disrupted_resolved
                        .emissions()
                        .iter()
                        .map(Emission::event)
                        .collect::<Vec<_>>(),
                )
                .expect("disrupted event bytes")
            );
            assert_eq!(
                reference_resolved.next_schedule(),
                disrupted_resolved.next_schedule()
            );

            apply_synthetic_events(&mut reference_people, reference_resolved.emissions());
            apply_synthetic_events(&mut disrupted_people, disrupted_resolved.emissions());
            assert_eq!(reference_people, disrupted_people);

            current_tick = reference_resolved.to_tick();
            assert_eq!(current_tick, disrupted_resolved.to_tick());
            reference_schedule = reference_resolved.next_schedule().clone();
            disrupted_schedule = disrupted_resolved.next_schedule().clone();
        }
    }

    #[test]
    fn capacity_probe_is_bounded_reproducible_and_counts_only_active_subjects() {
        let first = run_partition_capacity_probe(101, 10, 7).expect("capacity probe");
        let second = run_partition_capacity_probe(101, 10, 7).expect("same capacity probe");
        assert_eq!(first, second);
        assert_eq!(first.active_population, 11);
        assert_eq!(first.emitted_events, 77);
        assert!(first.canonical_event_bytes > first.emitted_events);
        assert_ne!(first.event_stream_digest, Digest::ZERO);
        assert_ne!(first.final_schedule_digest, Digest::ZERO);

        assert!(matches!(
            run_partition_capacity_probe(0, 10, 7),
            Err(SchedulerError::InvalidCapacityProbePopulation(0))
        ));
        assert!(matches!(
            run_partition_capacity_probe(101, 0, 7),
            Err(SchedulerError::InvalidCapacityProbeActivePercent(0))
        ));
        assert!(matches!(
            run_partition_capacity_probe(101, 10, 0),
            Err(SchedulerError::InvalidCapacityProbeTicks(0))
        ));
    }

    #[test]
    fn empty_tick_advances_once_and_overdue_work_is_never_skipped() {
        let empty = PartitionSchedule::new(PARTITION_LEVEL, Vec::new()).expect("empty schedule");
        let resolved = empty
            .plan_next_tick(SimTick::new(7))
            .expect("empty tick plans")
            .complete::<()>(Vec::new(), 1)
            .expect("empty tick resolves");
        assert_eq!(resolved.start_tick(), SimTick::new(7));
        assert_eq!(resolved.to_tick(), SimTick::new(8));

        let overdue = PartitionSchedule::new(
            PARTITION_LEVEL,
            vec![scheduled(1, "0000000100000000", 1, 0)],
        )
        .expect("valid schedule");
        assert!(matches!(
            overdue.plan_next_tick(SimTick::new(1)),
            Err(SchedulerError::OverdueWork { .. })
        ));
    }
}
