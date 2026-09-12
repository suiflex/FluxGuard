use fluxguard_core::{
    advise, assess_combined, CombinedSnapshot, Confidence, ExecutionMode, OperationCost,
    OperationImportance, OperationKind, OperationProfile, PressureLevel, ProceedDecision,
    Recommendation,
};
use fluxguard_runtime::{SourceError, SourceStateKind};
use rmcp::{handler::server::wrapper::Parameters, schemars::JsonSchema, tool, tool_router, Json};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::server::FluxGuardServer;

pub(crate) fn build_tool_router() -> rmcp::handler::server::router::tool::ToolRouter<FluxGuardServer>
{
    FluxGuardServer::tool_router()
}
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct StatusRequest {
    pub detail: Option<String>,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BottleneckResponse {
    pub source: String,
    pub window: String,
    pub remaining_percent: Option<f64>,
    pub resets_at: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SourceSummaryResponse {
    pub source: String,
    pub state: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StatusResponse {
    pub pressure: String,
    pub recommended_mode: String,
    pub bottleneck: Option<BottleneckResponse>,
    pub confidence: String,
    pub freshness: String,
    pub sources: Option<Vec<SourceSummaryResponse>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AdviceRequest {
    pub operation: String,
    pub estimated_cost: String,
    pub importance: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct AdviceResponse {
    pub proceed: String,
    pub mode: String,
    pub pressure: String,
    pub reasons: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct RefreshRequest {
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RefreshFailure {
    pub source: String,
    pub error: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RefreshResponse {
    pub refreshed: Vec<String>,
    pub failed: Vec<RefreshFailure>,
    pub observed_at: String,
}

#[tool_router(router = tool_router)]
impl FluxGuardServer {
    #[tool(
        name = "resource_status",
        description = "Read the latest normalized FluxGuard resource pressure."
    )]
    pub async fn resource_status(
        &self,
        Parameters(request): Parameters<StatusRequest>,
    ) -> Result<Json<StatusResponse>, String> {
        let registry = self.registry.lock().await;
        let states = registry.states();
        let selected_states = filter_states(states, &request.sources)?;
        let snapshots = selected_states
            .iter()
            .filter_map(|state| state.snapshot.clone())
            .collect();
        let pressure = assess_combined(&CombinedSnapshot { snapshots }, &self.pressure_config);
        let freshness = bottleneck_freshness(&selected_states, &pressure);
        let bottleneck = pressure
            .bottleneck
            .as_ref()
            .map(|constraint| BottleneckResponse {
                source: constraint.source.as_str().into(),
                window: constraint.window.as_str().into(),
                remaining_percent: pressure.effective_remaining_percent,
                resets_at: pressure.resets_at.map(format_timestamp),
            });
        let summaries = (request.detail.as_deref() == Some("summary")).then(|| {
            selected_states
                .iter()
                .map(|state| SourceSummaryResponse {
                    source: state.descriptor.id.as_str().into(),
                    state: state_name(&state.status).into(),
                })
                .collect()
        });

        Ok(Json(StatusResponse {
            pressure: pressure_name(&pressure.level).into(),
            recommended_mode: mode_for_pressure(&pressure.level).into(),
            bottleneck,
            confidence: confidence_name(&pressure.confidence).into(),
            freshness,
            sources: summaries,
        }))
    }

    #[tool(
        name = "resource_advice",
        description = "Advise whether an intended operation fits current resource pressure."
    )]
    pub async fn resource_advice(
        &self,
        Parameters(request): Parameters<AdviceRequest>,
    ) -> Result<Json<AdviceResponse>, String> {
        let operation = OperationProfile {
            kind: parse_operation(&request.operation)?,
            estimated_cost: parse_cost(&request.estimated_cost)?,
            importance: parse_importance(&request.importance)?,
        };
        let registry = self.registry.lock().await;
        let snapshots = registry
            .states()
            .into_iter()
            .filter_map(|state| state.snapshot)
            .collect();
        let pressure = assess_combined(&CombinedSnapshot { snapshots }, &self.pressure_config);
        let advice = advise(&pressure, &operation);
        Ok(Json(AdviceResponse {
            proceed: proceed_name(&advice.proceed).into(),
            mode: mode_name(&advice.mode).into(),
            pressure: pressure_name(&advice.pressure.level).into(),
            reasons: advice
                .reasons
                .iter()
                .map(reason_name)
                .map(str::to_owned)
                .collect(),
            recommendations: advice
                .recommendations
                .iter()
                .map(recommendation_name)
                .map(str::to_owned)
                .collect(),
        }))
    }

    #[tool(
        name = "resource_refresh",
        description = "Refresh selected FluxGuard resource sources explicitly."
    )]
    pub async fn resource_refresh(
        &self,
        Parameters(request): Parameters<RefreshRequest>,
    ) -> Result<Json<RefreshResponse>, String> {
        let registry = self.registry.lock().await;
        let source_ids = if request.sources.is_empty() {
            registry
                .states()
                .into_iter()
                .map(|state| state.descriptor.id)
                .collect()
        } else {
            request
                .sources
                .iter()
                .map(|source| fluxguard_core::SourceId::new(source.clone()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "invalid_request".to_owned())?
        };
        let mut refreshed = Vec::new();
        let mut failed = Vec::new();
        for source in source_ids {
            match registry.refresh(&source).await {
                Ok(_) => refreshed.push(source.as_str().into()),
                Err(error) => failed.push(RefreshFailure {
                    source: source.as_str().into(),
                    error: source_error_name(&error).into(),
                }),
            }
        }
        Ok(Json(RefreshResponse {
            refreshed,
            failed,
            observed_at: format_timestamp(OffsetDateTime::now_utc()),
        }))
    }
}

fn filter_states(
    states: Vec<fluxguard_runtime::SourceState>,
    requested: &[String],
) -> Result<Vec<fluxguard_runtime::SourceState>, String> {
    if requested.is_empty() {
        return Ok(states);
    }
    if requested.iter().any(|source| source.trim().is_empty()) {
        return Err("invalid_request".into());
    }
    Ok(states
        .into_iter()
        .filter(|state| {
            requested
                .iter()
                .any(|source| source == state.descriptor.id.as_str())
        })
        .collect())
}

fn bottleneck_freshness(
    states: &[fluxguard_runtime::SourceState],
    pressure: &fluxguard_core::PressureAssessment,
) -> String {
    let Some(bottleneck) = &pressure.bottleneck else {
        return "unknown".into();
    };
    states
        .iter()
        .flat_map(|state| state.snapshot.as_ref())
        .flat_map(|snapshot| snapshot.windows.iter())
        .find(|window| {
            snapshot_source_id(window) == bottleneck.source.as_str()
                && window.id.as_str() == bottleneck.window.as_str()
        })
        .map(|window| match window.freshness {
            fluxguard_core::Freshness::Fresh => "fresh",
            fluxguard_core::Freshness::Aging => "aging",
            fluxguard_core::Freshness::Stale => "stale",
        })
        .unwrap_or("unknown")
        .into()
}

fn snapshot_source_id(window: &fluxguard_core::BudgetWindow) -> &str {
    window.provenance.source_id.as_str()
}

fn format_timestamp(timestamp: OffsetDateTime) -> String {
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| timestamp.unix_timestamp().to_string())
}

fn pressure_name(level: &PressureLevel) -> &'static str {
    match level {
        PressureLevel::Normal => "normal",
        PressureLevel::Guarded => "guarded",
        PressureLevel::Conserve => "conserve",
        PressureLevel::Critical => "critical",
        PressureLevel::Emergency => "emergency",
        PressureLevel::Blocked => "blocked",
        PressureLevel::Unknown => "unknown",
    }
}

fn mode_for_pressure(level: &PressureLevel) -> &'static str {
    match level {
        PressureLevel::Normal => "normal",
        PressureLevel::Guarded | PressureLevel::Conserve => "efficiency",
        PressureLevel::Critical | PressureLevel::Emergency => "completion_first",
        PressureLevel::Blocked => "blocked",
        PressureLevel::Unknown => "efficiency",
    }
}

fn confidence_name(confidence: &Confidence) -> &'static str {
    match confidence {
        Confidence::High => "high",
        Confidence::Medium => "medium",
        Confidence::Low => "low",
        Confidence::Unknown => "unknown",
    }
}

fn state_name(state: &SourceStateKind) -> &'static str {
    match state {
        SourceStateKind::Pending => "pending",
        SourceStateKind::Ready => "ready",
        SourceStateKind::Failed => "failed",
        SourceStateKind::Stale => "stale",
        SourceStateKind::Shutdown => "shutdown",
    }
}

fn parse_operation(value: &str) -> Result<OperationKind, String> {
    let result = match value {
        "inspect_targeted" => OperationKind::InspectTargeted,
        "search_broad" => OperationKind::SearchBroad,
        "edit_small" => OperationKind::EditSmall,
        "refactor_large" => OperationKind::RefactorLarge,
        "test_targeted" => OperationKind::TestTargeted,
        "test_full" => OperationKind::TestFull,
        "spawn_subagent" => OperationKind::SpawnSubagent,
        "spawn_parallel_subagents" => OperationKind::SpawnParallelSubagents,
        "research_external" => OperationKind::ResearchExternal,
        "generate_artifacts" => OperationKind::GenerateArtifacts,
        "checkpoint" => OperationKind::Checkpoint,
        "finalize" => OperationKind::Finalize,
        other if !other.trim().is_empty() => OperationKind::Other(other.into()),
        _ => return Err("invalid_operation".into()),
    };
    Ok(result)
}

fn parse_cost(value: &str) -> Result<OperationCost, String> {
    match value {
        "low" => Ok(OperationCost::Low),
        "medium" => Ok(OperationCost::Medium),
        "high" => Ok(OperationCost::High),
        "unknown" => Ok(OperationCost::Unknown),
        _ => Err("invalid_request".into()),
    }
}

fn parse_importance(value: &str) -> Result<OperationImportance, String> {
    match value {
        "required" => Ok(OperationImportance::Required),
        "useful" => Ok(OperationImportance::Useful),
        "optional" => Ok(OperationImportance::Optional),
        _ => Err("invalid_request".into()),
    }
}

fn proceed_name(decision: &ProceedDecision) -> &'static str {
    match decision {
        ProceedDecision::Yes => "yes",
        ProceedDecision::YesWithConstraints => "yes_with_constraints",
        ProceedDecision::No => "no",
        ProceedDecision::Unknown => "unknown",
    }
}

fn mode_name(mode: &ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Normal => "normal",
        ExecutionMode::Efficiency => "efficiency",
        ExecutionMode::CompletionFirst => "completion_first",
        ExecutionMode::CheckpointOnly => "checkpoint_only",
        ExecutionMode::Blocked => "blocked",
    }
}

fn reason_name(reason: &fluxguard_core::ReasonCode) -> &'static str {
    match reason {
        fluxguard_core::ReasonCode::WeeklyQuotaLow => "weekly_quota_low",
        fluxguard_core::ReasonCode::FiveHourQuotaLow => "five_hour_quota_low",
        fluxguard_core::ReasonCode::ContextLow => "context_low",
        fluxguard_core::ReasonCode::OperationCostHigh => "operation_cost_high",
        fluxguard_core::ReasonCode::OperationOptional => "operation_optional",
        fluxguard_core::ReasonCode::SourceStale => "source_stale",
        fluxguard_core::ReasonCode::SourceEstimated => "source_estimated",
        fluxguard_core::ReasonCode::AvailabilityBlocked => "availability_blocked",
        fluxguard_core::ReasonCode::AvailabilityUnknown => "availability_unknown",
        fluxguard_core::ReasonCode::UnknownConstraint => "unknown_constraint",
        fluxguard_core::ReasonCode::ResetTimestampPassed => "reset_timestamp_passed",
        fluxguard_core::ReasonCode::ResetImminent => "reset_imminent",
        fluxguard_core::ReasonCode::ResetSoon => "reset_soon",
        fluxguard_core::ReasonCode::ResetLater => "reset_later",
        _ => "other",
    }
}

fn recommendation_name(recommendation: &Recommendation) -> &'static str {
    match recommendation {
        Recommendation::AvoidParallelSubagents => "avoid_parallel_subagents",
        Recommendation::AvoidOptionalRefactor => "avoid_optional_refactor",
        Recommendation::UseTargetedTests => "use_targeted_tests",
        Recommendation::CheckpointAfterMilestone => "checkpoint_after_milestone",
        Recommendation::RefreshBeforeExpensiveWork => "refresh_before_expensive_work",
        Recommendation::ContinueSingleAgent => "continue_single_agent",
        Recommendation::PerformTargetedInspection => "perform_targeted_inspection",
        Recommendation::Other(_) => "other",
    }
}

fn source_error_name(error: &SourceError) -> &'static str {
    match error {
        SourceError::Unavailable => "source_unavailable",
        SourceError::Unauthenticated => "source_unauthenticated",
        SourceError::UnsupportedVersion => "source_unsupported",
        SourceError::Timeout => "source_timeout",
        SourceError::RateLimited => "source_rate_limited",
        SourceError::Protocol => "source_protocol_error",
        SourceError::InvalidPayload => "source_protocol_error",
        SourceError::ProcessExited => "source_unavailable",
        SourceError::PermissionDenied => "source_unavailable",
        SourceError::Other => "source_unavailable",
    }
}
