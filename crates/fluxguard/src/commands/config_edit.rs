//! The interactive editor behind a bare `fluxguard config`.
//!
//! Every recommendation here comes from probing the adapters themselves rather
//! than from a second copy of their detection rules, and each row says whether
//! enabling that source actually yields quota data — enabling one that cannot
//! read anything is the mistake this screen exists to prevent.

use std::{fs, io::IsTerminal, path::Path};

use fluxguard_adapters::{
    clients::{
        antigravity::AntigravityAdapter, claude_code::ClaudeCodeAdapter, codex::CodexAdapter,
        copilot::CopilotAdapter, cursor::CursorAdapter, opencode::OpenCodeAdapter,
    },
    providers::{
        anthropic::AnthropicAdapter, openai::OpenAiAdapter, xai::XaiAdapter, zai::ZaiAdapter,
    },
};
use fluxguard_runtime::{BudgetSource, ProbeState};

use super::CliError;
use crate::config::Config;
use crate::theme;

/// Whether enabling a source yields quota data today. The detection-only ones
/// probe and report, but their refresh returns `source_unsupported`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Capability {
    ReadsQuota,
    DetectionOnly,
}

impl Capability {
    fn label(self) -> &'static str {
        match self {
            Self::ReadsQuota => "reads quota",
            Self::DetectionOnly => "detection only",
        }
    }
}

/// One row of the source menu: its config key, what it is, and what it can do.
struct Source {
    key: &'static str,
    display: &'static str,
    capability: Capability,
}

const SOURCES: &[Source] = &[
    Source {
        key: "clients.codex",
        display: "OpenAI Codex",
        capability: Capability::ReadsQuota,
    },
    Source {
        key: "clients.opencode",
        display: "OpenCode",
        capability: Capability::ReadsQuota,
    },
    Source {
        key: "clients.copilot",
        display: "GitHub Copilot",
        capability: Capability::ReadsQuota,
    },
    Source {
        key: "clients.cursor",
        display: "Cursor",
        capability: Capability::DetectionOnly,
    },
    Source {
        key: "clients.claude_code",
        display: "Claude Code",
        capability: Capability::DetectionOnly,
    },
    Source {
        key: "clients.antigravity",
        display: "Google Antigravity",
        capability: Capability::DetectionOnly,
    },
    Source {
        key: "providers.openai",
        display: "OpenAI API",
        capability: Capability::DetectionOnly,
    },
    Source {
        key: "providers.anthropic",
        display: "Anthropic API",
        capability: Capability::DetectionOnly,
    },
    Source {
        key: "providers.xai",
        display: "xAI API",
        capability: Capability::DetectionOnly,
    },
    Source {
        key: "providers.zai",
        display: "Z.AI GLM",
        capability: Capability::DetectionOnly,
    },
];

/// The thresholds, asked in the order they must descend.
const THRESHOLDS: [&str; 4] = ["guarded", "conserve", "critical", "emergency"];

fn enabled_in(config: &Config, key: &str) -> bool {
    match key {
        "clients.codex" => config.clients.codex.enabled,
        "clients.opencode" => config.clients.opencode.enabled,
        "clients.copilot" => config.clients.copilot.enabled,
        "clients.cursor" => config.clients.cursor.enabled,
        "clients.claude_code" => config.clients.claude_code.enabled,
        "clients.antigravity" => config.clients.antigravity.enabled,
        "providers.openai" => config.providers.openai.enabled,
        "providers.anthropic" => config.providers.anthropic.enabled,
        "providers.xai" => config.providers.xai.enabled,
        "providers.zai" => config.providers.zai.enabled,
        _ => false,
    }
}

fn set_enabled(config: &mut Config, key: &str, value: bool) {
    match key {
        "clients.codex" => config.clients.codex.enabled = value,
        "clients.opencode" => config.clients.opencode.enabled = value,
        "clients.copilot" => config.clients.copilot.enabled = value,
        "clients.cursor" => config.clients.cursor.enabled = value,
        "clients.claude_code" => config.clients.claude_code.enabled = value,
        "clients.antigravity" => config.clients.antigravity.enabled = value,
        "providers.openai" => config.providers.openai.enabled = value,
        "providers.anthropic" => config.providers.anthropic.enabled = value,
        "providers.xai" => config.providers.xai.enabled = value,
        "providers.zai" => config.providers.zai.enabled = value,
        _ => {}
    }
}

/// Ask each adapter about itself, using the commands the configuration already
/// names so a custom `command` is respected.
async fn probe_states(config: &Config) -> Vec<ProbeState> {
    let clients = &config.clients;
    let sources: Vec<Box<dyn BudgetSource>> = vec![
        Box::new(CodexAdapter::with_default_timeout(
            clients.codex.command.clone(),
        )),
        Box::new(OpenCodeAdapter::with_default_timeout(
            clients.opencode.command.clone(),
        )),
        Box::new(CopilotAdapter::with_default_timeout(
            clients.copilot.command.clone(),
        )),
        Box::new(CursorAdapter::new()),
        Box::new(ClaudeCodeAdapter::new(clients.claude_code.command.clone())),
        Box::new(AntigravityAdapter::new(clients.antigravity.command.clone())),
        Box::new(OpenAiAdapter::new()),
        Box::new(AnthropicAdapter::new()),
        Box::new(XaiAdapter::new()),
        Box::new(ZaiAdapter::new()),
    ];
    let mut states = Vec::with_capacity(sources.len());
    for source in sources {
        states.push(
            source
                .probe()
                .await
                .map(|report| report.state)
                .unwrap_or(ProbeState::BinaryMissing),
        );
    }
    states
}

fn present(state: &ProbeState) -> bool {
    matches!(state, ProbeState::Ready | ProbeState::Partial)
}

/// Rows are padded into columns so the markers and capabilities line up.
fn menu_rows(states: &[ProbeState]) -> Vec<String> {
    let width = SOURCES
        .iter()
        .map(|source| source.display.len())
        .max()
        .unwrap_or_default();
    SOURCES
        .iter()
        .zip(states)
        .map(|(source, state)| {
            let mark = if present(state) { "·" } else { " " };
            format!(
                "{:<width$} {mark} {:<14} {}",
                source.display,
                source.capability.label(),
                super::probe_state_name(state),
            )
        })
        .collect()
}

/// Selected rows come back as strings, so they are matched by position.
fn selected_keys(rows: &[String], picked: &[String]) -> Vec<&'static str> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| picked.contains(row))
        .filter_map(|(index, _)| SOURCES.get(index).map(|source| source.key))
        .collect()
}

/// Why a threshold cannot be accepted, phrased for the prompt that rejected it.
/// `above` is the level this one must stay under, absent for the first.
fn threshold_error(name: &str, above: Option<(&str, f64)>, value: f64) -> Option<String> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Some("enter a number between 0 and 100".to_owned());
    }
    // Each level must sit below the one before it, or the pressure ladder has a
    // rung that can never be reached.
    match above {
        Some((previous, ceiling)) if value > ceiling => Some(format!(
            "{name} must be at most {ceiling}, the {previous} threshold above it"
        )),
        _ => None,
    }
}

/// Ask for the four thresholds one at a time, each rejected on the spot when it
/// breaks the order, so a typo is corrected where it was made rather than
/// failing the whole screen at the end.
fn thresholds_prompt(current: [f64; 4]) -> Result<[f64; 4], CliError> {
    let mut values = current;
    for slot in 0..THRESHOLDS.len() {
        let name = THRESHOLDS[slot];
        let above = slot
            .checked_sub(1)
            .map(|previous| (THRESHOLDS[previous], values[previous]));
        let ceiling = above.map_or(100.0, |(_, ceiling)| ceiling);
        let help = match above {
            None => "remaining % that starts this level · 0–100".to_owned(),
            Some((previous, _)) => {
                format!("remaining % that starts this level · 0–{ceiling} (at most {previous})")
            }
        };
        let validator = move |value: &f64| match threshold_error(name, above, *value) {
            Some(message) => Ok(inquire::validator::Validation::Invalid(message.into())),
            None => Ok(inquire::validator::Validation::Valid),
        };
        values[slot] = inquire::CustomType::<f64>::new(&format!("{name} remaining %"))
            // Offering the current value keeps Enter meaningful: it keeps what
            // is already configured.
            .with_default(current[slot].min(ceiling))
            .with_error_message("enter a number between 0 and 100")
            .with_validator(validator)
            .with_help_message(&help)
            .with_render_config(theme::render_config())
            .prompt()
            .map_err(|_| CliError::InstallCancelled)?;
    }
    Ok(values)
}

/// Serialize the whole configuration, so manual sources and commands survive.
/// Comments in the previous file do not: it is copied to `.bak` first, the same
/// way `install` treats a client configuration it rewrites.
fn write_config(path: &Path, config: &Config) -> Result<(), CliError> {
    let rendered = toml::to_string_pretty(config).map_err(|_| CliError::InvalidClientConfig)?;
    if path.exists() {
        let backup = path.with_extension("toml.bak");
        fs::copy(path, backup).map_err(|_| CliError::InvalidClientConfig)?;
    } else if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| CliError::InvalidClientConfig)?;
    }
    fs::write(path, rendered).map_err(|_| CliError::InvalidClientConfig)
}

pub async fn run(mut config: Config, path: &Path, dry_run: bool) -> Result<(), CliError> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(CliError::ConfigRequiresTty);
    }
    println!("{}\n", theme::banner());

    let states = probe_states(&config).await;
    let detected: Vec<&str> = SOURCES
        .iter()
        .zip(&states)
        .filter(|(_, state)| present(state))
        .map(|(source, _)| source.display)
        .collect();
    println!(
        "{}\n",
        theme::step(
            "config — detected",
            &[
                format!("file: {}", path.display()),
                format!(
                    "present: {}",
                    if detected.is_empty() {
                        "nothing found on this machine".to_owned()
                    } else {
                        detected.join(", ")
                    }
                ),
            ],
            if detected.is_empty() {
                theme::AMBER
            } else {
                theme::ACCENT
            },
        )
    );

    let rows = menu_rows(&states);
    // Keep what is already enabled; on a first run, offer the sources that are
    // both present and able to return data.
    let defaults: Vec<usize> = SOURCES
        .iter()
        .zip(&states)
        .enumerate()
        .filter(|(_, (source, state))| {
            if path.exists() {
                enabled_in(&config, source.key)
            } else {
                present(state) && source.capability == Capability::ReadsQuota
            }
        })
        .map(|(index, _)| index)
        .collect();

    let formatter = &|picked: &[inquire::list_option::ListOption<&String>]| -> String {
        picked
            .iter()
            .map(|option| option.value.split("  ").next().unwrap_or_default().trim())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let picked = inquire::MultiSelect::new("Which sources should FluxGuard watch?", rows.clone())
        .with_default(&defaults)
        .with_page_size(SOURCES.len())
        .with_formatter(formatter)
        .with_render_config(theme::render_config())
        .with_help_message(
            "↑↓ move · space toggle · enter confirm · detection-only sources report unsupported",
        )
        .prompt()
        .map_err(|_| CliError::InstallCancelled)?;

    let keys = selected_keys(&rows, &picked);
    for source in SOURCES {
        set_enabled(&mut config, source.key, keys.contains(&source.key));
    }

    let current = [
        config.pressure.guarded_remaining_percent,
        config.pressure.conserve_remaining_percent,
        config.pressure.critical_remaining_percent,
        config.pressure.emergency_remaining_percent,
    ];
    let [guarded, conserve, critical, emergency] = thresholds_prompt(current)?;
    config.pressure.guarded_remaining_percent = guarded;
    config.pressure.conserve_remaining_percent = conserve;
    config.pressure.critical_remaining_percent = critical;
    config.pressure.emergency_remaining_percent = emergency;

    // Refuse to write a file the next run would reject.
    config.validate()?;

    println!(
        "\n{}",
        theme::step(
            "config — writing",
            &[
                format!(
                    "sources: {}",
                    if keys.is_empty() {
                        "none".to_owned()
                    } else {
                        keys.join(", ")
                    }
                ),
                format!("thresholds: {guarded} / {conserve} / {critical} / {emergency}"),
                format!("file: {}", path.display()),
            ],
            theme::ACCENT,
        )
    );
    if dry_run {
        println!(
            "{}",
            theme::point("dry run — nothing written", theme::AMBER)
        );
        print!("\n{}", toml::to_string_pretty(&config).unwrap_or_default());
        return Ok(());
    }
    let had_file = path.exists();
    write_config(path, &config)?;
    println!(
        "{}",
        theme::point(
            &if had_file {
                format!("updated {} (previous kept as .toml.bak)", path.display())
            } else {
                format!("created {}", path.display())
            },
            theme::ACCENT,
        )
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states() -> Vec<ProbeState> {
        let mut states = vec![ProbeState::BinaryMissing; SOURCES.len()];
        states[0] = ProbeState::Ready; // codex
        states[6] = ProbeState::NotAuthenticated; // openai
        states
    }

    #[test]
    fn every_source_key_round_trips_through_the_config() {
        let mut config = Config::default();
        for source in SOURCES {
            set_enabled(&mut config, source.key, true);
            assert!(enabled_in(&config, source.key), "{}", source.key);
            set_enabled(&mut config, source.key, false);
            assert!(!enabled_in(&config, source.key), "{}", source.key);
        }
    }

    #[test]
    fn rows_mark_what_is_present_and_what_can_read_quota() {
        let rows = menu_rows(&states());
        assert_eq!(rows.len(), SOURCES.len());
        assert!(rows[0].contains('·'), "{}", rows[0]);
        assert!(rows[0].contains("reads quota"));
        assert!(rows[0].contains("ready"));
        // Not present, so no marker, and it is honest about reading nothing.
        assert!(!rows[3].contains('·'), "{}", rows[3]);
        assert!(rows[3].contains("detection only"));
        assert!(rows[6].contains("not_authenticated"));
    }

    #[test]
    fn selection_maps_rows_back_to_config_keys() {
        let rows = menu_rows(&states());
        let picked = vec![rows[0].clone(), rows[8].clone()];
        assert_eq!(
            selected_keys(&rows, &picked),
            vec!["clients.codex", "providers.xai"]
        );
        assert!(selected_keys(&rows, &[]).is_empty());
    }

    #[test]
    fn written_config_is_valid_and_reloadable() {
        let directory = std::env::temp_dir().join(format!(
            "fluxguard-config-edit-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("create test dir");
        let path = directory.join("config.toml");

        let mut config = Config::default();
        set_enabled(&mut config, "providers.zai", true);
        config.pressure.guarded_remaining_percent = 65.0;
        write_config(&path, &config).expect("write");

        let reloaded = Config::load(Some(&path)).expect("reload");
        assert!(reloaded.providers.zai.enabled);
        assert_eq!(reloaded.pressure.guarded_remaining_percent, 65.0);
        assert!(
            reloaded.clients.codex.enabled,
            "defaults survive the round trip"
        );

        // A second write keeps the previous file beside it.
        write_config(&path, &config).expect("rewrite");
        assert!(directory.join("config.toml.bak").exists());

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn thresholds_are_rejected_where_the_typo_was_made() {
        // Out of range, whatever sits above it.
        assert!(threshold_error("guarded", None, 120.0).is_some());
        assert!(threshold_error("guarded", None, -1.0).is_some());
        assert!(threshold_error("guarded", None, f64::NAN).is_some());
        assert!(threshold_error("guarded", None, 50.0).is_none());

        // The example that started this: conserve typed above guarded.
        let message = threshold_error("conserve", Some(("guarded", 50.0)), 60.0)
            .expect("a higher value than the level above must be refused");
        assert!(message.contains("at most 50"), "{message}");
        assert!(message.contains("guarded"), "{message}");

        // Equal is allowed; the ladder only forbids climbing back up.
        assert!(threshold_error("conserve", Some(("guarded", 50.0)), 50.0).is_none());
        assert!(threshold_error("conserve", Some(("guarded", 50.0)), 25.0).is_none());
    }

    #[test]
    fn every_threshold_is_named_in_descending_order() {
        assert_eq!(
            THRESHOLDS,
            ["guarded", "conserve", "critical", "emergency"],
            "the prompt order is the order the values must descend in",
        );
    }
}
