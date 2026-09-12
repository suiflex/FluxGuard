use std::{
    fs,
    io::{self, IsTerminal, Write},
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

use crate::config::{Config, ConfigError};

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
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Check,
}

pub async fn run(cli: Cli) -> Result<(), CliError> {
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
        Command::Config {
            command: ConfigCommand::Check,
        } => {
            println!("configuration valid");
            Ok(())
        }
    }
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

fn choose_client() -> Result<String, CliError> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(CliError::InstallRequiresTty);
    }
    println!("FluxGuard MCP installation target:");
    for (index, client) in SUPPORTED_INSTALL_CLIENTS.iter().enumerate() {
        println!("  {}) {client}", index + 1);
    }
    print!("Select client [1-{}]: ", SUPPORTED_INSTALL_CLIENTS.len());
    io::stdout()
        .flush()
        .map_err(|_| CliError::ClientCommandFailed)?;
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|_| CliError::ClientCommandFailed)?;
    let index = input
        .trim()
        .parse::<usize>()
        .map_err(|_| CliError::UnsupportedClient(input.trim().into()))?;
    SUPPORTED_INSTALL_CLIENTS
        .get(index.saturating_sub(1))
        .map(|client| (*client).into())
        .ok_or_else(|| CliError::UnsupportedClient(input.trim().into()))
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
    println!("client.codex");
    if config.clients.codex.enabled {
        let codex = CodexAdapter::with_default_timeout(config.clients.codex.command);
        match codex.probe().await {
            Ok(report) => {
                println!("  binary: {}", probe_binary_name(&report.state));
                println!("  state: {}", probe_state_name(&report.state));
            }
            Err(_) => {
                println!("  binary: not found");
                println!("  state: unavailable");
                println!("  action: install Codex or set clients.codex.command");
            }
        }
    } else {
        println!("  state: disabled");
    }

    println!("client.opencode");
    if config.clients.opencode.enabled {
        let opencode = OpenCodeAdapter::with_default_timeout(config.clients.opencode.command);
        match opencode.probe().await {
            Ok(report) => {
                println!("  binary: {}", probe_binary_name(&report.state));
                println!("  state: {}", probe_state_name(&report.state));
            }
            Err(_) => {
                println!("  binary: not found");
                println!("  state: unavailable");
                println!("  action: install OpenCode or set clients.opencode.command");
            }
        }
    } else {
        println!("  state: disabled");
    }

    println!("client.copilot");
    if config.clients.copilot.enabled {
        let copilot = CopilotAdapter::with_default_timeout(config.clients.copilot.command);
        match copilot.probe().await {
            Ok(report) => {
                println!("  binary: {}", probe_binary_name(&report.state));
                println!("  state: {}", probe_state_name(&report.state));
            }
            Err(_) => {
                println!("  binary: not found");
                println!("  state: unavailable");
                println!(
                    "  action: install GitHub Copilot CLI (copilot) or set clients.copilot.command"
                );
            }
        }
    } else {
        println!("  state: disabled");
    }

    println!("client.cursor");
    if config.clients.cursor.enabled {
        let cursor = CursorAdapter::new();
        match cursor.probe().await {
            Ok(report) => {
                println!("  state: {}", probe_state_name(&report.state));
            }
            Err(_) => {
                println!("  state: unavailable");
            }
        }
    } else {
        println!("  state: disabled");
    }

    println!("client.claude_code");
    if config.clients.claude_code.enabled {
        let claude = ClaudeCodeAdapter::new(config.clients.claude_code.command);
        match claude.probe().await {
            Ok(report) => {
                println!("  binary: {}", probe_binary_name(&report.state));
                println!("  state: {}", probe_state_name(&report.state));
            }
            Err(_) => {
                println!("  binary: not found");
                println!("  state: unavailable");
                println!("  action: install Claude Code CLI or set clients.claude_code.command");
            }
        }
    } else {
        println!("  state: disabled");
    }

    println!("client.antigravity");
    if config.clients.antigravity.enabled {
        let agy = AntigravityAdapter::new(config.clients.antigravity.command);
        let surfaces = agy.probe_surfaces().await;
        match agy.probe().await {
            Ok(report) => {
                println!("  binary: {}", probe_binary_name(&report.state));
                println!("  state: {}", probe_state_name(&report.state));
                if !surfaces.is_empty() {
                    println!("  surfaces: {}", surfaces.join(", "));
                }
            }
            Err(_) => {
                println!("  binary: not found");
                println!("  state: unavailable");
                println!(
                    "  action: install Antigravity CLI (agy), Antigravity IDE, or set GEMINI_API_KEY"
                );
            }
        }
    } else {
        println!("  state: disabled");
    }

    println!("provider.openai");
    if config.providers.openai.enabled {
        let openai = OpenAiAdapter::new();
        match openai.probe().await {
            Ok(report) => println!("  state: {}", probe_state_name(&report.state)),
            Err(_) => println!("  state: unavailable"),
        }
    } else {
        println!("  state: disabled");
    }

    println!("provider.anthropic");
    if config.providers.anthropic.enabled {
        let anthropic = AnthropicAdapter::new();
        match anthropic.probe().await {
            Ok(report) => println!("  state: {}", probe_state_name(&report.state)),
            Err(_) => println!("  state: unavailable"),
        }
    } else {
        println!("  state: disabled");
    }

    println!("provider.xai");
    if config.providers.xai.enabled {
        let xai = XaiAdapter::new();
        match xai.probe().await {
            Ok(report) => println!("  state: {}", probe_state_name(&report.state)),
            Err(_) => println!("  state: unavailable"),
        }
    } else {
        println!("  state: disabled");
    }

    println!("provider.zai");
    if config.providers.zai.enabled {
        let zai = ZaiAdapter::new();
        match zai.probe().await {
            Ok(report) => println!("  state: {}", probe_state_name(&report.state)),
            Err(_) => println!("  state: unavailable"),
        }
    } else {
        println!("  state: disabled");
    }

    Ok(())
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
