use std::{
    fs,
    io::{self, IsTerminal},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
    sync::Arc,
    time::Duration,
};

use clap::{Parser, Subcommand};
use fluxguard_adapters::{
    clients::{
        antigravity::AntigravityAdapter, claude_code::ClaudeCodeAdapter, codex::CodexAdapter,
        copilot::CopilotAdapter, cursor::CursorAdapter, opencode::OpenCodeAdapter,
    },
    providers::{
        anthropic::AnthropicAdapter, openai::OpenAiAdapter, xai::XaiAdapter, zai::ZaiAdapter,
    },
};
use fluxguard_core::{
    advise, assess_combined, estimate_operation_cost, CombinedSnapshot, OperationImportance,
    OperationKind, OperationProfile, PressureLevel,
};
use fluxguard_mcp::FluxGuardServer;
use fluxguard_runtime::{BudgetSource, ManualSource, SourceRegistry, SourceStateKind};
use thiserror::Error;
use tokio::sync::Mutex;

mod config_edit;

use crate::config::{Config, ConfigError};
use crate::theme;

#[derive(Debug, Error)]
pub enum CliError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("source registration failed")]
    Registration,
    #[error("output serialization failed")]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    Runtime(#[from] fluxguard_runtime::SourceError),
    #[error("MCP server failed: {0}")]
    Server(String),
    #[error("install requires a TTY when --client is omitted")]
    InstallRequiresTty,
    #[error("editing the configuration requires a TTY")]
    ConfigRequiresTty,
    #[error("install cancelled")]
    InstallCancelled,
    #[error("could not determine the latest release")]
    UpdateCheckUnavailable,
    #[error("the installer did not complete")]
    UpdateFailed,
    #[error("unsupported client: {0}")]
    UnsupportedClient(String),
    #[error("MCP client configuration is invalid")]
    InvalidClientConfig,
    #[error(
        "MCP client configuration conflicts with an existing entry; pass --force to replace it"
    )]
    ClientConfigConflict,
    #[error("MCP client command failed")]
    ClientCommandFailed,
}

#[derive(Debug, Parser)]
#[command(
    name = "fluxguard",
    version,
    about = "Resource awareness for AI coding agents"
)]
pub struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Serve,
    Install {
        #[arg(long)]
        client: Option<String>,
        #[arg(long, default_value = "fluxguard")]
        name: String,
        #[arg(long)]
        project: bool,
        #[arg(long)]
        print: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
    },
    /// Run the local source supervisor in the foreground.
    Daemon,
    ServeHttp {
        #[arg(long, default_value = "127.0.0.1:8080")]
        listen: SocketAddr,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    Sources,
    Doctor,
    #[command(alias = "hook")]
    Advice {
        operation: String,
        #[arg(long, default_value = "optional")]
        importance: String,
        #[arg(long)]
        json: bool,
    },
    /// Inspect or edit the configuration; without a subcommand this opens the
    /// interactive editor.
    Config {
        #[command(subcommand)]
        command: Option<ConfigCommand>,
        /// Show the configuration that would be written, and write nothing
        #[arg(long)]
        dry_run: bool,
    },
    /// Update FluxGuard to the latest release, or only check for one.
    ///
    /// Running `fluxguard update` installs a newer release when one exists;
    /// `--check` reports without installing anything.
    Update {
        /// Only report whether a newer release exists
        #[arg(long)]
        check: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Validate the configuration and the environment overrides
    Check,
    /// Print the configuration file this machine reads
    Path,
}

pub async fn run(cli: Cli) -> Result<(), CliError> {
    // Updating must work even when the configuration on disk is the thing that
    // needs fixing, so it runs before the config is read.
    if let Command::Update { check, json } = cli.command {
        return update(check, json);
    }
    let config = Config::load(cli.config.as_deref())?;
    match cli.command {
        Command::Serve => serve(config).await,
        Command::Install {
            client,
            name,
            project,
            print,
            dry_run,
            force,
        } => install(client, name, project, print, dry_run, force),
        Command::Daemon => serve(config).await,
        Command::ServeHttp { listen } => serve_http(config, listen).await,
        Command::Status { json } => status(config, json).await,
        Command::Sources => sources(config).await,
        Command::Doctor => doctor(config).await,
        Command::Advice {
            operation,
            importance,
            json,
        } => advice(config, operation, importance, json).await,
        Command::Config { command, dry_run } => {
            // `--config` names the file to act on; otherwise it is this
            // machine's own.
            let path = cli.config.unwrap_or_else(Config::path);
            configure(config, command, &path, dry_run).await
        }
        Command::Update { .. } => unreachable!("handled before the configuration is loaded"),
    }
}

async fn configure(
    config: Config,
    command: Option<ConfigCommand>,
    path: &Path,
    dry_run: bool,
) -> Result<(), CliError> {
    match command {
        // Loading already validated it, including the environment overrides.
        Some(ConfigCommand::Check) => {
            println!("configuration valid");
            Ok(())
        }
        Some(ConfigCommand::Path) => {
            println!("{}", path.display());
            if !path.exists() {
                println!("(no file yet; defaults are in use)");
            }
            Ok(())
        }
        None => config_edit::run(config, path, dry_run).await,
    }
}

fn update(check_only: bool, json: bool) -> Result<(), CliError> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or(CliError::InvalidClientConfig)?);
    let current = crate::update::current_version();
    let Some(check) = crate::update::check_for_update(&home, true) else {
        // The remote could not be reached and nothing was cached: unknown, not
        // "up to date".
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "unknown",
                    "current": current,
                }))?
            );
        } else {
            println!(
                "{}",
                theme::point("release check unavailable", theme::AMBER)
            );
            println!("  current: {current}");
            println!("  action: check your network, or install manually:");
            println!("          {}", crate::update::INSTALL_COMMAND);
        }
        return Err(CliError::UpdateCheckUnavailable);
    };

    if !check.update_available {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "up_to_date",
                    "current": check.current,
                    "latest": check.latest,
                    "update_available": false,
                }))?
            );
        } else {
            println!("{}", theme::point("up to date", theme::ACCENT));
            println!("  current: {}", check.current);
        }
        return Ok(());
    }

    if check_only {
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "status": "update_available",
                    "current": check.current,
                    "latest": check.latest,
                    "update_available": true,
                }))?
            );
        } else {
            println!("{}", theme::point("update available", theme::AMBER));
            println!("  current: {}", check.current);
            println!("  latest:  {}", check.latest);
            println!("  action: run `fluxguard update` to install it");
        }
        return Ok(());
    }

    if !json {
        println!(
            "{}",
            theme::step(
                "update",
                &[format!("{} → {}", check.current, check.latest)],
                theme::ACCENT,
            )
        );
    }
    let status = crate::update::run_install_command().map_err(|_| CliError::UpdateFailed)?;
    if !status.success() {
        return Err(CliError::UpdateFailed);
    }
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "status": "updated",
                "previous": check.current,
                "latest": check.latest,
            }))?
        );
    } else {
        println!(
            "{}",
            theme::point(&format!("installed {}", check.latest), theme::ACCENT)
        );
    }
    Ok(())
}
fn install(
    client: Option<String>,
    name: String,
    project: bool,
    print: bool,
    dry_run: bool,
    force: bool,
) -> Result<(), CliError> {
    let client = match client {
        Some(client) => client,
        None => choose_client()?,
    };
    if !SUPPORTED_INSTALL_CLIENTS.contains(&client.as_str()) {
        return Err(CliError::UnsupportedClient(client));
    }

    let entry = server_entry(&client);
    if print || dry_run {
        println!("{}", serde_json::to_string_pretty(&entry)?);
    }

    match client.as_str() {
        "codex" => install_codex(&name, dry_run, force),
        "claude-code" => {
            let path = if project {
                PathBuf::from(".mcp.json")
            } else {
                home_path(".claude.json")?
            };
            install_file_client(&name, &path, &["mcpServers"], entry, print, dry_run, force)
        }
        "cursor" => install_file_client(
            &name,
            &home_path(".cursor/mcp.json")?,
            &["mcpServers"],
            entry,
            print,
            dry_run,
            force,
        ),
        "opencode" => install_file_client(
            &name,
            &home_path(".config/opencode/opencode.jsonc")?,
            &["mcp"],
            entry,
            print,
            dry_run,
            force,
        ),
        "antigravity" => install_file_client(
            &name,
            &home_path(".gemini/antigravity/mcp_config.json")?,
            &["mcpServers"],
            entry,
            print,
            dry_run,
            force,
        ),
        "openclaw" => install_file_client(
            &name,
            &home_path(".openclaw/openclaw.json")?,
            &["mcp", "servers"],
            entry,
            print,
            dry_run,
            force,
        ),
        "generic-json" | "omp" | "hermes" | "9router" => {
            println!("Configure this client to launch `fluxguard serve` over local MCP stdio.");
            Ok(())
        }
        _ => unreachable!("client list is exhaustive"),
    }
}

const SUPPORTED_INSTALL_CLIENTS: &[&str] = &[
    "claude-code",
    "codex",
    "cursor",
    "opencode",
    "antigravity",
    "openclaw",
    "omp",
    "hermes",
    "9router",
    "generic-json",
];

/// What selecting a client writes, so the menu says more than a bare name.
/// `None` means the client is configured by hand and nothing is detectable.
const CLIENT_SUMMARY: &[(&str, &str, Option<&str>)] = &[
    (
        "claude-code",
        "~/.claude.json (--project: .mcp.json)",
        Some(".claude.json"),
    ),
    ("codex", "codex mcp add", Some(".codex/config.toml")),
    ("cursor", "~/.cursor/mcp.json", Some(".cursor")),
    (
        "opencode",
        "~/.config/opencode/opencode.jsonc",
        Some(".config/opencode"),
    ),
    (
        "antigravity",
        "~/.gemini/antigravity/mcp_config.json",
        Some(".gemini"),
    ),
    ("openclaw", "~/.openclaw/openclaw.json", Some(".openclaw")),
    ("omp", "prints manual MCP instructions", None),
    ("hermes", "prints manual MCP instructions", None),
    ("9router", "prints manual MCP instructions", None),
    ("generic-json", "prints manual MCP instructions", None),
];

/// A client counts as present when the path it would be configured through
/// already exists. Nothing is read from those files; only their presence.
fn detected_clients() -> Vec<&'static str> {
    CLIENT_SUMMARY
        .iter()
        .filter(|(_, _, marker)| {
            marker.is_some_and(|marker| home_path(marker).is_ok_and(|path| path.exists()))
        })
        .map(|(client, _, _)| *client)
        .collect()
}

/// Menu rows pair the client name with what selecting it writes, padded so the
/// summaries line up into a readable column.
fn client_menu_rows(detected: &[&str]) -> Vec<String> {
    let width = CLIENT_SUMMARY
        .iter()
        .map(|(client, _, _)| client.len())
        .max()
        .unwrap_or_default();
    CLIENT_SUMMARY
        .iter()
        .map(|(client, summary, _)| {
            let mark = if detected.contains(client) { "·" } else { " " };
            format!("{client:<width$} {mark} {summary}")
        })
        .collect()
}

fn client_from_row(row: &str) -> Option<&'static str> {
    let name = row.split_whitespace().next()?;
    SUPPORTED_INSTALL_CLIENTS
        .iter()
        .find(|client| **client == name)
        .copied()
}

fn choose_client() -> Result<String, CliError> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(CliError::InstallRequiresTty);
    }
    println!("{}\n", theme::banner());

    let detected = detected_clients();
    println!(
        "{}\n",
        theme::step(
            "install — detected",
            &[if detected.is_empty() {
                "no client configuration found".to_owned()
            } else {
                format!("{} — marked with · below", detected.join(", "))
            }],
            if detected.is_empty() {
                theme::AMBER
            } else {
                theme::ACCENT
            },
        )
    );

    let rows = client_menu_rows(&detected);
    // Start the cursor on the first detected client, which is the one a user
    // most likely came here to configure.
    let cursor = detected
        .first()
        .and_then(|client| {
            CLIENT_SUMMARY
                .iter()
                .position(|(candidate, _, _)| candidate == client)
        })
        .unwrap_or_default();
    // The row carries its summary, which makes a useful menu but a wrapped mess
    // once echoed back as the answer. Echo the name alone.
    let formatter =
        &|row: inquire::list_option::ListOption<&String>| -> String { first_word(row.value) };
    let picked = inquire::Select::new("Which client?", rows)
        .with_starting_cursor(cursor)
        .with_page_size(CLIENT_SUMMARY.len())
        .with_formatter(formatter)
        .with_render_config(theme::render_config())
        .with_help_message("↑↓ move · enter confirm · esc cancel")
        .prompt()
        .map_err(|_| CliError::InstallCancelled)?;

    client_from_row(&picked)
        .map(Into::into)
        .ok_or_else(|| CliError::UnsupportedClient(first_word(&picked)))
}

fn first_word(row: &str) -> String {
    row.split_whitespace().next().unwrap_or_default().to_owned()
}

fn server_entry(client: &str) -> serde_json::Value {
    match client {
        "opencode" => serde_json::json!({
            "type": "local",
            "command": ["fluxguard", "serve"],
            "enabled": true
        }),
        "openclaw" => serde_json::json!({
            "command": "fluxguard",
            "args": ["serve"],
            "transport": "stdio"
        }),
        _ => serde_json::json!({
            "command": "fluxguard",
            "args": ["serve"]
        }),
    }
}

fn install_codex(name: &str, dry_run: bool, force: bool) -> Result<(), CliError> {
    let mut command = format!("codex mcp add {name}");
    if force {
        command.push_str(&format!("  (remove existing {name} first)"));
    }
    command.push_str(" -- fluxguard serve");
    if dry_run {
        println!("{command}");
        return Ok(());
    }
    if force {
        let _ = ProcessCommand::new("codex")
            .args(["mcp", "remove", name])
            .status();
    }
    let status = ProcessCommand::new("codex")
        .args(["mcp", "add", name, "--", "fluxguard", "serve"])
        .status()
        .map_err(|_| CliError::ClientCommandFailed)?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::ClientCommandFailed)
    }
}

fn install_file_client(
    name: &str,
    path: &Path,
    key_path: &[&str],
    entry: serde_json::Value,
    print: bool,
    dry_run: bool,
    force: bool,
) -> Result<(), CliError> {
    let mut root = if path.exists() {
        let content = fs::read_to_string(path).map_err(|_| CliError::InvalidClientConfig)?;
        serde_json::from_str::<serde_json::Value>(&strip_json_comments(&content))
            .map_err(|_| CliError::InvalidClientConfig)?
    } else {
        serde_json::json!({})
    };
    let target = ensure_object_path(&mut root, key_path)?;
    let object = target
        .as_object_mut()
        .ok_or(CliError::InvalidClientConfig)?;
    if let Some(existing) = object.get(name) {
        if existing != &entry && !force {
            return Err(CliError::ClientConfigConflict);
        }
    }
    object.insert(name.into(), entry);

    if print {
        println!("target: {}", path.display());
    }
    if dry_run {
        return Ok(());
    }
    if path.exists() {
        let backup = PathBuf::from(format!("{}.bak", path.display()));
        fs::copy(path, backup).map_err(|_| CliError::InvalidClientConfig)?;
    } else if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| CliError::InvalidClientConfig)?;
    }
    let content = serde_json::to_string_pretty(&root).map_err(|_| CliError::InvalidClientConfig)?;
    fs::write(path, format!("{content}\n")).map_err(|_| CliError::InvalidClientConfig)?;
    println!(
        "installed FluxGuard MCP entry `{name}` into {}",
        path.display()
    );
    Ok(())
}

fn ensure_object_path<'a>(
    root: &'a mut serde_json::Value,
    keys: &[&str],
) -> Result<&'a mut serde_json::Value, CliError> {
    let mut current = root;
    for key in keys {
        let object = current
            .as_object_mut()
            .ok_or(CliError::InvalidClientConfig)?;
        current = object.entry(*key).or_insert_with(|| serde_json::json!({}));
    }
    Ok(current)
}
fn strip_json_comments(content: &str) -> String {
    let mut output = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while let Some(character) = chars.next() {
        if line_comment {
            if character == '\n' {
                line_comment = false;
                output.push(character);
            }
            continue;
        }
        if block_comment {
            if character == '*' && chars.peek() == Some(&'/') {
                chars.next();
                block_comment = false;
                output.push(' ');
            } else if character == '\n' {
                output.push(character);
            }
            continue;
        }
        if in_string {
            output.push(character);
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
            continue;
        }
        if character == '"' {
            in_string = true;
            output.push(character);
        } else if character == '/' && chars.peek() == Some(&'/') {
            chars.next();
            line_comment = true;
        } else if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            block_comment = true;
        } else {
            output.push(character);
        }
    }
    output
}

fn home_path(relative: &str) -> Result<PathBuf, CliError> {
    let home = std::env::var_os("HOME").ok_or(CliError::InvalidClientConfig)?;
    Ok(PathBuf::from(home).join(relative))
}

async fn serve(config: Config) -> Result<(), CliError> {
    let registry = Arc::new(Mutex::new(build_registry(&config)?));
    registry.lock().await.start();
    let server = FluxGuardServer::with_pressure_config(registry.clone(), config.pressure_config()?);
    let result = server
        .serve_stdio()
        .await
        .map_err(|error| CliError::Server(error.to_string()));
    registry.lock().await.shutdown().await;
    result
}

async fn serve_http(config: Config, listen: SocketAddr) -> Result<(), CliError> {
    if !listen.ip().is_loopback() {
        return Err(CliError::Server(
            "remote HTTP transport requires authenticated deployment".into(),
        ));
    }
    let registry = Arc::new(Mutex::new(build_registry(&config)?));
    registry.lock().await.start();
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .map_err(|error| CliError::Server(error.to_string()))?;
    let server = FluxGuardServer::with_pressure_config(registry.clone(), config.pressure_config()?);
    let result = server
        .serve_http(listener)
        .await
        .map_err(|error| CliError::Server(error.to_string()));
    registry.lock().await.shutdown().await;
    result
}

async fn advice(
    config: Config,
    operation: String,
    importance: String,
    json: bool,
) -> Result<(), CliError> {
    let kind = parse_operation(&operation)?;
    let importance = parse_importance(&importance)?;
    let registry = build_registry(&config)?;
    for state in registry.states() {
        let _ = registry.refresh(&state.descriptor.id).await;
    }
    let snapshots = registry
        .states()
        .into_iter()
        .filter_map(|state| state.snapshot)
        .collect();
    let pressure = assess_combined(&CombinedSnapshot { snapshots }, &config.pressure_config()?);
    let advice = advise(
        &pressure,
        &OperationProfile {
            estimated_cost: estimate_operation_cost(&kind),
            kind,
            importance,
        },
    );
    if json {
        println!("{}", serde_json::to_string_pretty(&advice)?);
    } else {
        println!("proceed: {:?}", advice.proceed);
        println!("mode: {:?}", advice.mode);
        println!("pressure: {}", pressure_name(&advice.pressure.level));
        println!("recommendations: {}", advice.recommendations.len());
    }
    Ok(())
}

async fn status(config: Config, json: bool) -> Result<(), CliError> {
    let registry = build_registry(&config)?;
    for state in registry.states() {
        let _ = registry.refresh(&state.descriptor.id).await;
    }
    let states = registry.states();
    let snapshots = states
        .iter()
        .filter_map(|state| state.snapshot.clone())
        .collect();
    let pressure = assess_combined(&CombinedSnapshot { snapshots }, &config.pressure_config()?);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "pressure": pressure.level,
                "remaining_percent": pressure.effective_remaining_percent,
                "confidence": pressure.confidence,
                "bottleneck": pressure.bottleneck,
                "reasons": pressure.reasons,
            }))?
        );
    } else {
        println!("pressure: {}", pressure_name(&pressure.level));
        println!("confidence: {:?}", pressure.confidence);
        println!("sources: {}", states.len());
    }
    Ok(())
}

async fn sources(config: Config) -> Result<(), CliError> {
    let registry = build_registry(&config)?;
    for state in registry.states() {
        println!(
            "{}: {}",
            state.descriptor.id.as_str(),
            state_name(&state.status)
        );
    }
    Ok(())
}

async fn doctor(config: Config) -> Result<(), CliError> {
    const DETECTION_ONLY: bool = true;
    let clients = &config.clients;
    let providers = &config.providers;

    doctor_source(
        "client.codex",
        clients.codex.enabled,
        &CodexAdapter::with_default_timeout(clients.codex.command.clone()),
        Some("install Codex or set clients.codex.command"),
        !DETECTION_ONLY,
        &[],
    )
    .await;
    doctor_source(
        "client.opencode",
        clients.opencode.enabled,
        &OpenCodeAdapter::with_default_timeout(clients.opencode.command.clone()),
        Some("install OpenCode or set clients.opencode.command"),
        !DETECTION_ONLY,
        &[],
    )
    .await;
    doctor_source(
        "client.copilot",
        clients.copilot.enabled,
        &CopilotAdapter::with_default_timeout(clients.copilot.command.clone()),
        Some("install GitHub Copilot CLI (copilot) or set clients.copilot.command"),
        !DETECTION_ONLY,
        &[],
    )
    .await;
    doctor_source(
        "client.cursor",
        clients.cursor.enabled,
        &CursorAdapter::new(),
        None,
        DETECTION_ONLY,
        &[],
    )
    .await;
    doctor_source(
        "client.claude_code",
        clients.claude_code.enabled,
        &ClaudeCodeAdapter::new(clients.claude_code.command.clone()),
        Some("install Claude Code CLI or set clients.claude_code.command"),
        DETECTION_ONLY,
        &[],
    )
    .await;

    let antigravity = AntigravityAdapter::new(clients.antigravity.command.clone());
    let surfaces = if clients.antigravity.enabled {
        antigravity.probe_surfaces().await
    } else {
        Vec::new()
    };
    let surfaces_line: Vec<String> = (!surfaces.is_empty())
        .then(|| format!("surfaces: {}", surfaces.join(", ")))
        .into_iter()
        .collect();
    doctor_source(
        "client.antigravity",
        clients.antigravity.enabled,
        &antigravity,
        Some("install Antigravity CLI (agy), Antigravity IDE, or set GEMINI_API_KEY"),
        DETECTION_ONLY,
        &surfaces_line,
    )
    .await;

    doctor_source(
        "provider.openai",
        providers.openai.enabled,
        &OpenAiAdapter::new(),
        None,
        DETECTION_ONLY,
        &[],
    )
    .await;
    doctor_source(
        "provider.anthropic",
        providers.anthropic.enabled,
        &AnthropicAdapter::new(),
        None,
        DETECTION_ONLY,
        &[],
    )
    .await;
    doctor_source(
        "provider.xai",
        providers.xai.enabled,
        &XaiAdapter::new(),
        None,
        DETECTION_ONLY,
        &[],
    )
    .await;
    doctor_source(
        "provider.zai",
        providers.zai.enabled,
        &ZaiAdapter::new(),
        None,
        DETECTION_ONLY,
        &[],
    )
    .await;

    Ok(())
}

/// Prints one doctor block. `install_hint` marks binary-backed sources: it adds
/// the `binary:` line and the `action:` hint when the probe itself fails.
async fn doctor_source(
    name: &str,
    enabled: bool,
    source: &dyn BudgetSource,
    install_hint: Option<&str>,
    detection_only: bool,
    extra_lines: &[String],
) {
    println!("{name}");
    if !enabled {
        println!("  state: disabled");
        return;
    }
    if detection_only {
        println!("  data: unsupported (detection only; no verified quota surface yet)");
    }
    match source.probe().await {
        Ok(report) => {
            if install_hint.is_some() {
                println!("  binary: {}", probe_binary_name(&report.state));
            }
            println!("  state: {}", probe_state_name(&report.state));
            for line in extra_lines {
                println!("  {line}");
            }
        }
        Err(_) => {
            if let Some(hint) = install_hint {
                println!("  binary: not found");
                println!("  state: unavailable");
                println!("  action: {hint}");
            } else {
                println!("  state: unavailable");
            }
        }
    }
}

fn build_registry(config: &Config) -> Result<SourceRegistry, CliError> {
    let mut registry = SourceRegistry::new(Duration::from_secs(10));
    if config.clients.codex.enabled {
        registry
            .register(Arc::new(CodexAdapter::with_default_timeout(
                config.clients.codex.command.clone(),
            )))
            .map_err(|_| CliError::Registration)?;
    }
    if config.clients.opencode.enabled {
        registry
            .register(Arc::new(OpenCodeAdapter::with_default_timeout(
                config.clients.opencode.command.clone(),
            )))
            .map_err(|_| CliError::Registration)?;
    }
    if config.clients.copilot.enabled {
        registry
            .register(Arc::new(CopilotAdapter::with_default_timeout(
                config.clients.copilot.command.clone(),
            )))
            .map_err(|_| CliError::Registration)?;
    }
    if config.clients.cursor.enabled {
        registry
            .register(Arc::new(CursorAdapter::new()))
            .map_err(|_| CliError::Registration)?;
    }
    if config.clients.claude_code.enabled {
        registry
            .register(Arc::new(ClaudeCodeAdapter::new(
                config.clients.claude_code.command.clone(),
            )))
            .map_err(|_| CliError::Registration)?;
    }
    if config.clients.antigravity.enabled {
        registry
            .register(Arc::new(AntigravityAdapter::new(
                config.clients.antigravity.command.clone(),
            )))
            .map_err(|_| CliError::Registration)?;
    }
    if config.providers.openai.enabled {
        registry
            .register(Arc::new(OpenAiAdapter::new()))
            .map_err(|_| CliError::Registration)?;
    }
    if config.providers.anthropic.enabled {
        registry
            .register(Arc::new(AnthropicAdapter::new()))
            .map_err(|_| CliError::Registration)?;
    }
    if config.providers.xai.enabled {
        registry
            .register(Arc::new(XaiAdapter::new()))
            .map_err(|_| CliError::Registration)?;
    }
    if config.providers.zai.enabled {
        registry
            .register(Arc::new(ZaiAdapter::new()))
            .map_err(|_| CliError::Registration)?;
    }
    for snapshot in config.manual_snapshots()? {
        registry
            .register(Arc::new(ManualSource::new(snapshot)))
            .map_err(|_| CliError::Registration)?;
    }
    Ok(registry)
}

fn parse_operation(value: &str) -> Result<OperationKind, CliError> {
    let kind = match value {
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
        _ => return Err(CliError::Server("invalid_operation".into())),
    };
    Ok(kind)
}

fn parse_importance(value: &str) -> Result<OperationImportance, CliError> {
    match value {
        "required" => Ok(OperationImportance::Required),
        "useful" => Ok(OperationImportance::Useful),
        "optional" => Ok(OperationImportance::Optional),
        _ => Err(CliError::Server("invalid_request".into())),
    }
}
fn probe_binary_name(state: &fluxguard_runtime::ProbeState) -> &'static str {
    match state {
        fluxguard_runtime::ProbeState::Ready
        | fluxguard_runtime::ProbeState::UnsupportedVersion
        | fluxguard_runtime::ProbeState::Partial => "found",
        fluxguard_runtime::ProbeState::BinaryMissing => "not found",
        _ => "unavailable",
    }
}

fn probe_state_name(state: &fluxguard_runtime::ProbeState) -> &'static str {
    match state {
        fluxguard_runtime::ProbeState::Ready => "ready",
        fluxguard_runtime::ProbeState::BinaryMissing => "binary_missing",
        fluxguard_runtime::ProbeState::NotAuthenticated => "not_authenticated",
        fluxguard_runtime::ProbeState::UnsupportedVersion => "unsupported",
        fluxguard_runtime::ProbeState::Disabled => "disabled",
        fluxguard_runtime::ProbeState::Partial => "partial",
    }
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

fn state_name(state: &SourceStateKind) -> &'static str {
    match state {
        SourceStateKind::Pending => "pending",
        SourceStateKind::Ready => "ready",
        SourceStateKind::Failed => "failed",
        SourceStateKind::Stale => "stale",
        SourceStateKind::Shutdown => "shutdown",
    }
}
