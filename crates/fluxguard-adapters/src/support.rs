//! Shared building blocks for adapters that normalize a handful of windows.

use ::time::{Duration as SignedDuration, OffsetDateTime};
use fluxguard_core::{
    Applicability, Availability, BudgetSnapshot, BudgetWindow, DecimalValue, Freshness,
    MetricDimension, Provenance, SnapshotWarning, SourceCapabilities, SourceDescriptor, SourceId,
    SourceKind, SourceQuality, WindowId,
};
use fluxguard_runtime::{ProbeReport, ProbeState, SourceError};

pub(crate) fn descriptor(
    id: &str,
    kind: SourceKind,
    display_name: &str,
    source_quality: SourceQuality,
    capabilities: SourceCapabilities,
) -> SourceDescriptor {
    SourceDescriptor {
        id: SourceId::new(id).expect("static source id is valid"),
        kind,
        display_name: display_name.into(),
        adapter_version: env!("CARGO_PKG_VERSION").into(),
        source_quality,
        capabilities,
    }
}

/// Capabilities shared by every snapshot-style adapter; only reset-time and
/// cost support vary between them.
pub(crate) fn capabilities(supports_reset_time: bool, supports_cost: bool) -> SourceCapabilities {
    SourceCapabilities {
        supports_snapshot: true,
        supports_push_updates: false,
        supports_reset_time,
        supports_exact_remaining_percent: true,
        supports_model_scope: true,
        supports_cost,
    }
}

/// Ready when any of the named environment variables is set; the value is
/// never read.
pub(crate) fn probe_env_keys(names: &[&str]) -> ProbeReport {
    let state = if names.iter().any(|name| std::env::var_os(name).is_some()) {
        ProbeState::Ready
    } else {
        ProbeState::NotAuthenticated
    };
    ProbeReport { state }
}

/// Raw inputs for one window; whatever is missing is derived when possible.
pub(crate) struct WindowSpec<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub dimension: MetricDimension,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub window_duration_seconds: Option<u64>,
    pub resets_at: Option<OffsetDateTime>,
    pub ttl_seconds: i64,
}

impl<'a> WindowSpec<'a> {
    pub(crate) fn new(id: &'a str, label: &'a str, dimension: MetricDimension) -> Self {
        Self {
            id,
            label,
            dimension,
            used: None,
            limit: None,
            remaining: None,
            remaining_percent: None,
            window_duration_seconds: None,
            resets_at: None,
            ttl_seconds: 60,
        }
    }
}

fn decimal(value: Option<f64>) -> Option<DecimalValue> {
    value.and_then(|value| DecimalValue::try_new(value).ok())
}

/// Builds an applicable, fresh window. `remaining`, `used`, and
/// `remaining_percent` are derived from `limit` when not supplied, so
/// used + remaining always equals limit.
pub(crate) fn push_window(
    windows: &mut Vec<BudgetWindow>,
    source: &SourceDescriptor,
    observed_via: &str,
    now: OffsetDateTime,
    spec: WindowSpec<'_>,
) -> Result<(), SourceError> {
    let id = WindowId::new(spec.id).map_err(|_| SourceError::InvalidPayload)?;
    let limit = decimal(spec.limit);
    let mut used = decimal(spec.used);
    let mut remaining = decimal(spec.remaining);
    if let (Some(limit), Some(used), None) = (limit, used, remaining) {
        remaining = decimal(Some((limit.0 - used.0).max(0.0)));
    }
    if let (Some(limit), None, Some(remaining)) = (limit, used, remaining) {
        used = decimal(Some((limit.0 - remaining.0).max(0.0)));
    }
    let remaining_percent = spec.remaining_percent.or_else(|| match (limit, remaining) {
        (Some(limit), Some(remaining)) if limit.0 > 0.0 => Some((remaining.0 / limit.0) * 100.0),
        _ => None,
    });
    let hard_blocked = remaining_percent.is_some_and(|value| value <= 0.0)
        || remaining.is_some_and(|value| value.0 <= 0.0);

    windows.push(BudgetWindow {
        id,
        label: Some(spec.label.into()),
        dimension: spec.dimension,
        used,
        limit,
        remaining,
        used_percent: remaining_percent.map(|value| (100.0 - value).clamp(0.0, 100.0)),
        remaining_percent,
        window_duration_seconds: spec.window_duration_seconds,
        resets_at: spec.resets_at,
        fresh_until: Some(now + SignedDuration::seconds(spec.ttl_seconds)),
        hard_blocked,
        applicability: Applicability::Applicable,
        observed_at: now,
        freshness: Freshness::Fresh,
        provenance: Provenance {
            source_id: source.id.clone(),
            source_quality: source.source_quality.clone(),
            observed_via: Some(observed_via.into()),
        },
    });
    Ok(())
}

/// Wraps windows into a snapshot. No windows means the source told us
/// nothing, which stays `Unknown` rather than becoming "allowed".
pub(crate) fn snapshot(
    source: SourceDescriptor,
    now: OffsetDateTime,
    windows: Vec<BudgetWindow>,
    missing_code: &str,
    missing_message: &str,
) -> BudgetSnapshot {
    let mut warnings = Vec::new();
    if windows.is_empty() {
        warnings.push(SnapshotWarning {
            code: missing_code.into(),
            message: missing_message.into(),
        });
    }
    BudgetSnapshot {
        source,
        account_scope: None,
        availability: if windows.is_empty() {
            Availability::Unknown
        } else {
            Availability::Allowed
        },
        windows,
        observed_at: now,
        warnings,
    }
}

/// Detection-only adapters must keep an empty snapshot `Unknown`, refuse to
/// refresh, and stop their run loop instead of polling forever.
#[cfg(test)]
pub(crate) async fn assert_detection_only(
    adapter: &dyn fluxguard_runtime::BudgetSource,
    empty: BudgetSnapshot,
) {
    use fluxguard_runtime::SourceState;

    assert!(empty.windows.is_empty());
    assert!(matches!(empty.availability, Availability::Unknown));
    assert!(matches!(
        adapter.refresh().await,
        Err(SourceError::UnsupportedVersion)
    ));

    let (sender, _receiver) =
        tokio::sync::watch::channel(SourceState::initial(adapter.descriptor()));
    let cancel = tokio_util::sync::CancellationToken::new();
    let run = adapter.run(sender, cancel);
    let result = tokio::time::timeout(std::time::Duration::from_secs(2), run)
        .await
        .expect("run stops without polling");
    assert!(matches!(result, Err(SourceError::UnsupportedVersion)));
}

/// Parses a JSON fixture and normalizes it, panicking with context on failure.
#[cfg(test)]
pub(crate) fn fixture_snapshot<S, E>(
    json: &str,
    normalize: fn(S) -> Result<BudgetSnapshot, E>,
) -> BudgetSnapshot
where
    S: serde::de::DeserializeOwned,
    E: std::fmt::Debug,
{
    let stats: S = serde_json::from_str(json).expect("fixture parses");
    normalize(stats).expect("fixture normalizes")
}

/// Expands to the detection-only contract test for one adapter.
#[cfg(test)]
macro_rules! detection_only_contract {
    ($adapter:ty, $ctor:expr, $stats:ty) => {
        #[tokio::test]
        async fn detection_only_contract() {
            let empty = <$adapter>::normalize(<$stats>::default()).expect("normalize");
            $crate::support::assert_detection_only(&$ctor, empty).await;
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceDescriptor {
        descriptor(
            "test.source",
            SourceKind::Client,
            "Test",
            SourceQuality::Manual,
            capabilities(false, false),
        )
    }

    #[test]
    fn window_derives_missing_amounts_and_blocks_on_exhaustion() {
        let now = OffsetDateTime::now_utc();
        let mut windows = Vec::new();
        push_window(
            &mut windows,
            &source(),
            "test",
            now,
            WindowSpec {
                used: Some(80.0),
                limit: Some(100.0),
                ..WindowSpec::new("test.window", "Window", MetricDimension::Requests)
            },
        )
        .expect("window");
        let window = &windows[0];
        assert_eq!(window.remaining.map(|v| v.0), Some(20.0));
        assert_eq!(window.remaining_percent, Some(20.0));
        assert_eq!(window.used_percent, Some(80.0));
        assert!(!window.hard_blocked);
        window.validate().expect("consistent window");

        push_window(
            &mut windows,
            &source(),
            "test",
            now,
            WindowSpec {
                remaining: Some(0.0),
                limit: Some(100.0),
                ..WindowSpec::new("test.empty", "Empty", MetricDimension::Requests)
            },
        )
        .expect("window");
        assert!(windows[1].hard_blocked);
        assert_eq!(windows[1].used.map(|v| v.0), Some(100.0));

        let empty = snapshot(source(), now, Vec::new(), "missing", "nothing");
        assert!(matches!(empty.availability, Availability::Unknown));
        assert_eq!(empty.warnings[0].code, "missing");
    }
}
