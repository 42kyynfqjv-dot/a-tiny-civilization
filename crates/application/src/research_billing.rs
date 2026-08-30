use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;
use world_domain::Digest;

pub const LEGACY_CANCER_FIREWORKS_RECONCILIATION_SCHEMA_VERSION: u16 = 1;
pub const CANCER_FIREWORKS_RECONCILIATION_SCHEMA_VERSION: u16 = 2;
pub const CANCER_FIREWORKS_BILLING_EXPORT_FORMAT: &str =
    "fireworks_firectl_billing_export_metrics_csv_v1";
pub const CANCER_FIREWORKS_GPT_OSS_20B_MODEL: &str = "accounts/fireworks/models/gpt-oss-20b";
pub const CANCER_FIREWORKS_NEMOTRON_LIGHTNING_3_5_MODEL: &str =
    "accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b";
pub const CANCER_FIREWORKS_DISPATCH_MATCH_TOLERANCE_SECONDS: i64 = 5;
pub const CANCER_FIREWORKS_GPT_OSS_20B_INPUT_MICRO_USD_PER_MILLION_TOKENS: u64 = 70_000;
pub const CANCER_FIREWORKS_GPT_OSS_20B_OUTPUT_MICRO_USD_PER_MILLION_TOKENS: u64 = 300_000;
pub const CANCER_FIREWORKS_NEMOTRON_LIGHTNING_3_5_INPUT_MICRO_USD_PER_MILLION_TOKENS: u64 = 50_000;
pub const CANCER_FIREWORKS_NEMOTRON_LIGHTNING_3_5_OUTPUT_MICRO_USD_PER_MILLION_TOKENS: u64 =
    200_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchFireworksDispatchCandidate {
    pub request_id: Uuid,
    pub route_index: u16,
    pub requested_model: String,
    pub dispatched_at: DateTime<Utc>,
    pub billing_month: NaiveDate,
    pub reserved_micro_usd: u64,
}

impl CancerResearchFireworksDispatchCandidate {
    pub fn validate(&self) -> Result<(), CancerResearchBillingReconciliationError> {
        if !is_supported_fireworks_research_model(&self.requested_model)
            || self.route_index >= super::MAX_CANCER_RESEARCH_NETWORK_ATTEMPTS
            || self.billing_month.day() != 1
            || self.reserved_micro_usd == 0
            || self.reserved_micro_usd > super::MAX_CANCER_RESEARCH_PAID_RESERVATION_MICRO_USD
        {
            return Err(CancerResearchBillingReconciliationError::InvalidDispatchCandidate);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CancerResearchFireworksCostReconciliation {
    pub schema_version: u16,
    pub reconciliation_id: Uuid,
    pub request_id: Uuid,
    pub route_index: u16,
    pub billing_month: NaiveDate,
    pub source_format: String,
    pub export_hash: Digest,
    pub export_byte_length: u64,
    pub row_hash: Digest,
    pub row_start_offset: u64,
    pub row_byte_length: u64,
    pub provider_started_at: DateTime<Utc>,
    pub matched_dispatch_at: DateTime<Utc>,
    pub requested_model: String,
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub actual_micro_usd: u64,
    pub reserved_micro_usd: u64,
    pub released_micro_usd: u64,
}

impl CancerResearchFireworksCostReconciliation {
    #[allow(clippy::too_many_arguments)]
    pub fn from_export_row(
        candidate: &CancerResearchFireworksDispatchCandidate,
        export_hash: Digest,
        export_byte_length: u64,
        row_hash: Digest,
        row_start_offset: u64,
        row_byte_length: u64,
        provider_started_at: DateTime<Utc>,
        prompt_tokens: u32,
        completion_tokens: u32,
    ) -> Result<Self, CancerResearchBillingReconciliationError> {
        candidate.validate()?;
        let actual_micro_usd = fireworks_research_billed_micro_usd(
            &candidate.requested_model,
            prompt_tokens,
            completion_tokens,
        )?;
        let released_micro_usd = candidate
            .reserved_micro_usd
            .checked_sub(actual_micro_usd)
            .ok_or(CancerResearchBillingReconciliationError::CostExceedsReservation)?;
        let reconciliation_id = Uuid::new_v5(&candidate.request_id, row_hash.as_bytes());
        let reconciliation = Self {
            schema_version: CANCER_FIREWORKS_RECONCILIATION_SCHEMA_VERSION,
            reconciliation_id,
            request_id: candidate.request_id,
            route_index: candidate.route_index,
            billing_month: candidate.billing_month,
            source_format: CANCER_FIREWORKS_BILLING_EXPORT_FORMAT.to_owned(),
            export_hash,
            export_byte_length,
            row_hash,
            row_start_offset,
            row_byte_length,
            provider_started_at,
            matched_dispatch_at: candidate.dispatched_at,
            requested_model: candidate.requested_model.clone(),
            prompt_tokens,
            completion_tokens,
            actual_micro_usd,
            reserved_micro_usd: candidate.reserved_micro_usd,
            released_micro_usd,
        };
        reconciliation.validate_against(candidate)?;
        Ok(reconciliation)
    }

    pub fn validate_against(
        &self,
        candidate: &CancerResearchFireworksDispatchCandidate,
    ) -> Result<(), CancerResearchBillingReconciliationError> {
        candidate.validate()?;
        let timestamp_delta = self
            .provider_started_at
            .signed_duration_since(candidate.dispatched_at)
            .num_milliseconds()
            .unsigned_abs();
        let max_delta = u64::try_from(CANCER_FIREWORKS_DISPATCH_MATCH_TOLERANCE_SECONDS)
            .unwrap_or_default()
            .saturating_mul(1_000);
        let expected_actual = fireworks_research_billed_micro_usd(
            &candidate.requested_model,
            self.prompt_tokens,
            self.completion_tokens,
        )?;
        let expected_released = candidate
            .reserved_micro_usd
            .checked_sub(expected_actual)
            .ok_or(CancerResearchBillingReconciliationError::CostExceedsReservation)?;
        let supported_schema = self.schema_version
            == CANCER_FIREWORKS_RECONCILIATION_SCHEMA_VERSION
            || (self.schema_version == LEGACY_CANCER_FIREWORKS_RECONCILIATION_SCHEMA_VERSION
                && self.requested_model == CANCER_FIREWORKS_GPT_OSS_20B_MODEL);
        if !supported_schema
            || self.reconciliation_id != Uuid::new_v5(&self.request_id, self.row_hash.as_bytes())
            || self.request_id != candidate.request_id
            || self.route_index != candidate.route_index
            || self.billing_month != candidate.billing_month
            || self.source_format != CANCER_FIREWORKS_BILLING_EXPORT_FORMAT
            || self.export_hash == Digest::ZERO
            || self.export_byte_length == 0
            || self.export_byte_length > 128 * 1024 * 1024
            || self.row_hash == Digest::ZERO
            || self.row_byte_length == 0
            || self
                .row_start_offset
                .checked_add(self.row_byte_length)
                .is_none_or(|end| end > self.export_byte_length)
            || self.matched_dispatch_at != candidate.dispatched_at
            || self.requested_model != candidate.requested_model
            || timestamp_delta > max_delta
            || self.actual_micro_usd != expected_actual
            || self.reserved_micro_usd != candidate.reserved_micro_usd
            || self.released_micro_usd != expected_released
        {
            return Err(CancerResearchBillingReconciliationError::InvalidReconciliation);
        }
        Ok(())
    }
}

pub fn validate_fireworks_reconciliation_batch(
    reconciliations: &[CancerResearchFireworksCostReconciliation],
) -> Result<(), CancerResearchBillingReconciliationError> {
    let mut request_ids = BTreeSet::new();
    let mut reconciliation_ids = BTreeSet::new();
    let mut row_hashes = BTreeSet::new();
    for reconciliation in reconciliations {
        if !request_ids.insert(reconciliation.request_id)
            || !reconciliation_ids.insert(reconciliation.reconciliation_id)
            || !row_hashes.insert(reconciliation.row_hash)
        {
            return Err(CancerResearchBillingReconciliationError::DuplicateEvidence);
        }
    }
    Ok(())
}

/// Parse one authoritative `firectl billing export-metrics` CSV and require
/// exactly one provider row for every supplied unresolved dispatch. Rows for
/// other models and already-resolved calls may coexist in the monthly export;
/// no such row is admitted or persisted by this function.
pub fn reconcile_fireworks_billing_export(
    export_bytes: &[u8],
    candidates: &[CancerResearchFireworksDispatchCandidate],
) -> Result<Vec<CancerResearchFireworksCostReconciliation>, CancerResearchBillingReconciliationError>
{
    if export_bytes.is_empty() {
        return Err(CancerResearchBillingReconciliationError::InvalidCsv(
            "billing export is empty".to_owned(),
        ));
    }
    let records = csv_record_ranges(export_bytes)?;
    let (header_start, header_end) = records.first().copied().ok_or_else(|| {
        CancerResearchBillingReconciliationError::InvalidCsv(
            "billing export omitted its header".to_owned(),
        )
    })?;
    let header = parse_csv_record(&export_bytes[header_start..header_end])?;
    let columns = BillingColumns::resolve(&header)?;
    let export_hash = Digest::sha256(export_bytes);
    let mut provider_rows = Vec::new();
    for &(start, end) in &records[1..] {
        let raw_record = &export_bytes[start..end];
        let fields = parse_csv_record(raw_record)?;
        if fields.len() != header.len() {
            return Err(CancerResearchBillingReconciliationError::InvalidCsv(
                "billing CSV row width differs from its header".to_owned(),
            ));
        }
        if fields[columns.usage_type] != "TEXT_COMPLETION_INFERENCE_USAGE"
            || !is_supported_fireworks_research_model(&fields[columns.model])
        {
            continue;
        }
        let provider_started_at = parse_provider_timestamp(&fields[columns.timestamp])?;
        let prompt_tokens = parse_token_count(&fields[columns.prompt_tokens], "prompt_tokens")?;
        let completion_tokens =
            parse_token_count(&fields[columns.completion_tokens], "completion_tokens")?;
        if prompt_tokens == 0 && completion_tokens == 0 {
            return Err(CancerResearchBillingReconciliationError::EmptyUsage);
        }
        provider_rows.push(ParsedBillingRow {
            requested_model: fields[columns.model].clone(),
            row_hash: Digest::sha256(raw_record),
            row_start_offset: u64::try_from(start).map_err(|_| {
                CancerResearchBillingReconciliationError::InvalidCsv(
                    "billing export row offset overflowed".to_owned(),
                )
            })?,
            row_byte_length: u64::try_from(end - start).map_err(|_| {
                CancerResearchBillingReconciliationError::InvalidCsv(
                    "billing export row length overflowed".to_owned(),
                )
            })?,
            provider_started_at,
            prompt_tokens,
            completion_tokens,
        });
    }

    let mut seen_requests = BTreeSet::new();
    for candidate in candidates {
        candidate.validate()?;
        if !seen_requests.insert(candidate.request_id) {
            return Err(CancerResearchBillingReconciliationError::DuplicateEvidence);
        }
    }
    let mut reconciliations = Vec::with_capacity(candidates.len());
    let mut used_rows = BTreeSet::new();
    for candidate in candidates {
        let matches = provider_rows
            .iter()
            .filter(|row| {
                row.requested_model == candidate.requested_model
                    && row
                        .provider_started_at
                        .signed_duration_since(candidate.dispatched_at)
                        .num_milliseconds()
                        .unsigned_abs()
                        <= u64::try_from(CANCER_FIREWORKS_DISPATCH_MATCH_TOLERANCE_SECONDS)
                            .unwrap_or_default()
                            .saturating_mul(1_000)
            })
            .collect::<Vec<_>>();
        let [provider_row] = matches.as_slice() else {
            return Err(CancerResearchBillingReconciliationError::MatchCardinality {
                request_id: candidate.request_id,
                matches: matches.len(),
            });
        };
        if !used_rows.insert(provider_row.row_hash) {
            return Err(CancerResearchBillingReconciliationError::DuplicateEvidence);
        }
        reconciliations.push(CancerResearchFireworksCostReconciliation::from_export_row(
            candidate,
            export_hash,
            u64::try_from(export_bytes.len()).map_err(|_| {
                CancerResearchBillingReconciliationError::InvalidCsv(
                    "billing export length overflowed".to_owned(),
                )
            })?,
            provider_row.row_hash,
            provider_row.row_start_offset,
            provider_row.row_byte_length,
            provider_row.provider_started_at,
            provider_row.prompt_tokens,
            provider_row.completion_tokens,
        )?);
    }
    for provider_row in &provider_rows {
        let matching_candidates = candidates
            .iter()
            .filter(|candidate| {
                provider_row.requested_model == candidate.requested_model
                    && provider_row
                        .provider_started_at
                        .signed_duration_since(candidate.dispatched_at)
                        .num_milliseconds()
                        .unsigned_abs()
                        <= u64::try_from(CANCER_FIREWORKS_DISPATCH_MATCH_TOLERANCE_SECONDS)
                            .unwrap_or_default()
                            .saturating_mul(1_000)
            })
            .count();
        if matching_candidates > 1 {
            return Err(
                CancerResearchBillingReconciliationError::AmbiguousDispatches {
                    matches: matching_candidates,
                },
            );
        }
    }
    validate_fireworks_reconciliation_batch(&reconciliations)?;
    Ok(reconciliations)
}

struct ParsedBillingRow {
    requested_model: String,
    row_hash: Digest,
    row_start_offset: u64,
    row_byte_length: u64,
    provider_started_at: DateTime<Utc>,
    prompt_tokens: u32,
    completion_tokens: u32,
}

struct BillingColumns {
    timestamp: usize,
    usage_type: usize,
    model: usize,
    prompt_tokens: usize,
    completion_tokens: usize,
}

impl BillingColumns {
    fn resolve(header: &[String]) -> Result<Self, CancerResearchBillingReconciliationError> {
        let mut positions = BTreeMap::new();
        for (index, name) in header.iter().enumerate() {
            if positions.insert(name.as_str(), index).is_some() {
                return Err(CancerResearchBillingReconciliationError::InvalidCsv(
                    format!("duplicate billing export column {name:?}"),
                ));
            }
        }
        Ok(Self {
            timestamp: exactly_one_alias(
                &positions,
                &["start_time", "request_timestamp", "timestamp"],
                "request timestamp",
            )?,
            usage_type: exactly_one_alias(&positions, &["usage_type"], "usage type")?,
            model: exactly_one_alias(&positions, &["base_model_name", "model"], "model")?,
            prompt_tokens: exactly_one_alias(
                &positions,
                &["prompt_tokens", "input_tokens"],
                "prompt tokens",
            )?,
            completion_tokens: exactly_one_alias(
                &positions,
                &["completion_tokens", "output_tokens"],
                "completion tokens",
            )?,
        })
    }
}

fn exactly_one_alias(
    positions: &BTreeMap<&str, usize>,
    aliases: &[&str],
    field: &str,
) -> Result<usize, CancerResearchBillingReconciliationError> {
    let matches = aliases
        .iter()
        .filter_map(|alias| positions.get(alias).copied())
        .collect::<Vec<_>>();
    let [position] = matches.as_slice() else {
        return Err(CancerResearchBillingReconciliationError::InvalidCsv(
            format!("billing export requires exactly one {field} column"),
        ));
    };
    Ok(*position)
}

fn parse_provider_timestamp(
    value: &str,
) -> Result<DateTime<Utc>, CancerResearchBillingReconciliationError> {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Ok(timestamp.with_timezone(&Utc));
    }
    for format in [
        "%Y-%m-%d %H:%M:%S UTC",
        "%Y-%m-%d %H:%M:%S%.f UTC",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
    ] {
        if let Ok(timestamp) = NaiveDateTime::parse_from_str(value, format) {
            return Ok(Utc.from_utc_datetime(&timestamp));
        }
    }
    Err(CancerResearchBillingReconciliationError::InvalidCsv(
        "billing export request timestamp is invalid".to_owned(),
    ))
}

fn parse_token_count(
    value: &str,
    field: &str,
) -> Result<u32, CancerResearchBillingReconciliationError> {
    value.parse::<u32>().map_err(|_| {
        CancerResearchBillingReconciliationError::InvalidCsv(format!(
            "billing export {field} is not a canonical unsigned integer"
        ))
    })
}

fn csv_record_ranges(
    bytes: &[u8],
) -> Result<Vec<(usize, usize)>, CancerResearchBillingReconciliationError> {
    let mut records = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' if quoted && bytes.get(index + 1) == Some(&b'"') => index += 1,
            b'"' => quoted = !quoted,
            b'\n' if !quoted => {
                if index == start || (index == start + 1 && bytes[start] == b'\r') {
                    return Err(CancerResearchBillingReconciliationError::InvalidCsv(
                        "billing export contains an empty record".to_owned(),
                    ));
                }
                records.push((start, index + 1));
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    if quoted {
        return Err(CancerResearchBillingReconciliationError::InvalidCsv(
            "billing export has an unterminated quoted field".to_owned(),
        ));
    }
    if start < bytes.len() {
        records.push((start, bytes.len()));
    }
    Ok(records)
}

fn parse_csv_record(
    raw_record: &[u8],
) -> Result<Vec<String>, CancerResearchBillingReconciliationError> {
    let mut record = raw_record;
    if let Some(without_lf) = record.strip_suffix(b"\n") {
        record = without_lf.strip_suffix(b"\r").unwrap_or(without_lf);
    }
    let mut fields = Vec::new();
    let mut index = 0;
    loop {
        let mut field = Vec::new();
        if record.get(index) == Some(&b'"') {
            index += 1;
            loop {
                match record.get(index).copied() {
                    Some(b'"') if record.get(index + 1) == Some(&b'"') => {
                        field.push(b'"');
                        index += 2;
                    }
                    Some(b'"') => {
                        index += 1;
                        break;
                    }
                    Some(byte) => {
                        field.push(byte);
                        index += 1;
                    }
                    None => {
                        return Err(CancerResearchBillingReconciliationError::InvalidCsv(
                            "billing export has an unterminated quoted field".to_owned(),
                        ));
                    }
                }
            }
            if index < record.len() && record[index] != b',' {
                return Err(CancerResearchBillingReconciliationError::InvalidCsv(
                    "billing export has bytes after a closing quote".to_owned(),
                ));
            }
        } else {
            while index < record.len() && record[index] != b',' {
                if record[index] == b'"' {
                    return Err(CancerResearchBillingReconciliationError::InvalidCsv(
                        "billing export has a quote inside an unquoted field".to_owned(),
                    ));
                }
                field.push(record[index]);
                index += 1;
            }
        }
        fields.push(String::from_utf8(field).map_err(|_| {
            CancerResearchBillingReconciliationError::InvalidCsv(
                "billing export is not UTF-8".to_owned(),
            )
        })?);
        if index == record.len() {
            break;
        }
        index += 1;
        if index == record.len() {
            fields.push(String::new());
            break;
        }
    }
    Ok(fields)
}

pub fn fireworks_research_billed_micro_usd(
    requested_model: &str,
    prompt_tokens: u32,
    completion_tokens: u32,
) -> Result<u64, CancerResearchBillingReconciliationError> {
    let (input_price, output_price) = match requested_model {
        CANCER_FIREWORKS_GPT_OSS_20B_MODEL => (
            CANCER_FIREWORKS_GPT_OSS_20B_INPUT_MICRO_USD_PER_MILLION_TOKENS,
            CANCER_FIREWORKS_GPT_OSS_20B_OUTPUT_MICRO_USD_PER_MILLION_TOKENS,
        ),
        CANCER_FIREWORKS_NEMOTRON_LIGHTNING_3_5_MODEL => (
            CANCER_FIREWORKS_NEMOTRON_LIGHTNING_3_5_INPUT_MICRO_USD_PER_MILLION_TOKENS,
            CANCER_FIREWORKS_NEMOTRON_LIGHTNING_3_5_OUTPUT_MICRO_USD_PER_MILLION_TOKENS,
        ),
        _ => return Err(CancerResearchBillingReconciliationError::InvalidDispatchCandidate),
    };
    let numerator = u64::from(prompt_tokens)
        .checked_mul(input_price)
        .and_then(|input| {
            u64::from(completion_tokens)
                .checked_mul(output_price)
                .and_then(|output| input.checked_add(output))
        })
        .ok_or(CancerResearchBillingReconciliationError::CostOverflow)?;
    if numerator == 0 {
        return Err(CancerResearchBillingReconciliationError::EmptyUsage);
    }
    Ok(numerator.div_ceil(1_000_000))
}

pub fn fireworks_gpt_oss_20b_billed_micro_usd(
    prompt_tokens: u32,
    completion_tokens: u32,
) -> Result<u64, CancerResearchBillingReconciliationError> {
    fireworks_research_billed_micro_usd(
        CANCER_FIREWORKS_GPT_OSS_20B_MODEL,
        prompt_tokens,
        completion_tokens,
    )
}

fn is_supported_fireworks_research_model(model: &str) -> bool {
    matches!(
        model,
        CANCER_FIREWORKS_GPT_OSS_20B_MODEL | CANCER_FIREWORKS_NEMOTRON_LIGHTNING_3_5_MODEL
    )
}

#[derive(Debug, Error)]
pub enum CancerResearchBillingReconciliationError {
    #[error("Fireworks dispatch candidate is outside the closed paid route")]
    InvalidDispatchCandidate,
    #[error("Fireworks billing evidence contains no token usage")]
    EmptyUsage,
    #[error("Fireworks billing calculation overflowed")]
    CostOverflow,
    #[error("Fireworks billing cost exceeds its original reservation")]
    CostExceedsReservation,
    #[error("Fireworks billing reconciliation failed its immutable contract")]
    InvalidReconciliation,
    #[error("Fireworks billing reconciliation batch reuses a request, row, or identity")]
    DuplicateEvidence,
    #[error("Fireworks billing export is invalid: {0}")]
    InvalidCsv(String),
    #[error("Fireworks billing row match for request {request_id} had {matches} candidates")]
    MatchCardinality { request_id: Uuid, matches: usize },
    #[error("one Fireworks billing row matched {matches} indeterminate dispatches")]
    AmbiguousDispatches { matches: usize },
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn candidate() -> CancerResearchFireworksDispatchCandidate {
        CancerResearchFireworksDispatchCandidate {
            request_id: Uuid::parse_str("837382b3-532d-4988-9883-a5a9c43e0349")
                .expect("request UUID"),
            route_index: 1,
            requested_model: CANCER_FIREWORKS_GPT_OSS_20B_MODEL.to_owned(),
            dispatched_at: Utc
                .with_ymd_and_hms(2026, 8, 13, 2, 13, 1)
                .single()
                .expect("dispatch time"),
            billing_month: NaiveDate::from_ymd_opt(2026, 8, 1).expect("billing month"),
            reserved_micro_usd: 15_000,
        }
    }

    fn nemotron_candidate() -> CancerResearchFireworksDispatchCandidate {
        CancerResearchFireworksDispatchCandidate {
            request_id: Uuid::parse_str("a16a8ccb-6cbf-4c5f-8579-8dac2f1ec626")
                .expect("request UUID"),
            route_index: 8,
            requested_model: CANCER_FIREWORKS_NEMOTRON_LIGHTNING_3_5_MODEL.to_owned(),
            dispatched_at: Utc
                .with_ymd_and_hms(2026, 8, 13, 2, 13, 1)
                .single()
                .expect("dispatch time"),
            billing_month: NaiveDate::from_ymd_opt(2026, 8, 1).expect("billing month"),
            reserved_micro_usd: 250_000,
        }
    }

    #[test]
    fn pricing_is_the_full_input_fireworks_tariff_rounded_up() {
        assert!(matches!(
            fireworks_gpt_oss_20b_billed_micro_usd(1, 0),
            Ok(1)
        ));
        assert!(matches!(
            fireworks_gpt_oss_20b_billed_micro_usd(0, 1),
            Ok(1)
        ));
        assert!(matches!(
            fireworks_gpt_oss_20b_billed_micro_usd(58_026, 4_089),
            Ok(5_289)
        ));
        assert!(fireworks_gpt_oss_20b_billed_micro_usd(0, 0).is_err());
        assert!(matches!(
            fireworks_research_billed_micro_usd(
                CANCER_FIREWORKS_NEMOTRON_LIGHTNING_3_5_MODEL,
                58_026,
                4_089,
            ),
            Ok(3_720)
        ));
    }

    #[test]
    fn legacy_schema_only_accepts_the_historical_gpt_oss_tariff() {
        let old_candidate = candidate();
        let mut old = CancerResearchFireworksCostReconciliation::from_export_row(
            &old_candidate,
            Digest::sha256(b"whole export"),
            114,
            Digest::sha256(b"exact old CSV row\n"),
            90,
            20,
            old_candidate.dispatched_at,
            10,
            10,
        )
        .expect("current old-model evidence");
        old.schema_version = LEGACY_CANCER_FIREWORKS_RECONCILIATION_SCHEMA_VERSION;
        assert!(old.validate_against(&old_candidate).is_ok());

        let new_candidate = nemotron_candidate();
        let mut new = CancerResearchFireworksCostReconciliation::from_export_row(
            &new_candidate,
            Digest::sha256(b"whole export"),
            114,
            Digest::sha256(b"exact new CSV row\n"),
            90,
            20,
            new_candidate.dispatched_at,
            10,
            10,
        )
        .expect("current new-model evidence");
        new.schema_version = LEGACY_CANCER_FIREWORKS_RECONCILIATION_SCHEMA_VERSION;
        assert!(new.validate_against(&new_candidate).is_err());
    }

    #[test]
    fn reconciliation_is_content_addressed_and_tightly_time_bound() {
        let candidate = candidate();
        let evidence = CancerResearchFireworksCostReconciliation::from_export_row(
            &candidate,
            Digest::sha256(b"whole export"),
            114,
            Digest::sha256(b"exact CSV row\n"),
            100,
            14,
            candidate.dispatched_at + chrono::Duration::seconds(1),
            58_026,
            4_089,
        )
        .expect("valid evidence");
        assert_eq!(evidence.actual_micro_usd, 5_289);
        assert_eq!(evidence.released_micro_usd, 9_711);
        assert_eq!(
            evidence.reconciliation_id,
            Uuid::new_v5(&candidate.request_id, evidence.row_hash.as_bytes())
        );

        let mut late = evidence;
        late.provider_started_at = candidate.dispatched_at + chrono::Duration::seconds(6);
        assert!(late.validate_against(&candidate).is_err());
        late.provider_started_at = candidate.dispatched_at;
        late.row_start_offset = late.export_byte_length;
        assert!(
            late.validate_against(&candidate).is_err(),
            "the exact row range must lie inside the hashed export"
        );
    }

    #[test]
    fn batch_rejects_reused_source_rows() {
        let candidate = candidate();
        let evidence = CancerResearchFireworksCostReconciliation::from_export_row(
            &candidate,
            Digest::sha256(b"whole export"),
            114,
            Digest::sha256(b"exact CSV row\n"),
            100,
            14,
            candidate.dispatched_at,
            10,
            10,
        )
        .expect("valid evidence");
        assert!(validate_fireworks_reconciliation_batch(&[evidence.clone(), evidence]).is_err());
    }

    #[test]
    fn official_fireworks_export_headers_match_one_indeterminate_dispatch() {
        let candidate = candidate();
        let csv = concat!(
            "email,start_time,end_time,usage_type,accelerator_type,accelerator_seconds,",
            "base_model_name,model_bucket,parameter_count,prompt_tokens,completion_tokens\r\n",
            "research@example.invalid,2026-08-13 02:13:02 UTC,2026-08-13 02:13:10 UTC,",
            "TEXT_COMPLETION_INFERENCE_USAGE,,,accounts/fireworks/models/gpt-oss-20b,",
            "GPT OSS 20B,20000000000,58026,4089\r\n",
            "research@example.invalid,2026-08-13 02:14:00 UTC,2026-08-13 02:14:01 UTC,",
            "TEXT_COMPLETION_INFERENCE_USAGE,,,accounts/fireworks/models/deepseek-v4-flash,",
            "DeepSeek,1,1,1\r\n",
        );
        let reconciliations = reconcile_fireworks_billing_export(csv.as_bytes(), &[candidate])
            .expect("official export");
        assert_eq!(reconciliations.len(), 1);
        assert_eq!(reconciliations[0].actual_micro_usd, 5_289);
        assert_eq!(
            reconciliations[0].export_hash,
            Digest::sha256(csv.as_bytes())
        );
        assert_eq!(
            reconciliations[0].row_hash,
            Digest::sha256(
                concat!(
                    "research@example.invalid,2026-08-13 02:13:02 UTC,",
                    "2026-08-13 02:13:10 UTC,TEXT_COMPLETION_INFERENCE_USAGE,,,",
                    "accounts/fireworks/models/gpt-oss-20b,GPT OSS 20B,",
                    "20000000000,58026,4089\r\n"
                )
                .as_bytes()
            )
        );
    }

    #[test]
    fn explicit_observed_aliases_and_quoted_fields_are_supported() {
        let candidate = candidate();
        let csv = concat!(
            "email,request_timestamp,usage_type,model,input_tokens,output_tokens\n",
            "\"research, account@example.invalid\",2026-08-13T02:13:02Z,",
            "TEXT_COMPLETION_INFERENCE_USAGE,accounts/fireworks/models/gpt-oss-20b,",
            "58026,4089\n",
        );
        assert_eq!(
            reconcile_fireworks_billing_export(csv.as_bytes(), &[candidate])
                .expect("alias export")
                .len(),
            1
        );
    }

    #[test]
    fn importer_refuses_missing_unknown_ambiguous_and_reused_rows() {
        let candidate = candidate();
        let missing = concat!(
            "start_time,usage_type,base_model_name,prompt_tokens\n",
            "2026-08-13 02:13:02 UTC,TEXT_COMPLETION_INFERENCE_USAGE,",
            "accounts/fireworks/models/gpt-oss-20b,1\n",
        );
        assert!(
            reconcile_fireworks_billing_export(
                missing.as_bytes(),
                std::slice::from_ref(&candidate)
            )
            .is_err()
        );

        let unknown_required_alias = concat!(
            "start_time,usage_type,base_model_name,prompt_count,completion_tokens\n",
            "2026-08-13 02:13:02 UTC,TEXT_COMPLETION_INFERENCE_USAGE,",
            "accounts/fireworks/models/gpt-oss-20b,1,1\n",
        );
        assert!(
            reconcile_fireworks_billing_export(
                unknown_required_alias.as_bytes(),
                std::slice::from_ref(&candidate)
            )
            .is_err()
        );

        let ambiguous_header = concat!(
            "start_time,timestamp,usage_type,base_model_name,prompt_tokens,completion_tokens\n",
            "2026-08-13 02:13:02 UTC,2026-08-13 02:13:02 UTC,",
            "TEXT_COMPLETION_INFERENCE_USAGE,accounts/fireworks/models/gpt-oss-20b,1,1\n",
        );
        assert!(
            reconcile_fireworks_billing_export(
                ambiguous_header.as_bytes(),
                std::slice::from_ref(&candidate),
            )
            .is_err()
        );

        let two_rows = concat!(
            "start_time,usage_type,base_model_name,prompt_tokens,completion_tokens\n",
            "2026-08-13 02:13:02 UTC,TEXT_COMPLETION_INFERENCE_USAGE,",
            "accounts/fireworks/models/gpt-oss-20b,1,1\n",
            "2026-08-13 02:13:03 UTC,TEXT_COMPLETION_INFERENCE_USAGE,",
            "accounts/fireworks/models/gpt-oss-20b,2,2\n",
        );
        assert!(reconcile_fireworks_billing_export(two_rows.as_bytes(), &[candidate]).is_err());
    }

    #[test]
    fn one_provider_row_cannot_be_used_to_guess_between_close_dispatches() {
        let first = candidate();
        let mut second = first.clone();
        second.request_id = Uuid::new_v4();
        second.dispatched_at += chrono::Duration::seconds(2);
        let csv = concat!(
            "start_time,usage_type,base_model_name,prompt_tokens,completion_tokens\n",
            "2026-08-13 02:13:02 UTC,TEXT_COMPLETION_INFERENCE_USAGE,",
            "accounts/fireworks/models/gpt-oss-20b,58026,4089\n",
        );
        assert!(
            reconcile_fireworks_billing_export(csv.as_bytes(), &[first, second]).is_err(),
            "a single row inside both windows is ambiguous, not first-match wins"
        );
    }

    #[test]
    fn simultaneous_rows_for_distinct_models_match_their_exact_dispatches() {
        let old = candidate();
        let new = nemotron_candidate();
        let csv = concat!(
            "start_time,usage_type,base_model_name,prompt_tokens,completion_tokens\n",
            "2026-08-13 02:13:02 UTC,TEXT_COMPLETION_INFERENCE_USAGE,",
            "accounts/fireworks/models/gpt-oss-20b,58026,4089\n",
            "2026-08-13 02:13:02 UTC,TEXT_COMPLETION_INFERENCE_USAGE,",
            "accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b,58026,4089\n",
        );
        let reconciliations = reconcile_fireworks_billing_export(csv.as_bytes(), &[old, new])
            .expect("model identity disambiguates simultaneous provider rows");
        assert_eq!(reconciliations.len(), 2);
        assert_eq!(reconciliations[0].actual_micro_usd, 5_289);
        assert_eq!(reconciliations[1].actual_micro_usd, 3_720);
    }
}
