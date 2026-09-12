use serde::{Deserialize, Serialize};

/// Type of work an agent intends to perform.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    InspectTargeted,
    SearchBroad,
    EditSmall,
    RefactorLarge,
    TestTargeted,
    TestFull,
    SpawnSubagent,
    SpawnParallelSubagents,
    ResearchExternal,
    GenerateArtifacts,
    Checkpoint,
    Finalize,
    Other(String),
}

/// Heuristic resource cost class for an operation.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationCost {
    Low,
    Medium,
    High,
    Unknown,
}

/// Estimate an operation's resource cost without claiming billing precision.
pub fn estimate_operation_cost(kind: &OperationKind) -> OperationCost {
    match kind {
        OperationKind::InspectTargeted
        | OperationKind::EditSmall
        | OperationKind::TestTargeted
        | OperationKind::Checkpoint
        | OperationKind::Finalize => OperationCost::Low,
        OperationKind::SearchBroad
        | OperationKind::ResearchExternal
        | OperationKind::GenerateArtifacts
        | OperationKind::TestFull
        | OperationKind::SpawnSubagent => OperationCost::Medium,
        OperationKind::RefactorLarge | OperationKind::SpawnParallelSubagents => OperationCost::High,
        OperationKind::Other(_) => OperationCost::Unknown,
    }
}

/// Whether the operation is correctness-critical or optional.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationImportance {
    Required,
    Useful,
    Optional,
}

/// Input profile consumed by the policy engine.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct OperationProfile {
    pub kind: OperationKind,
    pub estimated_cost: OperationCost,
    pub importance: OperationImportance,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimator_matches_documented_operation_cost_classes() {
        assert_eq!(
            estimate_operation_cost(&OperationKind::SpawnParallelSubagents),
            OperationCost::High
        );
        assert_eq!(
            estimate_operation_cost(&OperationKind::TestTargeted),
            OperationCost::Low
        );
        assert_eq!(
            estimate_operation_cost(&OperationKind::Other("custom".into())),
            OperationCost::Unknown
        );
    }
}
