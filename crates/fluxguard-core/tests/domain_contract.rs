use fluxguard_core::{
    ExecutionMode, OperationCost, OperationImportance, OperationKind, OperationProfile,
    ProceedDecision, Recommendation,
};

#[test]
fn operation_profile_round_trips_with_stable_enum_names() {
    let profile = OperationProfile {
        kind: OperationKind::SpawnParallelSubagents,
        estimated_cost: OperationCost::High,
        importance: OperationImportance::Optional,
    };

    let encoded = serde_json::to_string(&profile).expect("serialize");
    assert_eq!(
        encoded,
        r#"{"kind":"spawn_parallel_subagents","estimated_cost":"high","importance":"optional"}"#
    );
    let decoded: OperationProfile = serde_json::from_str(&encoded).expect("deserialize");
    assert_eq!(decoded, profile);
}

#[test]
fn advice_contract_preserves_constraint_mode_and_recommendation() {
    let encoded = r#"{
        "proceed":"yes_with_constraints",
        "mode":"completion_first",
        "pressure":{
            "level":"critical",
            "bottleneck":null,
            "effective_remaining_percent":8.0,
            "resets_at":null,
            "confidence":"high",
            "reasons":["weekly_quota_low"]
        },
        "recommendations":["use_targeted_tests"],
        "reasons":["operation_cost_high"]
    }"#;

    let advice: fluxguard_core::ExecutionAdvice =
        serde_json::from_str(encoded).expect("deserialize");
    assert_eq!(advice.proceed, ProceedDecision::YesWithConstraints);
    assert_eq!(advice.mode, ExecutionMode::CompletionFirst);
    assert_eq!(
        advice.recommendations,
        vec![Recommendation::UseTargetedTests]
    );
}
