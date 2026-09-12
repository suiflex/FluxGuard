use std::collections::BTreeMap;
use std::time::Duration;

use ::time::{Duration as SignedDuration, OffsetDateTime};
use async_trait::async_trait;
use fluxguard_core::{
    Applicability, Availability, BlockReason, BudgetSnapshot, BudgetWindow, DecimalValue,
    Freshness, MetricDimension, Provenance, SnapshotWarning, SourceCapabilities, SourceDescriptor,
    SourceId, SourceKind, SourceQuality, WindowId,
};
use serde::Deserialize;
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout, Command},
    time,
};
use tokio_util::sync::CancellationToken;

use fluxguard_runtime::{BudgetSource, ProbeReport, ProbeState, SourceError, SourceState};
use tokio::sync::watch;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

/// Typed errors produced while normalizing or querying Codex App Server.
#[derive(Debug, Error)]
pub enum CodexError {
    #[error("invalid Codex payload")]
    InvalidPayload,
    #[error("Codex App Server protocol error")]
    Protocol,
    #[error("Codex App Server process unavailable")]
    Unavailable,
    #[error("Codex App Server request timed out")]
    Timeout,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexRateLimitsResponse {
    pub ordinary_usage_allowed: Option<bool>,
    pub rate_limits: Option<RateLimitSnapshot>,
    pub rate_limits_by_limit_id: Option<BTreeMap<String, RateLimitSnapshot>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitSnapshot {
    pub limit_id: Option<String>,
    pub limit_name: Option<String>,
    pub primary: Option<RateLimitWindow>,
    pub secondary: Option<RateLimitWindow>,
    pub individual_limit: Option<SpendControlLimitSnapshot>,
    pub spend_control_reached: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RateLimitWindow {
    pub used_percent: i32,
    pub resets_at: Option<i64>,
    pub window_duration_mins: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SpendControlLimitSnapshot {
    pub limit: String,
    pub remaining_percent: i32,
    pub resets_at: i64,
    pub used: String,
}

/// Configured local Codex App Server client.
#[derive(Clone, Debug)]
pub struct CodexAdapter {
    command: String,
    timeout: Duration,
    descriptor: SourceDescriptor,
}

impl CodexAdapter {
    pub fn new(command: impl Into<String>, timeout: Duration) -> Self {
        let id = SourceId::new("client.codex").expect("static source id is valid");
        Self {
            command: command.into(),
            timeout,
            descriptor: SourceDescriptor {
                id,
                kind: SourceKind::Client,
                display_name: "OpenAI Codex".into(),
                adapter_version: env!("CARGO_PKG_VERSION").into(),
                source_quality: SourceQuality::OfficialStructured,
                capabilities: SourceCapabilities {
                    supports_snapshot: true,
                    supports_push_updates: false,
                    supports_reset_time: true,
                    supports_exact_remaining_percent: true,
                    supports_model_scope: false,
                    supports_cost: true,
                },
            },
        }
    }

    pub fn with_default_timeout(command: impl Into<String>) -> Self {
        Self::new(command, DEFAULT_TIMEOUT)
    }

    pub fn normalize(response: CodexRateLimitsResponse) -> Result<BudgetSnapshot, CodexError> {
        normalize_response(response)
    }

    async fn fetch_rate_limits(&self) -> Result<CodexRateLimitsResponse, CodexError> {
        let mut child = Command::new(&self.command)
            .arg("app-server")
            .arg("--stdio")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| CodexError::Unavailable)?;

        let mut stdin = child.stdin.take().ok_or(CodexError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(CodexError::Unavailable)?;
        let mut stdout = BufReader::new(stdout);
        let stderr = child.stderr.take().ok_or(CodexError::Unavailable)?;
        let stderr_task = tokio::spawn(async move {
            let mut stderr = stderr;
            let mut discarded = Vec::new();
            let _ = stderr.read_to_end(&mut discarded).await;
        });

        let result = self.exchange(&mut stdin, &mut stdout).await;
        let _ = child.kill().await;
        let _ = child.wait().await;
        stderr_task.abort();
        result
    }

    async fn exchange(
        &self,
        stdin: &mut ChildStdin,
        stdout: &mut BufReader<ChildStdout>,
    ) -> Result<CodexRateLimitsResponse, CodexError> {
        write_rpc(
            stdin,
            1,
            "initialize",
            serde_json::json!({
                "clientInfo": {
                    "name": "fluxguard",
                    "title": "FluxGuard",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {}
            }),
        )
        .await?;
        let _: serde_json::Value = read_rpc(stdout, 1, self.timeout).await?;
        write_notification(stdin, "initialized", serde_json::json!({})).await?;
        write_rpc(stdin, 2, "account/rateLimits/read", serde_json::json!({})).await?;
        read_rpc(stdout, 2, self.timeout).await
    }
    async fn run_session(
        &self,
        updates: watch::Sender<SourceState>,
        cancel: CancellationToken,
    ) -> Result<(), SourceError> {
        let mut child = Command::new(&self.command)
            .arg("app-server")
            .arg("--stdio")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| SourceError::Unavailable)?;
        let mut stdin = child.stdin.take().ok_or(SourceError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(SourceError::Unavailable)?;
        let stderr = child.stderr.take().ok_or(SourceError::Unavailable)?;
        let stderr_task = tokio::spawn(async move {
            let mut stderr = stderr;
            let mut discarded = Vec::new();
            let _ = stderr.read_to_end(&mut discarded).await;
        });
        let mut stdout = BufReader::new(stdout);

        let initial = self
            .exchange(&mut stdin, &mut stdout)
            .await
            .map_err(map_error)?;
        updates
            .send(SourceState::ready(
                Self::normalize(initial).map_err(|_| SourceError::InvalidPayload)?,
            ))
            .map_err(|_| SourceError::Other)?;

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    stderr_task.abort();
                    return Ok(());
                }
                result = time::timeout(self.timeout, read_message(&mut stdout)) => {
                    let message = match result {
                        Ok(message) => message.map_err(map_error)?,
                        Err(_) => return Err(SourceError::Timeout),
                    };
                    if message.method.as_deref() == Some("account/rateLimits/updated") {
                        if let Some(params) = message.params {
                            let response = serde_json::from_value::<CodexRateLimitsResponse>(params)
                                .map_err(|_| SourceError::InvalidPayload)?;
                            let snapshot = Self::normalize(response)
                                .map_err(|_| SourceError::InvalidPayload)?;
                            updates
                                .send(SourceState::ready(snapshot))
                                .map_err(|_| SourceError::Other)?;
                        }
                    }
                }
            }
        }
    }
}

#[async_trait]
impl BudgetSource for CodexAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        let result = Command::new(&self.command)
            .arg("--version")
            .output()
            .await
            .map_err(|_| SourceError::Unavailable)?;
        if result.status.success() {
            Ok(ProbeReport {
                state: ProbeState::Ready,
            })
        } else {
            Ok(ProbeReport {
                state: ProbeState::Partial,
            })
        }
    }

    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError> {
        let response = self.fetch_rate_limits().await.map_err(map_error)?;
        Self::normalize(response).map_err(|_| SourceError::InvalidPayload)
    }

    async fn run(
        &self,
        updates: watch::Sender<SourceState>,
        cancel: CancellationToken,
    ) -> Result<(), SourceError> {
        let mut backoff = Duration::from_secs(1);
        loop {
            let result = self
                .run_session(updates.clone(), cancel.child_token())
                .await;
            if cancel.is_cancelled() {
                return Ok(());
            }
            if result.is_ok() {
                return Ok(());
            }
            time::sleep(backoff).await;
            backoff = std::cmp::min(backoff.saturating_mul(2), Duration::from_secs(60));
        }
    }
}

fn normalize_response(response: CodexRateLimitsResponse) -> Result<BudgetSnapshot, CodexError> {
    let source_id = SourceId::new("client.codex").map_err(|_| CodexError::InvalidPayload)?;
    let descriptor = SourceDescriptor {
        id: source_id.clone(),
        kind: SourceKind::Client,
        display_name: "OpenAI Codex".into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        source_quality: SourceQuality::OfficialStructured,
        capabilities: SourceCapabilities {
            supports_snapshot: true,
            supports_push_updates: false,
            supports_reset_time: true,
            supports_exact_remaining_percent: true,
            supports_model_scope: false,
            supports_cost: true,
        },
    };

    let now = OffsetDateTime::now_utc();
    let blocked = response.ordinary_usage_allowed == Some(false);
    let availability = if blocked {
        Availability::Blocked {
            reason: BlockReason::QuotaExhausted,
        }
    } else if response.ordinary_usage_allowed.is_none() {
        Availability::Unknown
    } else {
        Availability::Allowed
    };
    let mut windows = Vec::new();
    let mut warnings = Vec::new();
    let buckets = response
        .rate_limits_by_limit_id
        .filter(|buckets| !buckets.is_empty());
    if let Some(buckets) = buckets {
        for (key, bucket) in buckets {
            append_bucket(
                &mut NormalizeContext {
                    windows: &mut windows,
                    warnings: &mut warnings,
                    source_id: &source_id,
                    blocked,
                    now,
                },
                &key,
                &bucket,
            );
        }
    } else if let Some(bucket) = response.rate_limits {
        let key = bucket.limit_id.as_deref().unwrap_or("codex");
        append_bucket(
            &mut NormalizeContext {
                windows: &mut windows,
                warnings: &mut warnings,
                source_id: &source_id,
                blocked,
                now,
            },
            key,
            &bucket,
        );
    }

    Ok(BudgetSnapshot {
        source: descriptor,
        account_scope: None,
        availability,
        windows,
        observed_at: now,
        warnings,
    })
}

struct NormalizeContext<'a> {
    windows: &'a mut Vec<BudgetWindow>,
    warnings: &'a mut Vec<SnapshotWarning>,
    source_id: &'a SourceId,
    blocked: bool,
    now: OffsetDateTime,
}
fn append_bucket(context: &mut NormalizeContext<'_>, key: &str, bucket: &RateLimitSnapshot) {
    if let Some(primary) = &bucket.primary {
        append_window(context, key, "primary", primary);
    }
    if let Some(secondary) = &bucket.secondary {
        append_window(context, key, "secondary", secondary);
    }
    if let Some(spend) = &bucket.individual_limit {
        let used = parse_decimal(&spend.used, context.warnings, "spend_used");
        let limit = parse_decimal(&spend.limit, context.warnings, "spend_limit");
        let resets_at = timestamp(spend.resets_at, context.warnings, "spend_reset");
        if let Ok(id) = WindowId::new(format!("{key}.individual_limit")) {
            context.windows.push(BudgetWindow {
                id,
                label: Some("Individual spend limit".into()),
                dimension: MetricDimension::Currency,
                used,
                limit,
                remaining: None,
                used_percent: None,
                remaining_percent: Some(clamp_percent(spend.remaining_percent)),
                window_duration_seconds: None,
                resets_at,
                fresh_until: Some(context.now + SignedDuration::minutes(1)),
                hard_blocked: context.blocked || bucket.spend_control_reached == Some(true),
                applicability: Applicability::Applicable,
                observed_at: context.now,
                freshness: Freshness::Fresh,
                provenance: Provenance {
                    source_id: context.source_id.clone(),
                    source_quality: SourceQuality::OfficialStructured,
                    observed_via: Some("codex_app_server".into()),
                },
            });
        }
    }
}

fn append_window(
    context: &mut NormalizeContext<'_>,
    key: &str,
    kind: &str,
    upstream: &RateLimitWindow,
) {
    let Ok(id) = WindowId::new(format!("{key}.{kind}")) else {
        return;
    };
    let used_percent = clamp_percent(upstream.used_percent);
    let resets_at = upstream
        .resets_at
        .and_then(|value| timestamp(value, context.warnings, "window_reset"));
    context.windows.push(BudgetWindow {
        id,
        label: Some(format!("{key} {kind}")),
        dimension: MetricDimension::Requests,
        used: None,
        limit: None,
        remaining: None,
        used_percent: Some(used_percent),
        remaining_percent: Some(100.0 - used_percent),
        window_duration_seconds: upstream
            .window_duration_mins
            .and_then(|minutes| u64::try_from(minutes).ok())
            .map(|minutes| minutes.saturating_mul(60)),
        resets_at,
        fresh_until: Some(context.now + SignedDuration::minutes(1)),
        hard_blocked: context.blocked,
        applicability: Applicability::Applicable,
        observed_at: context.now,
        freshness: Freshness::Fresh,
        provenance: Provenance {
            source_id: context.source_id.clone(),
            source_quality: SourceQuality::OfficialStructured,
            observed_via: Some("codex_app_server".into()),
        },
    });
}

fn clamp_percent(value: i32) -> f64 {
    f64::from(value).clamp(0.0, 100.0)
}

fn parse_decimal(
    value: &str,
    warnings: &mut Vec<SnapshotWarning>,
    field: &str,
) -> Option<DecimalValue> {
    match value.parse::<f64>() {
        Ok(value) => DecimalValue::try_new(value).ok(),
        Err(_) => {
            warnings.push(SnapshotWarning {
                code: "invalid_decimal".into(),
                message: field.into(),
            });
            None
        }
    }
}

fn timestamp(
    value: i64,
    warnings: &mut Vec<SnapshotWarning>,
    field: &str,
) -> Option<OffsetDateTime> {
    match OffsetDateTime::from_unix_timestamp(value) {
        Ok(value) => Some(value),
        Err(_) => {
            warnings.push(SnapshotWarning {
                code: "invalid_timestamp".into(),
                message: field.into(),
            });
            None
        }
    }
}

fn map_error(error: CodexError) -> SourceError {
    match error {
        CodexError::InvalidPayload => SourceError::InvalidPayload,
        CodexError::Protocol => SourceError::Protocol,
        CodexError::Unavailable => SourceError::Unavailable,
        CodexError::Timeout => SourceError::Timeout,
    }
}

async fn write_rpc(
    stdin: &mut ChildStdin,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> Result<(), CodexError> {
    let message = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    write_message(stdin, &message).await
}

async fn write_notification(
    stdin: &mut ChildStdin,
    method: &str,
    params: serde_json::Value,
) -> Result<(), CodexError> {
    let message = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });
    write_message(stdin, &message).await
}

async fn write_message(
    stdin: &mut ChildStdin,
    message: &serde_json::Value,
) -> Result<(), CodexError> {
    let mut encoded = serde_json::to_vec(message).map_err(|_| CodexError::Protocol)?;
    encoded.push(b'\n');
    if encoded.len() > MAX_MESSAGE_BYTES {
        return Err(CodexError::InvalidPayload);
    }
    stdin
        .write_all(&encoded)
        .await
        .map_err(|_| CodexError::Unavailable)
}

async fn read_rpc<T: for<'de> Deserialize<'de>>(
    stdout: &mut BufReader<ChildStdout>,
    expected_id: u64,
    timeout: Duration,
) -> Result<T, CodexError> {
    loop {
        let message = time::timeout(timeout, read_message(stdout))
            .await
            .map_err(|_| CodexError::Timeout)??;
        if message.id != Some(expected_id) {
            continue;
        }
        if message.error.is_some() {
            return Err(CodexError::Protocol);
        }
        return message
            .result
            .ok_or(CodexError::Protocol)
            .and_then(|result| {
                serde_json::from_value(result).map_err(|_| CodexError::InvalidPayload)
            });
    }
}

async fn read_message(
    stdout: &mut BufReader<ChildStdout>,
) -> Result<RpcEnvelope<serde_json::Value>, CodexError> {
    let mut line = String::new();
    let read = stdout
        .read_line(&mut line)
        .await
        .map_err(|_| CodexError::Unavailable)?;
    if read == 0 || line.len() > MAX_MESSAGE_BYTES {
        return Err(CodexError::InvalidPayload);
    }
    serde_json::from_str(&line).map_err(|_| CodexError::InvalidPayload)
}

#[derive(Deserialize)]
struct RpcEnvelope<T> {
    id: Option<u64>,
    method: Option<String>,
    params: Option<serde_json::Value>,
    result: Option<T>,
    error: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use fluxguard_runtime::{SourceRegistry, SourceStateKind};

    #[test]
    fn basic_fixture_maps_primary_and_secondary_windows() {
        let response: CodexRateLimitsResponse = serde_json::from_str(include_str!(
            "../../../tests/fixtures/codex/rate_limits_basic.json"
        ))
        .expect("fixture");
        let snapshot = CodexAdapter::normalize(response).expect("normalize");

        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].remaining_percent, Some(80.0));
        assert_eq!(snapshot.windows[1].remaining_percent, Some(50.0));
    }

    #[test]
    fn multi_bucket_fixture_preserves_each_limit_id() {
        let response: CodexRateLimitsResponse = serde_json::from_str(include_str!(
            "../../../tests/fixtures/codex/rate_limits_multi_bucket.json"
        ))
        .expect("fixture");
        let snapshot = CodexAdapter::normalize(response).expect("normalize");

        assert_eq!(snapshot.windows.len(), 2);
        assert!(snapshot
            .windows
            .iter()
            .any(|window| window.id.as_str() == "codex.primary"));
        assert!(snapshot
            .windows
            .iter()
            .any(|window| window.id.as_str() == "gpt-5.primary"));
    }

    #[test]
    fn blocked_fixture_preserves_blocked_availability_without_timestamp_recovery() {
        let response: CodexRateLimitsResponse = serde_json::from_str(include_str!(
            "../../../tests/fixtures/codex/rate_limits_blocked.json"
        ))
        .expect("fixture");
        let snapshot = CodexAdapter::normalize(response).expect("normalize");

        assert!(matches!(
            snapshot.availability,
            Availability::Blocked { .. }
        ));
        assert!(snapshot.windows[0].hard_blocked);
    }

    #[test]
    fn missing_optional_fields_are_accepted() {
        let response: CodexRateLimitsResponse = serde_json::from_str(include_str!(
            "../../../tests/fixtures/codex/rate_limits_missing_optional.json"
        ))
        .expect("fixture");
        let snapshot = CodexAdapter::normalize(response).expect("normalize");

        assert!(snapshot.windows.is_empty());
        assert!(matches!(snapshot.availability, Availability::Unknown));
    }
    #[test]
    fn fixtures_contain_no_auth_material() {
        for fixture in [
            include_str!("../../../tests/fixtures/codex/rate_limits_basic.json"),
            include_str!("../../../tests/fixtures/codex/rate_limits_multi_bucket.json"),
            include_str!("../../../tests/fixtures/codex/rate_limits_blocked.json"),
            include_str!("../../../tests/fixtures/codex/rate_limits_missing_optional.json"),
        ] {
            let lower = fixture.to_ascii_lowercase();
            assert!(!lower.contains("bearer"));
            assert!(!lower.contains("cookie"));
            assert!(!lower.contains("access_token"));
            assert!(!lower.contains("refresh_token"));
        }
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn app_server_process_crash_is_recovered_with_bounded_restart() {
        use std::{
            fs,
            os::unix::fs::PermissionsExt,
            time::{SystemTime, UNIX_EPOCH},
        };

        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("fluxguard-codex-{suffix}"));
        fs::create_dir_all(&root).expect("create test directory");
        let script_path = root.join("codex");
        let counter_path = root.join("starts");
        let counter_line = format!("COUNT_FILE={}", counter_path.display());
        let script = [
            "#!/bin/sh",
            counter_line.as_str(),
            "if [ -f \"$COUNT_FILE\" ]; then n=$(cat \"$COUNT_FILE\"); else n=0; fi",
            "n=$((n + 1))",
            "printf '%s' \"$n\" > \"$COUNT_FILE\"",
            "if [ \"$n\" -eq 1 ]; then exit 1; fi",
            "IFS= read -r line",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}'",
            "IFS= read -r line",
            "printf '%s\\n' '{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{\"ordinaryUsageAllowed\":true,\"rateLimits\":null}}'",
            "sleep 10",
        ]
        .join("\n");
        fs::write(&script_path, script).expect("write test process");
        let mut permissions = fs::metadata(&script_path)
            .expect("script metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("make executable");

        let adapter = Arc::new(CodexAdapter::new(
            script_path.to_string_lossy().into_owned(),
            Duration::from_secs(2),
        ));
        let id = adapter.id();
        let mut registry = SourceRegistry::new(Duration::from_secs(2));
        registry.register(adapter).expect("register");
        let mut state = registry.subscribe(&id).expect("subscribe");
        registry.start();
        tokio::time::timeout(Duration::from_secs(5), state.changed())
            .await
            .expect("restart completed")
            .expect("state channel");
        assert_eq!(state.borrow().status, SourceStateKind::Ready);
        registry.shutdown().await;
        let _ = fs::remove_dir_all(root);
    }
}
