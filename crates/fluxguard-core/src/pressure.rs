use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::{
    Applicability, Availability, BudgetSnapshot, BudgetWindow, Freshness, SourceId, SourceQuality,
    WindowId,
};

/// Overall pressure imposed by the strongest applicable constraint.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PressureLevel {
    Normal,
    Guarded,
    Conserve,
    Critical,
    Emergency,
    Blocked,
    Unknown,
}

/// Identity of the constraint selected as the bottleneck.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ConstraintRef {
    pub source: SourceId,
    pub window: WindowId,
}

/// Confidence in the pressure result.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
    Unknown,
}

/// Stable machine-readable reasons for a pressure assessment or recommendation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    WeeklyQuotaLow,
    FiveHourQuotaLow,
    ContextLow,
    OperationCostHigh,
    OperationOptional,
    SourceStale,
    SourceEstimated,
    AvailabilityBlocked,
    AvailabilityUnknown,
    UnknownConstraint,
    ResetTimestampPassed,
    ResetImminent,
    ResetSoon,
    ResetLater,
    AvoidParallelSubagents,
    AvoidOptionalRefactor,
    UseTargetedTests,
    CheckpointAfterMilestone,
    RefreshBeforeExpensiveWork,
    ContinueSingleAgent,
    PerformTargetedInspection,
    Other(String),
}

/// Thresholds used to classify the strongest applicable remaining percentage.
#[derive(Clone, Debug, PartialEq)]
pub struct PressureConfig {
    pub guarded_remaining_percent: f64,
    pub conserve_remaining_percent: f64,
    pub critical_remaining_percent: f64,
    pub emergency_remaining_percent: f64,
}

impl Default for PressureConfig {
    fn default() -> Self {
        Self {
            guarded_remaining_percent: 50.0,
            conserve_remaining_percent: 25.0,
            critical_remaining_percent: 10.0,
            emergency_remaining_percent: 3.0,
        }
    }
}

impl PressureConfig {
    pub fn validate(&self) -> Result<(), crate::DomainError> {
        let values = [
            self.guarded_remaining_percent,
            self.conserve_remaining_percent,
            self.critical_remaining_percent,
            self.emergency_remaining_percent,
        ];
        if values.iter().any(|value| !value.is_finite()) {
            return Err(crate::DomainError::NonFiniteValue);
        }
        if !(100.0 >= values[0]
            && values[0] >= values[1]
            && values[1] >= values[2]
            && values[2] >= values[3]
            && values[3] >= 0.0)
        {
            return Err(crate::DomainError::InvalidThresholds);
        }
        Ok(())
    }
}

/// Collection of source snapshots assessed together.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CombinedSnapshot {
    pub snapshots: Vec<BudgetSnapshot>,
}

/// Result of assessing the strongest relevant resource constraint.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PressureAssessment {
    pub level: PressureLevel,
    pub bottleneck: Option<ConstraintRef>,
    pub effective_remaining_percent: Option<f64>,
    pub resets_at: Option<OffsetDateTime>,
    pub confidence: Confidence,
    pub reasons: Vec<ReasonCode>,
}

pub fn assess_snapshot(snapshot: &BudgetSnapshot, config: &PressureConfig) -> PressureAssessment {
    assess_snapshot_at(snapshot, config, OffsetDateTime::now_utc())
}

pub fn assess_snapshot_at(
    snapshot: &BudgetSnapshot,
    config: &PressureConfig,
    now: OffsetDateTime,
) -> PressureAssessment {
    assess_combined_at(
        &CombinedSnapshot {
            snapshots: vec![snapshot.clone()],
        },
        config,
        now,
    )
}

pub fn assess_combined(
    snapshots: &CombinedSnapshot,
    config: &PressureConfig,
) -> PressureAssessment {
    assess_combined_at(snapshots, config, OffsetDateTime::now_utc())
}

pub fn assess_combined_at(
    snapshots: &CombinedSnapshot,
    config: &PressureConfig,
    now: OffsetDateTime,
) -> PressureAssessment {
    if config.validate().is_err() {
        return unknown_assessment(vec![ReasonCode::UnknownConstraint]);
    }

    let mut unknown_applicability = false;
    let mut selected: Option<(&BudgetSnapshot, &BudgetWindow, f64)> = None;

    for snapshot in &snapshots.snapshots {
        if matches!(snapshot.availability, Availability::Blocked { .. }) {
            return PressureAssessment {
                level: PressureLevel::Blocked,
                bottleneck: first_constraint(snapshot),
                effective_remaining_percent: None,
                resets_at: None,
                confidence: confidence_for(snapshot.source.source_quality.clone(), None),
                reasons: vec![ReasonCode::AvailabilityBlocked],
            };
        }

        if matches!(snapshot.availability, Availability::Unknown) {
            unknown_applicability = true;
        }

        for window in &snapshot.windows {
            if matches!(window.applicability, Applicability::Unknown) {
                unknown_applicability = true;
            }
            if !matches!(window.applicability, Applicability::Applicable) {
                continue;
            }

            if window.hard_blocked {
                return PressureAssessment {
                    level: PressureLevel::Blocked,
                    bottleneck: Some(ConstraintRef {
                        source: snapshot.source.id.clone(),
                        window: window.id.clone(),
                    }),
                    effective_remaining_percent: remaining_percent(window),
                    resets_at: window.resets_at,
                    confidence: confidence_for(
                        snapshot.source.source_quality.clone(),
                        Some(window.freshness.clone()),
                    ),
                    reasons: vec![ReasonCode::AvailabilityBlocked],
                };
            }

            let Some(remaining) = remaining_percent(window) else {
                continue;
            };
            if selected
                .as_ref()
                .is_none_or(|(_, _, current)| remaining < *current)
            {
                selected = Some((snapshot, window, remaining));
            }
        }
    }

    let Some((snapshot, window, remaining)) = selected else {
        let mut reasons = vec![ReasonCode::UnknownConstraint];
        if unknown_applicability {
            reasons.push(ReasonCode::AvailabilityUnknown);
        }
        return unknown_assessment(reasons);
    };

    let mut reasons = vec![window_reason(window, remaining, config)];
    if matches!(window.freshness, Freshness::Stale) {
        reasons.push(ReasonCode::SourceStale);
    }
    if matches!(
        snapshot.source.source_quality,
        SourceQuality::Estimated | SourceQuality::Experimental
    ) {
        reasons.push(ReasonCode::SourceEstimated);
    }
    if unknown_applicability {
        reasons.push(ReasonCode::AvailabilityUnknown);
    }
    if let Some(reset) = window.resets_at {
        reasons.push(reset_reason(reset, now));
    }

    let mut confidence = confidence_for(
        snapshot.source.source_quality.clone(),
        Some(window.freshness.clone()),
    );
    if unknown_applicability && confidence == Confidence::High {
        confidence = Confidence::Medium;
    }

    PressureAssessment {
        level: classify(remaining, config),
        bottleneck: Some(ConstraintRef {
            source: snapshot.source.id.clone(),
            window: window.id.clone(),
        }),
        effective_remaining_percent: Some(remaining),
        resets_at: window.resets_at,
        confidence,
        reasons,
    }
}

fn classify(remaining: f64, config: &PressureConfig) -> PressureLevel {
    if remaining > config.guarded_remaining_percent {
        PressureLevel::Normal
    } else if remaining > config.conserve_remaining_percent {
        PressureLevel::Guarded
    } else if remaining > config.critical_remaining_percent {
        PressureLevel::Conserve
    } else if remaining > config.emergency_remaining_percent {
        PressureLevel::Critical
    } else {
        PressureLevel::Emergency
    }
}

fn remaining_percent(window: &BudgetWindow) -> Option<f64> {
    let value = window.remaining_percent.or_else(|| {
        let remaining = window.remaining?;
        let limit = window.limit?;
        if limit.0 <= 0.0 {
            return None;
        }
        Some(remaining.0 / limit.0 * 100.0)
    })?;
    (value.is_finite() && (0.0..=100.0).contains(&value)).then_some(value)
}

fn confidence_for(quality: SourceQuality, freshness: Option<Freshness>) -> Confidence {
    if matches!(freshness, Some(Freshness::Stale)) {
        return Confidence::Low;
    }
    match quality {
        SourceQuality::OfficialStructured => Confidence::High,
        SourceQuality::OfficialCli
        | SourceQuality::OfficialHeaders
        | SourceQuality::OfficialTelemetry
        | SourceQuality::Manual => Confidence::Medium,
        SourceQuality::Estimated | SourceQuality::Experimental => Confidence::Low,
    }
}

fn first_constraint(snapshot: &BudgetSnapshot) -> Option<ConstraintRef> {
    snapshot.windows.first().map(|window| ConstraintRef {
        source: snapshot.source.id.clone(),
        window: window.id.clone(),
    })
}

fn unknown_assessment(reasons: Vec<ReasonCode>) -> PressureAssessment {
    PressureAssessment {
        level: PressureLevel::Unknown,
        bottleneck: None,
        effective_remaining_percent: None,
        resets_at: None,
        confidence: Confidence::Unknown,
        reasons,
    }
}

fn window_reason(window: &BudgetWindow, remaining: f64, config: &PressureConfig) -> ReasonCode {
    let id = window.id.as_str().to_ascii_lowercase();
    if id.contains("weekly") && remaining <= config.guarded_remaining_percent {
        ReasonCode::WeeklyQuotaLow
    } else if id.contains("five_hour") && remaining <= config.guarded_remaining_percent {
        ReasonCode::FiveHourQuotaLow
    } else if id.contains("context") && remaining <= config.guarded_remaining_percent {
        ReasonCode::ContextLow
    } else {
        ReasonCode::Other("constraint_pressure".into())
    }
}

fn reset_reason(reset: OffsetDateTime, now: OffsetDateTime) -> ReasonCode {
    let until = reset - now;
    if until < Duration::ZERO {
        ReasonCode::ResetTimestampPassed
    } else if until <= Duration::minutes(5) {
        ReasonCode::ResetImminent
    } else if until <= Duration::hours(1) {
        ReasonCode::ResetSoon
    } else {
        ReasonCode::ResetLater
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Applicability, Availability, BudgetSnapshot, Freshness, MetricDimension, Provenance,
        SnapshotWarning, SourceCapabilities, SourceDescriptor, SourceKind,
    };

    fn snapshot(id: &str, remaining: Option<f64>) -> BudgetSnapshot {
        let source = SourceId::new("client.codex").expect("valid source");
        BudgetSnapshot {
            source: SourceDescriptor {
                id: source.clone(),
                kind: SourceKind::Client,
                display_name: "Codex".into(),
                adapter_version: "0.1.0".into(),
                source_quality: SourceQuality::OfficialStructured,
                capabilities: SourceCapabilities::default(),
            },
            account_scope: None,
            availability: Availability::Allowed,
            windows: vec![BudgetWindow {
                id: WindowId::new(id).expect("valid window"),
                label: None,
                dimension: MetricDimension::Requests,
                used: None,
                limit: None,
                remaining: None,
                used_percent: None,
                remaining_percent: remaining,
                window_duration_seconds: None,
                resets_at: None,
                fresh_until: None,
                hard_blocked: false,
                applicability: Applicability::Applicable,
                observed_at: OffsetDateTime::UNIX_EPOCH,
                freshness: Freshness::Fresh,
                provenance: Provenance {
                    source_id: source,
                    source_quality: SourceQuality::OfficialStructured,
                    observed_via: None,
                },
            }],
            observed_at: OffsetDateTime::UNIX_EPOCH,
            warnings: Vec::<SnapshotWarning>::new(),
        }
    }

    #[test]
    fn boundary_values_select_expected_levels() {
        let config = PressureConfig::default();
        let cases = [
            ("normal", 50.1, PressureLevel::Normal),
            ("guarded", 50.0, PressureLevel::Guarded),
            ("conserve", 25.0, PressureLevel::Conserve),
            ("critical", 10.0, PressureLevel::Critical),
            ("emergency", 3.0, PressureLevel::Emergency),
        ];

        for (id, remaining, expected) in cases {
            let result = assess_snapshot_at(
                &snapshot(id, Some(remaining)),
                &config,
                OffsetDateTime::UNIX_EPOCH,
            );
            assert_eq!(result.level, expected, "{id}");
        }
    }

    #[test]
    fn combined_limits_choose_lowest_applicable_window_without_averaging() {
        let snapshots = CombinedSnapshot {
            snapshots: vec![
                snapshot("weekly", Some(8.0)),
                snapshot("five_hour", Some(80.0)),
            ],
        };

        let result = assess_combined_at(
            &snapshots,
            &PressureConfig::default(),
            OffsetDateTime::UNIX_EPOCH,
        );

        assert_eq!(result.level, PressureLevel::Critical);
        assert_eq!(result.effective_remaining_percent, Some(8.0));
        assert_eq!(
            result.bottleneck.expect("bottleneck").window.as_str(),
            "weekly"
        );
    }

    #[test]
    fn blocked_state_dominates_numeric_pressure() {
        let mut blocked = snapshot("weekly", Some(100.0));
        blocked.availability = Availability::Blocked {
            reason: crate::BlockReason::QuotaExhausted,
        };

        let result = assess_snapshot_at(
            &blocked,
            &PressureConfig::default(),
            OffsetDateTime::UNIX_EPOCH,
        );

        assert_eq!(result.level, PressureLevel::Blocked);
        assert!(result.reasons.contains(&ReasonCode::AvailabilityBlocked));
    }

    #[test]
    fn stale_estimated_data_lowers_confidence_and_does_not_recover() {
        let mut stale = snapshot("weekly", Some(8.0));
        stale.source.source_quality = SourceQuality::Estimated;
        stale.windows[0].freshness = Freshness::Stale;
        stale.windows[0].resets_at = Some(OffsetDateTime::UNIX_EPOCH);

        let result = assess_snapshot_at(
            &stale,
            &PressureConfig::default(),
            OffsetDateTime::UNIX_EPOCH + Duration::hours(1),
        );

        assert_eq!(result.level, PressureLevel::Critical);
        assert_eq!(result.confidence, Confidence::Low);
        assert!(result.reasons.contains(&ReasonCode::SourceStale));
        assert!(result.reasons.contains(&ReasonCode::SourceEstimated));
        assert!(result.reasons.contains(&ReasonCode::ResetTimestampPassed));
    }
    proptest::proptest! {
        #[test]
        fn lower_remaining_never_reduces_pressure(a in 0u8..=100, b in 0u8..=100) {
            let higher = f64::from(a.max(b));
            let lower = f64::from(a.min(b));
            let config = PressureConfig::default();
            let high_result = assess_snapshot_at(
                &snapshot("weekly", Some(higher)),
                &config,
                OffsetDateTime::UNIX_EPOCH,
            );
            let low_result = assess_snapshot_at(
                &snapshot("weekly", Some(lower)),
                &config,
                OffsetDateTime::UNIX_EPOCH,
            );
            assert!(pressure_rank(&low_result.level) >= pressure_rank(&high_result.level));
        }
    }

    fn pressure_rank(level: &PressureLevel) -> u8 {
        match level {
            PressureLevel::Normal => 0,
            PressureLevel::Guarded => 1,
            PressureLevel::Conserve => 2,
            PressureLevel::Critical => 3,
            PressureLevel::Emergency => 4,
            PressureLevel::Blocked => 5,
            PressureLevel::Unknown => 0,
        }
    }
}
