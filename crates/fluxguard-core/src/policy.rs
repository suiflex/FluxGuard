use serde::{Deserialize, Serialize};

use crate::{
    Confidence, OperationCost, OperationImportance, OperationKind, OperationProfile,
    PressureAssessment, PressureLevel, ReasonCode,
};

/// Whether the caller should proceed with the requested operation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProceedDecision {
    Yes,
    YesWithConstraints,
    No,
    Unknown,
}

/// Execution strategy suggested by current resource pressure.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Normal,
    Efficiency,
    CompletionFirst,
    CheckpointOnly,
    Blocked,
}

/// A concrete, machine-readable execution recommendation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Recommendation {
    AvoidParallelSubagents,
    AvoidOptionalRefactor,
    UseTargetedTests,
    CheckpointAfterMilestone,
    RefreshBeforeExpensiveWork,
    ContinueSingleAgent,
    PerformTargetedInspection,
    Other(String),
}

/// Advice for an agent operation under the assessed constraints.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExecutionAdvice {
    pub proceed: ProceedDecision,
    pub mode: ExecutionMode,
    pub pressure: PressureAssessment,
    pub recommendations: Vec<Recommendation>,
    pub reasons: Vec<ReasonCode>,
}

/// Convert a pressure assessment and operation profile into execution advice.
pub fn advise(pressure: &PressureAssessment, operation: &OperationProfile) -> ExecutionAdvice {
    let expensive = matches!(
        operation.estimated_cost,
        OperationCost::Medium | OperationCost::High
    );
    let optional = operation.importance == OperationImportance::Optional;
    let constrained = matches!(
        pressure.level,
        PressureLevel::Guarded
            | PressureLevel::Conserve
            | PressureLevel::Critical
            | PressureLevel::Emergency
    );

    let mut reasons = pressure.reasons.clone();
    if expensive {
        reasons.push(ReasonCode::OperationCostHigh);
    }
    if optional {
        reasons.push(ReasonCode::OperationOptional);
    }

    let mut recommendations = Vec::new();
    if matches!(operation.kind, OperationKind::SpawnParallelSubagents) && constrained {
        recommendations.push(Recommendation::AvoidParallelSubagents);
        reasons.push(ReasonCode::AvoidParallelSubagents);
    }
    if matches!(operation.kind, OperationKind::RefactorLarge) && constrained {
        recommendations.push(Recommendation::AvoidOptionalRefactor);
        reasons.push(ReasonCode::AvoidOptionalRefactor);
    }
    if matches!(operation.kind, OperationKind::TestFull)
        && matches!(
            pressure.level,
            PressureLevel::Conserve | PressureLevel::Critical | PressureLevel::Emergency
        )
    {
        recommendations.push(Recommendation::UseTargetedTests);
        reasons.push(ReasonCode::UseTargetedTests);
    }
    if matches!(
        pressure.level,
        PressureLevel::Conserve | PressureLevel::Critical | PressureLevel::Emergency
    ) {
        recommendations.push(Recommendation::CheckpointAfterMilestone);
        reasons.push(ReasonCode::CheckpointAfterMilestone);
    }
    if pressure.confidence == Confidence::Low && expensive {
        recommendations.push(Recommendation::RefreshBeforeExpensiveWork);
        reasons.push(ReasonCode::RefreshBeforeExpensiveWork);
    }

    let (proceed, mode) = match pressure.level {
        PressureLevel::Normal => (ProceedDecision::Yes, ExecutionMode::Normal),
        PressureLevel::Guarded => (
            ProceedDecision::YesWithConstraints,
            ExecutionMode::Efficiency,
        ),
        PressureLevel::Conserve => (
            ProceedDecision::YesWithConstraints,
            ExecutionMode::Efficiency,
        ),
        PressureLevel::Critical | PressureLevel::Emergency => {
            if optional && expensive {
                (ProceedDecision::No, ExecutionMode::CompletionFirst)
            } else {
                (
                    ProceedDecision::YesWithConstraints,
                    ExecutionMode::CompletionFirst,
                )
            }
        }
        PressureLevel::Blocked => (ProceedDecision::No, ExecutionMode::Blocked),
        PressureLevel::Unknown => (ProceedDecision::Unknown, ExecutionMode::Efficiency),
    };

    ExecutionAdvice {
        proceed,
        mode,
        pressure: pressure.clone(),
        recommendations,
        reasons,
    }
}

/// Named policy behavior for different execution preferences.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyProfile {
    #[default]
    Balanced,
    Conservative,
    CompletionFirst,
}

/// Policy configuration passed by an application service.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct PolicyConfig {
    pub profile: PolicyProfile,
}

/// Apply a named profile to the baseline policy advice.
pub fn advise_with_profile(
    pressure: &PressureAssessment,
    operation: &OperationProfile,
    config: &PolicyConfig,
) -> ExecutionAdvice {
    let mut advice = advise(pressure, operation);
    match config.profile {
        PolicyProfile::Balanced => {}
        PolicyProfile::Conservative => {
            if advice.proceed == ProceedDecision::Yes
                && operation.importance != OperationImportance::Required
            {
                advice.proceed = ProceedDecision::YesWithConstraints;
                advice.mode = ExecutionMode::Efficiency;
            }
        }
        PolicyProfile::CompletionFirst => {
            if !matches!(advice.mode, ExecutionMode::Blocked) {
                advice.mode = ExecutionMode::CompletionFirst;
            }
        }
    }
    advice
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pressure(level: PressureLevel) -> PressureAssessment {
        PressureAssessment {
            level,
            bottleneck: None,
            effective_remaining_percent: None,
            resets_at: None,
            confidence: Confidence::High,
            reasons: Vec::new(),
        }
    }

    fn operation(
        kind: OperationKind,
        cost: OperationCost,
        importance: OperationImportance,
    ) -> OperationProfile {
        OperationProfile {
            kind,
            estimated_cost: cost,
            importance,
        }
    }

    #[test]
    fn policy_table_respects_pressure_and_operation_importance() {
        let cases = [
            (
                PressureLevel::Normal,
                OperationCost::High,
                OperationImportance::Optional,
                ProceedDecision::Yes,
                ExecutionMode::Normal,
            ),
            (
                PressureLevel::Critical,
                OperationCost::High,
                OperationImportance::Optional,
                ProceedDecision::No,
                ExecutionMode::CompletionFirst,
            ),
            (
                PressureLevel::Emergency,
                OperationCost::High,
                OperationImportance::Required,
                ProceedDecision::YesWithConstraints,
                ExecutionMode::CompletionFirst,
            ),
            (
                PressureLevel::Blocked,
                OperationCost::Low,
                OperationImportance::Required,
                ProceedDecision::No,
                ExecutionMode::Blocked,
            ),
        ];

        for (level, cost, importance, expected_proceed, expected_mode) in cases {
            let advice = advise(
                &pressure(level),
                &operation(OperationKind::EditSmall, cost, importance),
            );
            assert_eq!(advice.proceed, expected_proceed);
            assert_eq!(advice.mode, expected_mode);
        }
    }

    #[test]
    fn expensive_parallel_work_gets_targeted_recommendations() {
        let mut current = pressure(PressureLevel::Critical);
        current.confidence = Confidence::Low;
        let advice = advise(
            &current,
            &operation(
                OperationKind::SpawnParallelSubagents,
                OperationCost::High,
                OperationImportance::Optional,
            ),
        );

        assert_eq!(advice.proceed, ProceedDecision::No);
        assert!(advice
            .recommendations
            .contains(&Recommendation::AvoidParallelSubagents));
        assert!(advice
            .recommendations
            .contains(&Recommendation::RefreshBeforeExpensiveWork));
    }
    #[test]
    fn completion_first_profile_changes_only_execution_mode() {
        let advice = advise_with_profile(
            &pressure(PressureLevel::Normal),
            &operation(
                OperationKind::EditSmall,
                OperationCost::Low,
                OperationImportance::Required,
            ),
            &PolicyConfig {
                profile: PolicyProfile::CompletionFirst,
            },
        );
        assert_eq!(advice.proceed, ProceedDecision::Yes);
        assert_eq!(advice.mode, ExecutionMode::CompletionFirst);
    }
}
