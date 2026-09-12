//! Release check and self-update.
//!
//! The latest release is discovered with `git ls-remote --tags`, so there is no
//! HTTP dependency, API token, or rate-limited endpoint involved. The answer is
//! cached for a day so repeated calls stay offline, and installing reuses the
//! same script a user would run by hand.

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

const REPOSITORY_URL: &str = "https://github.com/suiflex/FluxGuard";
/// Named inside the platform cache directory the caller supplies, so the answer
/// lands where a throwaway file belongs: excluded from backups on macOS,
/// sweepable on Linux, and outside the roaming profile on Windows.
const CACHE_FILE: &str = "update-check.json";
const THROTTLE_SECONDS: u64 = 86_400;

#[cfg(windows)]
pub const INSTALL_COMMAND: &str =
    "irm https://raw.githubusercontent.com/suiflex/FluxGuard/develop/scripts/install.ps1 | iex";
#[cfg(not(windows))]
pub const INSTALL_COMMAND: &str =
    "curl -fsSL https://raw.githubusercontent.com/suiflex/FluxGuard/develop/scripts/install.sh | sh";

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCache {
    checked_at: u64,
    latest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateCheck {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
}

/// Query whether a newer release exists, consulting the remote when the cache
/// is stale or `force` is set. `None` means the remote could not be reached and
/// nothing was cached earlier — an unknown answer, never "up to date".
pub fn check_for_update(cache_dir: &Path, force: bool) -> Option<UpdateCheck> {
    check_for_update_for(cache_dir, force, current_version())
}

pub fn check_for_update_for(
    cache_dir: &Path,
    force: bool,
    current_version: &str,
) -> Option<UpdateCheck> {
    if force || is_stale(cache_dir) {
        if let Some(latest) = fetch_latest_version() {
            write_cache(
                cache_dir,
                &UpdateCache {
                    checked_at: now_seconds(),
                    latest,
                },
            );
        }
    }
    let latest = read_cache(cache_dir)?.latest;
    let update_available = match (parse_version(&latest), parse_version(current_version)) {
        (Some(latest_version), Some(current)) => latest_version > current,
        _ => false,
    };
    Some(UpdateCheck {
        current: current_version.to_owned(),
        latest,
        update_available,
    })
}

/// Run the same installer a user would invoke by hand, with inherited stdio so
/// its output and any prompt stay visible.
pub fn run_install_command() -> io::Result<ExitStatus> {
    let mut command = installer();
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
}

#[cfg(not(windows))]
fn installer() -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(INSTALL_COMMAND);
    command
}

#[cfg(windows)]
fn installer() -> Command {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        INSTALL_COMMAND,
    ]);
    command
}

/// The highest `vX.Y.Z` tag on the remote. Pre-release and non-numeric tags are
/// ignored so a release candidate never looks newer than a release.
fn fetch_latest_version() -> Option<String> {
    let output = Command::new("git")
        .args(["ls-remote", "--tags", "--refs", REPOSITORY_URL])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    highest_tag(&listing)
}

fn highest_tag(listing: &str) -> Option<String> {
    listing
        .lines()
        .filter_map(|line| line.rsplit("refs/tags/").next())
        .filter_map(|tag| parse_version(tag).map(|version| (version, tag.to_owned())))
        .max_by_key(|(version, _)| *version)
        .map(|(_, tag)| tag.trim_start_matches('v').to_owned())
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.trim().trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    // A trailing pre-release or build suffix leaves an extra segment; treat the
    // tag as unparseable rather than guessing its order.
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn cache_path(cache_dir: &Path) -> PathBuf {
    cache_dir.join(CACHE_FILE)
}

fn read_cache(cache_dir: &Path) -> Option<UpdateCache> {
    let contents = fs::read_to_string(cache_path(cache_dir)).ok()?;
    serde_json::from_str(&contents).ok()
}

fn write_cache(cache_dir: &Path, cache: &UpdateCache) {
    let path = cache_path(cache_dir);
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(contents) = serde_json::to_string(cache) {
        let _ = fs::write(path, contents);
    }
}

fn is_stale(cache_dir: &Path) -> bool {
    match read_cache(cache_dir) {
        Some(cache) => now_seconds().saturating_sub(cache.checked_at) >= THROTTLE_SECONDS,
        None => true,
    }
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_cache_dir(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("fluxguard-update-{name}-{}", now_seconds()));
        fs::create_dir_all(&directory).expect("create test cache dir");
        directory
    }

    const LISTING: &str = "\
aaaa\trefs/tags/v0.1.2
bbbb\trefs/tags/v0.1.10
cccc\trefs/tags/v0.2.0-rc.1
dddd\trefs/tags/nightly
";

    #[test]
    fn highest_tag_orders_numerically_and_skips_pre_releases() {
        // Lexically "v0.1.2" beats "v0.1.10"; numerically it must not.
        assert_eq!(highest_tag(LISTING).as_deref(), Some("0.1.10"));
        assert_eq!(highest_tag(""), None);
    }

    #[test]
    fn versions_compare_by_component_and_reject_suffixes() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
        assert!(parse_version("1.2.3-rc.1").is_none());
        assert!(parse_version("1.2").is_none());
        assert!(parse_version("nightly").is_none());
        assert!(parse_version("1.2.3") > parse_version("1.2.2"));
    }

    #[test]
    fn cached_answer_decides_whether_an_update_is_available() {
        let cache = temp_cache_dir("cached");
        write_cache(
            &cache,
            &UpdateCache {
                checked_at: now_seconds(),
                latest: "9.9.9".into(),
            },
        );

        let check = check_for_update_for(&cache, false, "0.1.3").expect("cached check");
        assert_eq!(check.current, "0.1.3");
        assert_eq!(check.latest, "9.9.9");
        assert!(check.update_available);

        let check = check_for_update_for(&cache, false, "9.9.9").expect("cached check");
        assert!(!check.update_available);

        let _ = fs::remove_dir_all(cache);
    }

    #[test]
    fn an_unreadable_cache_is_no_answer_at_all() {
        // A truncated or hand-edited cache must read as "unknown" so the caller
        // reports that rather than treating the current version as latest.
        let cache = temp_cache_dir("corrupt");
        let path = cache_path(&cache);
        fs::create_dir_all(path.parent().expect("cache parent")).expect("create cache dir");
        fs::write(&path, "{ not json").expect("write cache");
        assert!(read_cache(&cache).is_none());
        assert!(is_stale(&cache));
        let _ = fs::remove_dir_all(cache);
    }

    #[test]
    fn a_fresh_cache_is_not_stale() {
        let cache = temp_cache_dir("stale");
        assert!(is_stale(&cache), "absent cache is stale");
        write_cache(
            &cache,
            &UpdateCache {
                checked_at: now_seconds(),
                latest: "0.1.3".into(),
            },
        );
        assert!(!is_stale(&cache));
        write_cache(
            &cache,
            &UpdateCache {
                checked_at: now_seconds().saturating_sub(THROTTLE_SECONDS),
                latest: "0.1.3".into(),
            },
        );
        assert!(is_stale(&cache));
        let _ = fs::remove_dir_all(cache);
    }
}
