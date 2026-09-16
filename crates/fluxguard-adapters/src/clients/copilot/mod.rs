use std::{collections::BTreeMap, process::Stdio, time::Duration};

use ::time::{format_description::well_known::Rfc3339, Duration as SignedDuration, OffsetDateTime};
use async_trait::async_trait;
use fluxguard_core::{
    Applicability, Availability, BudgetSnapshot, BudgetWindow, DecimalValue, Freshness,
    MetricDimension, Provenance, SnapshotWarning, SourceCapabilities, SourceDescriptor, SourceKind,
    SourceQuality, WindowId,
};
use fluxguard_runtime::{BudgetSource, ProbeReport, ProbeState, SourceError};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use crate::support;

const MAX_MESSAGE_BYTES: usize = 1024 * 1024;
const SOURCE_ID: &str = "client.copilot";
const JSON_RPC_METHOD_NOT_FOUND: i64 = -32601;

/// GitHub Copilot quota adapter.
///
/// Talks to the Copilot CLI in headless server mode (`copilot --headless --stdio`)
/// and calls the documented `account.getQuota` JSON-RPC method, the same surface
/// the official Copilot SDKs use.
#[derive(Clone, Debug)]
pub struct CopilotAdapter {
    command: String,
    descriptor: SourceDescriptor,
    timeout: Duration,
}

/// `account.getQuota` result: snapshots keyed by quota type
/// (`premium_interactions`, `chat`, `completions`, ...).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotQuotaResult {
    #[serde(default)]
    pub quota_snapshots: BTreeMap<String, Option<CopilotQuotaSnapshot>>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotQuotaSnapshot {
    #[serde(default)]
    pub is_unlimited_entitlement: bool,
    /// Requests included in the entitlement, or `-1` for unlimited.
    #[serde(default)]
    pub entitlement_requests: Option<f64>,
    #[serde(default)]
    pub used_requests: Option<f64>,
    #[serde(default)]
    pub usage_allowed_with_exhausted_quota: bool,
    #[serde(default)]
    pub remaining_percentage: Option<f64>,
    #[serde(default)]
    pub overage: Option<f64>,
    #[serde(default)]
    pub overage_allowed_with_exhausted_quota: bool,
    /// ISO 8601 reset date.
    #[serde(default)]
    pub reset_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RpcEnvelope {
    #[serde(default)]
    id: Option<serde_json::Value>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    result: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

fn descriptor() -> SourceDescriptor {
    support::descriptor(
        SOURCE_ID,
        SourceKind::Client,
        "GitHub Copilot",
        SourceQuality::OfficialStructured,
        SourceCapabilities {
            supports_snapshot: true,
            supports_push_updates: false,
            supports_reset_time: true,
            supports_exact_remaining_percent: true,
            supports_model_scope: false,
            supports_cost: false,
        },
    )
}

impl CopilotAdapter {
    pub fn new(command: impl Into<String>, timeout: Duration) -> Self {
        Self {
            command: command.into(),
            descriptor: descriptor(),
            timeout,
        }
    }

    pub fn with_default_timeout(command: impl Into<String>) -> Self {
        Self::new(command, Duration::from_secs(10))
    }

    pub fn normalize(result: CopilotQuotaResult) -> Result<BudgetSnapshot, SourceError> {
        let descriptor = descriptor();
        let source_id = descriptor.id.clone();
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();
        let mut warnings = Vec::new();

        for (key, snapshot) in result.quota_snapshots {
            let Some(snapshot) = snapshot else { continue };
            let window_id = WindowId::new(format!("{SOURCE_ID}.{key}"))
                .map_err(|_| SourceError::InvalidPayload)?;

            let unlimited = snapshot.is_unlimited_entitlement
                || snapshot.entitlement_requests.is_some_and(|v| v < 0.0);
            let used = snapshot
                .used_requests
                .and_then(|v| DecimalValue::try_new(v).ok());
            let limit = if unlimited {
                None
            } else {
                snapshot
                    .entitlement_requests
                    .and_then(|v| DecimalValue::try_new(v).ok())
            };
            let remaining = match (limit, used) {
                (Some(limit), Some(used)) => {
                    DecimalValue::try_new((limit.0 - used.0).max(0.0)).ok()
                }
                _ => None,
            };
            let remaining_percent = if unlimited {
                None
            } else {
                snapshot
                    .remaining_percentage
                    .filter(|v| v.is_finite())
                    .or_else(|| match (limit, remaining) {
                        (Some(limit), Some(rem)) if limit.0 > 0.0 => {
                            Some((rem.0 / limit.0) * 100.0)
                        }
                        _ => None,
                    })
                    .map(|v| v.clamp(0.0, 100.0))
            };

            let resets_at = match snapshot.reset_date.as_deref() {
                None => None,
                Some(raw) => match OffsetDateTime::parse(raw, &Rfc3339) {
                    Ok(ts) => Some(ts),
                    Err(_) => {
                        warnings.push(SnapshotWarning {
                            code: "invalid_reset_date".into(),
                            message: key.clone(),
                        });
                        None
                    }
                },
            };

            let exhausted = remaining_percent.is_some_and(|v| v <= 0.0)
                || remaining.is_some_and(|v| v.0 <= 0.0);
            let hard_blocked = !unlimited
                && exhausted
                && !snapshot.usage_allowed_with_exhausted_quota
                && !snapshot.overage_allowed_with_exhausted_quota;

            windows.push(BudgetWindow {
                id: window_id,
                label: Some(label_for(&key)),
                dimension: MetricDimension::Requests,
                used,
                limit,
                remaining,
                used_percent: remaining_percent.map(|rem| 100.0 - rem),
                remaining_percent,
                window_duration_seconds: None,
                resets_at,
                fresh_until: Some(now + SignedDuration::seconds(60)),
                hard_blocked,
                applicability: if unlimited {
                    Applicability::NotApplicable
                } else {
                    Applicability::Applicable
                },
                observed_at: now,
                freshness: Freshness::Fresh,
                provenance: Provenance {
                    source_id: source_id.clone(),
                    source_quality: SourceQuality::OfficialStructured,
                    observed_via: Some("copilot_cli_account_get_quota".into()),
                },
            });
        }

        if windows.is_empty() {
            warnings.push(SnapshotWarning {
                code: "copilot_quota_missing".into(),
                message: "Copilot returned no quota snapshots".into(),
            });
        }

        Ok(BudgetSnapshot {
            source: descriptor,
            account_scope: None,
            availability: if windows.is_empty() {
                Availability::Unknown
            } else {
                Availability::Allowed
            },
            windows,
            observed_at: now,
            warnings,
        })
    }

    async fn read_quota(&self) -> Result<CopilotQuotaResult, SourceError> {
        let mut child = Command::new(&self.command)
            .args(["--headless", "--stdio", "--no-auto-update"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr may echo auth details; never read it.
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| SourceError::Unavailable)?;

        let mut stdin = child.stdin.take().ok_or(SourceError::Unavailable)?;
        let mut stdout = BufReader::new(child.stdout.take().ok_or(SourceError::Unavailable)?);

        let result = self.exchange(&mut stdin, &mut stdout).await;
        let _ = child.kill().await;
        let _ = child.wait().await;
        result
    }

    async fn exchange(
        &self,
        stdin: &mut ChildStdin,
        stdout: &mut BufReader<ChildStdout>,
    ) -> Result<CopilotQuotaResult, SourceError> {
        write_request(stdin, 1, "connect", serde_json::json!({})).await?;
        match read_response(stdin, stdout, 1, self.timeout).await {
            Ok(_) => {}
            // Older CLI builds predate `connect`; the SDK falls back the same way.
            Err(RpcFailure::Remote(err)) if err.code == JSON_RPC_METHOD_NOT_FOUND => {}
            Err(RpcFailure::Remote(_)) => return Err(SourceError::Protocol),
            Err(RpcFailure::Source(err)) => return Err(err),
        }

        write_request(stdin, 2, "account.getQuota", serde_json::json!({})).await?;
        match read_response(stdin, stdout, 2, self.timeout).await {
            Ok(value) => serde_json::from_value(value).map_err(|_| SourceError::InvalidPayload),
            Err(RpcFailure::Remote(err)) => Err(classify_remote_error(&err)),
            Err(RpcFailure::Source(err)) => Err(err),
        }
    }
}

fn label_for(key: &str) -> String {
    key.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn classify_remote_error(err: &RpcError) -> SourceError {
    let message = err.message.to_ascii_lowercase();
    if ["auth", "login", "token", "sign in", "signed in"]
        .iter()
        .any(|needle| message.contains(needle))
    {
        SourceError::Unauthenticated
    } else {
        SourceError::Protocol
    }
}

enum RpcFailure {
    Remote(RpcError),
    Source(SourceError),
}

impl From<SourceError> for RpcFailure {
    fn from(err: SourceError) -> Self {
        Self::Source(err)
    }
}

async fn write_request(
    stdin: &mut ChildStdin,
    id: u64,
    method: &str,
    params: serde_json::Value,
) -> Result<(), SourceError> {
    write_frame(
        stdin,
        &serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}),
    )
    .await
}

/// vscode-jsonrpc framing: `Content-Length: N\r\n\r\n<body>`.
async fn write_frame(
    stdin: &mut ChildStdin,
    message: &serde_json::Value,
) -> Result<(), SourceError> {
    let body = serde_json::to_vec(message).map_err(|_| SourceError::Protocol)?;
    if body.len() > MAX_MESSAGE_BYTES {
        return Err(SourceError::InvalidPayload);
    }
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin
        .write_all(header.as_bytes())
        .await
        .map_err(|_| SourceError::Unavailable)?;
    stdin
        .write_all(&body)
        .await
        .map_err(|_| SourceError::Unavailable)?;
    stdin.flush().await.map_err(|_| SourceError::Unavailable)
}

async fn read_frame(stdout: &mut BufReader<ChildStdout>) -> Result<RpcEnvelope, SourceError> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let read = stdout
            .read_line(&mut line)
            .await
            .map_err(|_| SourceError::Unavailable)?;
        if read == 0 {
            return Err(SourceError::ProcessExited);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(value) = line
            .split_once(':')
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map(|(_, value)| value.trim())
        {
            content_length = Some(value.parse().map_err(|_| SourceError::Protocol)?);
        }
    }
    let length = content_length.ok_or(SourceError::Protocol)?;
    if length > MAX_MESSAGE_BYTES {
        return Err(SourceError::InvalidPayload);
    }
    let mut body = vec![0u8; length];
    stdout
        .read_exact(&mut body)
        .await
        .map_err(|_| SourceError::ProcessExited)?;
    serde_json::from_slice(&body).map_err(|_| SourceError::Protocol)
}

async fn read_response(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    expected_id: u64,
    timeout: Duration,
) -> Result<serde_json::Value, RpcFailure> {
    let expected = serde_json::json!(expected_id);
    loop {
        let message = tokio::time::timeout(timeout, read_frame(stdout))
            .await
            .map_err(|_| SourceError::Timeout)??;
        // Server-to-client requests (e.g. token acquisition) get a polite refusal
        // so the CLI does not hang waiting on us.
        if let (Some(id), Some(_)) = (&message.id, &message.method) {
            write_frame(
                stdin,
                &serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": JSON_RPC_METHOD_NOT_FOUND, "message": "unsupported"}
                }),
            )
            .await?;
            continue;
        }
        if message.id.as_ref() != Some(&expected) {
            continue;
        }
        if let Some(err) = message.error {
            return Err(RpcFailure::Remote(err));
        }
        return message
            .result
            .ok_or(RpcFailure::Source(SourceError::Protocol));
    }
}

#[async_trait]
impl BudgetSource for CopilotAdapter {
    fn descriptor(&self) -> SourceDescriptor {
        self.descriptor.clone()
    }

    async fn probe(&self) -> Result<ProbeReport, SourceError> {
        let output = Command::new(&self.command)
            .args(["--version"])
            .stdin(Stdio::null())
            .output()
            .await;

        match output {
            Ok(out) if out.status.success() => Ok(ProbeReport {
                state: ProbeState::Ready,
            }),
            Ok(_) => Ok(ProbeReport {
                state: ProbeState::UnsupportedVersion,
            }),
            Err(_) => Ok(ProbeReport {
                state: ProbeState::BinaryMissing,
            }),
        }
    }

    async fn refresh(&self) -> Result<BudgetSnapshot, SourceError> {
        Self::normalize(self.read_quota().await?)
    }

    async fn run(
        &self,
        updates: watch::Sender<fluxguard_runtime::SourceState>,
        cancel: CancellationToken,
    ) -> Result<(), SourceError> {
        fluxguard_runtime::publish_once(self, updates, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../../../tests/fixtures/copilot/quota_basic.json");

    fn window<'a>(snapshot: &'a BudgetSnapshot, id: &str) -> &'a BudgetWindow {
        snapshot
            .windows
            .iter()
            .find(|w| w.id.as_str() == id)
            .expect("window present")
    }

    #[test]
    fn quota_result_normalizes_limited_and_unlimited_snapshots() {
        let snapshot = crate::support::fixture_snapshot(FIXTURE, CopilotAdapter::normalize);

        assert_eq!(snapshot.windows.len(), 2);
        assert!(matches!(snapshot.availability, Availability::Allowed));

        let premium = window(&snapshot, "client.copilot.premium_interactions");
        assert_eq!(premium.label.as_deref(), Some("Premium Interactions"));
        assert_eq!(premium.used.map(|v| v.0), Some(75.0));
        assert_eq!(premium.limit.map(|v| v.0), Some(300.0));
        assert_eq!(premium.remaining.map(|v| v.0), Some(225.0));
        assert_eq!(premium.remaining_percent, Some(75.0));
        assert!(premium.resets_at.is_some());
        assert!(!premium.hard_blocked);
        assert_eq!(premium.applicability, Applicability::Applicable);

        let chat = window(&snapshot, "client.copilot.chat");
        assert_eq!(chat.limit, None);
        assert_eq!(chat.remaining_percent, None);
        assert_eq!(chat.applicability, Applicability::NotApplicable);
        assert!(!chat.hard_blocked);
    }

    #[test]
    fn exhausted_quota_without_overage_is_hard_blocked() {
        let payload = include_str!("../../../tests/fixtures/copilot/quota_exhausted.json");
        let snapshot = crate::support::fixture_snapshot(payload, CopilotAdapter::normalize);
        assert!(window(&snapshot, "client.copilot.premium_interactions").hard_blocked);

        let payload = payload.replace(
            "\"overageAllowedWithExhaustedQuota\": false",
            "\"overageAllowedWithExhaustedQuota\": true",
        );
        let snapshot = crate::support::fixture_snapshot(&payload, CopilotAdapter::normalize);
        assert!(!window(&snapshot, "client.copilot.premium_interactions").hard_blocked);
    }

    #[test]
    fn empty_or_malformed_payloads_are_handled() {
        let snapshot = CopilotAdapter::normalize(CopilotQuotaResult::default()).expect("normalize");
        assert!(snapshot.windows.is_empty());
        assert!(matches!(snapshot.availability, Availability::Unknown));
        assert_eq!(snapshot.warnings[0].code, "copilot_quota_missing");

        let bad_date = r#"{"quotaSnapshots":{"chat":{"entitlementRequests":10,"usedRequests":1,"resetDate":"soon"}}}"#;
        let snapshot = crate::support::fixture_snapshot(bad_date, CopilotAdapter::normalize);
        assert_eq!(snapshot.windows[0].resets_at, None);
        assert_eq!(snapshot.warnings[0].code, "invalid_reset_date");

        assert!(serde_json::from_str::<CopilotQuotaResult>(r#"{"quotaSnapshots": 42}"#).is_err());
    }

    #[test]
    fn remote_errors_mentioning_auth_map_to_unauthenticated() {
        let err = RpcError {
            code: -32000,
            message: "Not signed in to GitHub".into(),
        };
        assert!(matches!(
            classify_remote_error(&err),
            SourceError::Unauthenticated
        ));
        let err = RpcError {
            code: -32000,
            message: "boom".into(),
        };
        assert!(matches!(classify_remote_error(&err), SourceError::Protocol));
    }

    #[cfg(unix)]
    mod fake_cli {
        use super::*;
        use std::fs;

        /// Generous, because these tests assert on which error comes back: a
        /// slow machine must not turn an expected auth failure into a timeout.
        const READ_TIMEOUT: Duration = Duration::from_secs(30);

        /// Writes a shell script that answers with the given JSON-RPC bodies, in order,
        /// using Content-Length framing, then lingers until killed.
        fn script(replies: &[&str]) -> (std::path::PathBuf, std::path::PathBuf) {
            let root = crate::test_support::test_dir("copilot");
            let path = root.join("copilot");
            let mut lines = vec![
                "#!/bin/sh".to_string(),
                "if [ \"$1\" = \"--version\" ]; then echo 0.0.0-test; exit 0; fi".to_string(),
                "reply() { printf 'Content-Length: %s\\r\\n\\r\\n%s' \"${#1}\" \"$1\"; }"
                    .to_string(),
            ];
            for reply in replies {
                lines.push(format!("reply '{reply}'"));
            }
            lines.push("sleep 10".to_string());
            lines.push(String::new());
            crate::test_support::write_executable(&path, &lines.join("\n"));
            (root, path)
        }

        #[tokio::test(flavor = "current_thread")]
        async fn legacy_connect_fallback_then_quota_is_read() {
            let (root, path) = script(&[
                r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"Unhandled method connect"}}"#,
                r#"{"jsonrpc":"2.0","method":"log","params":{"level":"info"}}"#,
                r#"{"jsonrpc":"2.0","id":2,"result":{"quotaSnapshots":{"premium_interactions":{"entitlementRequests":300,"usedRequests":30,"remainingPercentage":90}}}}"#,
            ]);
            let adapter = CopilotAdapter::new(path.to_string_lossy().into_owned(), READ_TIMEOUT);
            assert_eq!(
                adapter.probe().await.expect("probe").state,
                ProbeState::Ready
            );
            let snapshot = adapter.refresh().await.expect("refresh");
            assert_eq!(snapshot.windows.len(), 1);
            assert_eq!(snapshot.windows[0].remaining_percent, Some(90.0));
            let _ = fs::remove_dir_all(root);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn unauthenticated_cli_is_reported_as_such() {
            let (root, path) = script(&[
                r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}"#,
                r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"No GitHub authentication available"}}"#,
            ]);
            let adapter = CopilotAdapter::new(path.to_string_lossy().into_owned(), READ_TIMEOUT);
            assert!(matches!(
                adapter.refresh().await,
                Err(SourceError::Unauthenticated)
            ));
            let _ = fs::remove_dir_all(root);
        }

        #[tokio::test(flavor = "current_thread")]
        async fn silent_cli_times_out_and_missing_binary_is_unavailable() {
            let (root, path) = script(&[]);
            let adapter = CopilotAdapter::new(
                path.to_string_lossy().into_owned(),
                Duration::from_millis(300),
            );
            assert!(matches!(adapter.refresh().await, Err(SourceError::Timeout)));
            let _ = fs::remove_dir_all(root);

            let missing = CopilotAdapter::with_default_timeout("/nonexistent/fluxguard-copilot");
            assert_eq!(
                missing.probe().await.expect("probe").state,
                ProbeState::BinaryMissing
            );
            assert!(matches!(
                missing.refresh().await,
                Err(SourceError::Unavailable)
            ));
        }
    }
}
