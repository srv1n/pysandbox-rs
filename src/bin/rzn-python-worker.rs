use pysandbox::{
    ExecutionMode, ExecutionOptions, NativePythonEngine, PythonEngine, PythonSandbox,
    SandboxConfig, SandboxedPythonEngine, SecurityProfile,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const DEFAULT_ENVS_DIR_NAME: &str = "python_envs";
const ENV_METADATA_FILENAME: &str = "rzn_env.json";
const ENV_TOOL_TIMEOUT_SECS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PythonRuntime {
    Auto,
    Bundled,
    System,
}

impl PythonRuntime {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "bundled" => Some(Self::Bundled),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct WorkerConfig {
    plugin_dir: Option<PathBuf>,
    python_runtime: PythonRuntime,
    python_runtime_explicit: bool,
    python_path_override: Option<PathBuf>,
    sandbox_profile_path: Option<PathBuf>,
}

impl WorkerConfig {
    fn from_env_and_args() -> Self {
        let args: Vec<String> = std::env::args().collect();

        let plugin_dir = std::env::var("RZN_PLUGIN_DIR")
            .ok()
            .or_else(|| std::env::var("CLAUDE_PLUGIN_ROOT").ok())
            .map(PathBuf::from);

        let runtime_env = std::env::var("RZN_PYTHON_RUNTIME")
            .ok()
            .and_then(|v| PythonRuntime::parse(&v));
        let mut python_runtime = runtime_env.unwrap_or(PythonRuntime::Auto);
        let mut python_runtime_explicit = runtime_env.is_some();
        let mut python_path_override = std::env::var("RZN_PYTHON_PATH").ok().map(PathBuf::from);
        let mut sandbox_profile_path = std::env::var("RZN_PYTHON_SANDBOX_PROFILE")
            .ok()
            .map(PathBuf::from);

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--python-runtime" => {
                    if let Some(v) = args.get(i + 1) {
                        if let Some(rt) = PythonRuntime::parse(v) {
                            python_runtime = rt;
                            python_runtime_explicit = true;
                        }
                    }
                    i += 2;
                }
                "--python-path" => {
                    if let Some(v) = args.get(i + 1) {
                        python_path_override = Some(PathBuf::from(v));
                    }
                    i += 2;
                }
                "--sandbox-profile" => {
                    if let Some(v) = args.get(i + 1) {
                        sandbox_profile_path = Some(PathBuf::from(v));
                    }
                    i += 2;
                }
                _ => i += 1,
            }
        }

        Self {
            plugin_dir,
            python_runtime,
            python_runtime_explicit,
            python_path_override,
            sandbox_profile_path,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManagedEnvMetadata {
    alias: String,
    env_dir: String,
    python_path: String,
    base_python: Option<String>,
    created_at_unix_seconds: u64,
}

fn managed_envs_base_dir() -> PathBuf {
    if let Ok(v) = std::env::var("RZN_PYTHON_ENVS_DIR") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if let Ok(v) = std::env::var("RZN_APP_BASE_DIR") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(DEFAULT_ENVS_DIR_NAME);
        }
    }
    if let Ok(v) = std::env::var("HOME") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed)
                .join(".rzn")
                .join(DEFAULT_ENVS_DIR_NAME);
        }
    }
    std::env::temp_dir().join("rzn").join(DEFAULT_ENVS_DIR_NAME)
}

fn validate_env_alias(raw: &str) -> std::result::Result<String, Value> {
    let alias = raw.trim();
    if alias.is_empty() {
        return Err(json!({
            "code": -32602,
            "message": "python_env alias must be non-empty"
        }));
    }
    if alias.len() > 64 {
        return Err(json!({
            "code": -32602,
            "message": "python_env alias is too long (max 64 chars)"
        }));
    }
    if !alias
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(json!({
            "code": -32602,
            "message": "python_env alias may only contain [a-zA-Z0-9._-]"
        }));
    }
    Ok(alias.to_string())
}

fn managed_env_dir(alias: &str) -> PathBuf {
    managed_envs_base_dir().join(alias)
}

fn managed_env_python_path(env_dir: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        env_dir.join("Scripts").join("python.exe")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let python3 = env_dir.join("bin").join("python3");
        if python3.exists() {
            return python3;
        }
        env_dir.join("bin").join("python")
    }
}

fn metadata_path_for_env(env_dir: &Path) -> PathBuf {
    env_dir.join(ENV_METADATA_FILENAME)
}

fn read_env_metadata(env_dir: &Path) -> Option<ManagedEnvMetadata> {
    let metadata_path = metadata_path_for_env(env_dir);
    let raw = std::fs::read_to_string(metadata_path).ok()?;
    serde_json::from_str::<ManagedEnvMetadata>(&raw).ok()
}

fn write_env_metadata(
    alias: &str,
    env_dir: &Path,
    python_path: &Path,
    base_python: Option<&Path>,
) -> std::result::Result<ManagedEnvMetadata, Value> {
    let created_at_unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();

    let metadata = ManagedEnvMetadata {
        alias: alias.to_string(),
        env_dir: env_dir.to_string_lossy().to_string(),
        python_path: python_path.to_string_lossy().to_string(),
        base_python: base_python.map(|p| p.to_string_lossy().to_string()),
        created_at_unix_seconds,
    };

    let metadata_path = metadata_path_for_env(env_dir);
    let serialized = serde_json::to_string_pretty(&metadata).map_err(|e| {
        json!({
            "code": -32000,
            "message": format!("Failed to serialize env metadata: {}", e)
        })
    })?;
    std::fs::write(metadata_path, serialized).map_err(|e| {
        json!({
            "code": -32000,
            "message": format!("Failed to persist env metadata: {}", e)
        })
    })?;
    Ok(metadata)
}

fn parse_python_env_alias(args: &Value) -> std::result::Result<Option<String>, Value> {
    let from_args = args
        .get("python_env")
        .or_else(|| args.get("pythonEnv"))
        .or_else(|| args.get("env_alias"))
        .or_else(|| args.get("envAlias"))
        .and_then(|v| v.as_str())
        .map(validate_env_alias)
        .transpose()?;
    if from_args.is_some() {
        return Ok(from_args);
    }
    let from_env = std::env::var("RZN_PYTHON_ENV_ALIAS")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|s| validate_env_alias(&s))
        .transpose()?;
    Ok(from_env)
}

fn resolve_existing_managed_env(alias: &str) -> std::result::Result<(PathBuf, PathBuf), Value> {
    let env_dir = managed_env_dir(alias);
    if !env_dir.exists() {
        return Err(json!({
            "code": -32000,
            "message": format!("Managed python env '{}' not found", alias),
            "data": {
                "alias": alias,
                "env_dir": env_dir,
                "hint": "Use python_env.create first."
            }
        }));
    }
    let python_path = managed_env_python_path(&env_dir);
    if !python_path.exists() {
        return Err(json!({
            "code": -32000,
            "message": format!("Managed python env '{}' is missing interpreter", alias),
            "data": {
                "alias": alias,
                "env_dir": env_dir,
                "python_path": python_path
            }
        }));
    }
    Ok((env_dir, python_path))
}

async fn run_cmd_capture(
    cmd: &mut Command,
    timeout_secs: u64,
) -> std::result::Result<std::process::Output, Value> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(json!({
            "code": -32000,
            "message": format!("Process execution failed: {}", e)
        })),
        Err(_) => Err(json!({
            "code": -32000,
            "message": format!("Process timed out after {}s", timeout_secs)
        })),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "rzn_python_worker=info,pysandbox=info".to_string()),
        )
        .with_target(false)
        .init();

    let cfg = WorkerConfig::from_env_and_args();
    tracing::info!(
        "rzn-python-worker starting (plugin_dir={:?})",
        cfg.plugin_dir
    );

    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();
    let mut stdout = io::stdout();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("invalid json-rpc line: {} | err={}", line, e);
                continue;
            }
        };

        let response = handle_message(&cfg, parsed).await;
        if let Some(resp) = response {
            let s = serde_json::to_string(&resp)?;
            stdout.write_all(s.as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }

    Ok(())
}

async fn handle_message(cfg: &WorkerConfig, msg: Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or_else(|| json!({}));

    // Notifications have no id: ignore most, but allow "exit" to terminate.
    if id.is_none() {
        if method == "exit" {
            std::process::exit(0);
        }
        return None;
    }

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-06-18",
            "serverInfo": {
                "name": "RZN Python Tools Worker",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "listChanged": false },
                "prompts": { "listChanged": false },
                "experimental": {}
            }
        })),
        "tools/list" => Ok(json!({ "tools": tools_list() })),
        "tools/call" => call_tool(cfg, &params).await,
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "shutdown" => {
            std::process::exit(0);
        }
        _ => Err(json!({
            "code": -32601,
            "message": format!("Method not found: {}", method)
        })),
    };

    Some(match result {
        Ok(v) => json!({ "jsonrpc": "2.0", "result": v, "id": id }),
        Err(err_obj) => json!({ "jsonrpc": "2.0", "error": err_obj, "id": id }),
    })
}

fn tools_list() -> Vec<Value> {
    vec![
        json!({
            "name": "python_tools.ping",
            "description": "Health check for the Python Tools worker.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "rzn.worker.health",
            "description": "Standard RZN worker health probe (compat shim).",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "example.echo",
            "description": "Echo a string back (compat shim).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                },
                "required": ["text"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "python_env.list",
            "description": "List managed Python virtual environments available to this worker.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "python_env.create",
            "description": "Create (or recreate) an app-managed Python virtual environment for YOLO workflows.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "alias": { "type": "string", "description": "Environment alias ([a-zA-Z0-9._-], max 64 chars)." },
                    "python_path": { "type": "string", "description": "Optional base interpreter path used to create the venv." },
                    "recreate": { "type": "boolean", "description": "If true, delete and recreate an existing env alias." },
                    "without_pip": { "type": "boolean", "description": "If true, skip bootstrapping pip in the new venv." }
                },
                "required": ["alias"],
                "additionalProperties": true
            }
        }),
        json!({
            "name": "python_env.install",
            "description": "Install dependencies into a managed Python environment using pip.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "alias": { "type": "string", "description": "Environment alias to install into." },
                    "packages": {
                        "description": "Package targets for pip install (package names, wheel paths, or URLs).",
                        "oneOf": [
                            { "type": "array", "items": { "type": "string" } },
                            { "type": "string", "description": "Comma-separated package targets" }
                        ]
                    },
                    "requirements_file": { "type": "string", "description": "Optional requirements file path for pip -r." },
                    "upgrade": { "type": "boolean", "description": "If true, pass --upgrade to pip." },
                    "no_deps": { "type": "boolean", "description": "If true, pass --no-deps to pip." },
                    "index_url": { "type": "string", "description": "Optional pip --index-url value." },
                    "extra_index_url": { "type": "string", "description": "Optional pip --extra-index-url value." }
                },
                "required": ["alias"],
                "additionalProperties": true
            }
        }),
        json!({
            "name": "python_sandbox",
            "description": "Execute Python code with policy-selected sandboxing and runtime selection.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "Python code to execute. You may set a `result` variable to return structured data." },
                    "inputs": { "type": "object", "description": "JSON object available as `inputs` in the Python program." },
                    "policy_id": { "type": "string", "description": "Host-selected policy id (e.g. yolo, balanced, data_science, enterprise)." },
                    "policyId": { "type": "string", "description": "Alias of policy_id (legacy)." },
                    "python_env": { "type": "string", "description": "Managed env alias to run with (YOLO mode). Alias of pythonEnv/env_alias." },
                    "pythonEnv": { "type": "string", "description": "Alias of python_env (legacy camelCase)." },
                    "env_alias": { "type": "string", "description": "Alias of python_env (legacy snake_case)." },
                    "approved_folders": { "type": "array", "items": { "type": "string" }, "description": "Host-managed allowlist of approved folders (informational; OS boundary enforces in Secure mode)." },
                    "python_runtime": { "type": "string", "enum": ["auto","bundled","system"], "description": "Select which Python interpreter to use." },
                    "network_allowlist": {
                        "description": "Optional outbound host allowlist for runtime network access. Use exact hosts, wildcard suffix entries like '*.example.com', or '*'.",
                        "oneOf": [
                            { "type": "array", "items": { "type": "string" } },
                            { "type": "string", "description": "Comma-separated hosts" }
                        ]
                    },
                    "networkAllowlist": {
                        "description": "Alias of network_allowlist (legacy camelCase).",
                        "oneOf": [
                            { "type": "array", "items": { "type": "string" } },
                            { "type": "string", "description": "Comma-separated hosts" }
                        ]
                    },
                    "python_path": { "type": "string", "description": "Override Python executable path. Relative paths are resolved against RZN_PLUGIN_DIR when present." },
                    "execution_mode": { "type": "string", "enum": ["native","workspace_isolated","platform_sandboxed"], "description": "Override execution mode. If omitted, derived from policy_id." },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": 600, "description": "Wall-clock timeout for the run." }
                },
                "required": ["code"],
                "additionalProperties": true
            }
        }),
    ]
}

async fn call_tool(cfg: &WorkerConfig, params: &Value) -> std::result::Result<Value, Value> {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    match normalize_tool_name(name).as_str() {
        "python_tools_ping" => Ok(json!({
            "content": [{ "type": "text", "text": "pong" }],
            "isError": false,
        })),
        "rzn_worker_health" => Ok(json!({
            "content": [{ "type": "text", "text": "ok" }],
            "structuredContent": {
                "ok": true,
                "worker": "rzn-python-worker",
                "version": env!("CARGO_PKG_VERSION"),
                "plugin_dir": cfg.plugin_dir,
            },
            "isError": false
        })),
        "example_echo" => Ok(json!({
            "content": [{
                "type": "text",
                "text": args.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string()
            }],
            "isError": false
        })),
        "python_env_list" => python_env_list_call(&args).await,
        "python_env_create" => python_env_create_call(cfg, &args).await,
        "python_env_install" => python_env_install_call(&args).await,
        "python_sandbox" => python_sandbox_call(cfg, &args).await,
        other => Ok(json!({
            "content": [{ "type": "text", "text": format!("Unknown tool: {}", other) }],
            "isError": true
        })),
    }
}

fn normalize_tool_name(name: &str) -> String {
    name.replace('.', "_").replace('/', "_")
}

fn policy_id_from_args(args: &Value) -> String {
    args.get("policy_id")
        .or_else(|| args.get("policyId"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("RZN_PYTHON_POLICY_ID")
                .ok()
                .map(|s| s.trim().to_ascii_lowercase())
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "balanced".to_string())
}

fn map_policy_to_profile(policy_id: &str) -> SecurityProfile {
    match policy_id {
        "yolo" => SecurityProfile::Yolo,
        "enterprise" => SecurityProfile::Strict,
        "data_science" | "document_processing" => SecurityProfile::DataScience,
        "balanced" => SecurityProfile::Blacklist,
        _ => SecurityProfile::Blacklist,
    }
}

fn map_policy_to_execution_mode(policy_id: &str) -> ExecutionMode {
    match policy_id {
        "enterprise" | "data_science" | "document_processing" => ExecutionMode::WorkspaceIsolated,
        _ => ExecutionMode::Native,
    }
}

fn execution_mode_from_args(args: &Value, policy_id: &str) -> ExecutionMode {
    let override_str = args
        .get("execution_mode")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_ascii_lowercase());
    match override_str.as_deref() {
        Some("native") => ExecutionMode::Native,
        Some("workspace_isolated") | Some("workspace-isolated") => ExecutionMode::WorkspaceIsolated,
        Some("platform_sandboxed") | Some("platform-sandboxed") => ExecutionMode::PlatformSandboxed,
        _ => map_policy_to_execution_mode(policy_id),
    }
}

fn runtime_override_from_args(args: &Value) -> Option<PythonRuntime> {
    args.get("python_runtime")
        .or_else(|| args.get("pythonRuntime"))
        .and_then(|v| v.as_str())
        .and_then(PythonRuntime::parse)
}

fn effective_python_runtime(cfg: &WorkerConfig, args: &Value, policy_id: &str) -> PythonRuntime {
    if let Some(rt) = runtime_override_from_args(args) {
        return rt;
    }
    if cfg.python_runtime_explicit {
        return cfg.python_runtime;
    }
    if cfg.python_runtime == PythonRuntime::Auto && policy_id == "yolo" {
        return PythonRuntime::System;
    }
    cfg.python_runtime
}

fn parse_network_allowlist(args: &Value) -> std::result::Result<Option<Vec<String>>, Value> {
    fn normalize(values: impl IntoIterator<Item = String>) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for value in values {
            let candidate = value.trim().to_ascii_lowercase();
            if candidate.is_empty() {
                continue;
            }
            if seen.insert(candidate.clone()) {
                out.push(candidate);
            }
        }
        out
    }

    fn parse_value(value: &Value) -> std::result::Result<Vec<String>, Value> {
        match value {
            Value::String(s) => Ok(normalize(
                s.split(',')
                    .map(|part| part.trim().to_string())
                    .collect::<Vec<_>>(),
            )),
            Value::Array(items) => {
                let mut hosts = Vec::new();
                for item in items {
                    let host = item.as_str().ok_or_else(|| {
                        json!({
                            "code": -32602,
                            "message": "Invalid network_allowlist entry: all entries must be strings"
                        })
                    })?;
                    hosts.push(host.to_string());
                }
                Ok(normalize(hosts))
            }
            _ => Err(json!({
                "code": -32602,
                "message": "Invalid network_allowlist: expected array of strings or comma-separated string"
            })),
        }
    }

    if let Some(v) = args
        .get("network_allowlist")
        .or_else(|| args.get("networkAllowlist"))
    {
        let hosts = parse_value(v)?;
        return Ok(if hosts.is_empty() { None } else { Some(hosts) });
    }

    if let Ok(raw) = std::env::var("RZN_PYTHON_NETWORK_ALLOWLIST") {
        let hosts = normalize(raw.split(',').map(|part| part.trim().to_string()));
        if !hosts.is_empty() {
            return Ok(Some(hosts));
        }
    }

    Ok(None)
}

fn env_alias_from_args(args: &Value) -> std::result::Result<String, Value> {
    let alias = args
        .get("alias")
        .and_then(|v| v.as_str())
        .ok_or_else(|| json!({ "code": -32602, "message": "Missing required argument: alias" }))?;
    validate_env_alias(alias)
}

fn parse_string_list(args: &Value, key: &str) -> std::result::Result<Vec<String>, Value> {
    let Some(value) = args.get(key) else {
        return Ok(Vec::new());
    };
    match value {
        Value::String(s) => Ok(s
            .split(',')
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect()),
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                let s = item.as_str().ok_or_else(|| {
                    json!({
                        "code": -32602,
                        "message": format!("Invalid '{}': expected string entries", key)
                    })
                })?;
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    out.push(trimmed.to_string());
                }
            }
            Ok(out)
        }
        _ => Err(json!({
            "code": -32602,
            "message": format!("Invalid '{}': expected string or array of strings", key)
        })),
    }
}

fn parse_bool(args: &Value, key: &str, default: bool) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

fn parse_optional_string(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn python_env_list_call(args: &Value) -> std::result::Result<Value, Value> {
    let include_broken = parse_bool(args, "include_broken", true);
    let base_dir = managed_envs_base_dir();
    std::fs::create_dir_all(&base_dir).map_err(|e| {
        json!({
            "code": -32000,
            "message": format!("Failed to ensure env base dir exists: {}", e),
            "data": { "envs_dir": base_dir }
        })
    })?;

    let mut envs = Vec::<Value>::new();
    let entries = std::fs::read_dir(&base_dir).map_err(|e| {
        json!({
            "code": -32000,
            "message": format!("Failed to read env base dir: {}", e),
            "data": { "envs_dir": base_dir }
        })
    })?;

    for entry in entries {
        let entry = match entry {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let alias = entry.file_name().to_string_lossy().to_string();
        let env_dir = entry.path();
        let python_path = managed_env_python_path(&env_dir);
        let python_exists = python_path.exists();
        if !python_exists && !include_broken {
            continue;
        }
        let metadata = read_env_metadata(&env_dir);
        envs.push(json!({
            "alias": alias,
            "env_dir": env_dir,
            "python_path": python_path,
            "healthy": python_exists,
            "metadata": metadata,
        }));
    }
    envs.sort_by(|a, b| {
        let ak = a.get("alias").and_then(|v| v.as_str()).unwrap_or("");
        let bk = b.get("alias").and_then(|v| v.as_str()).unwrap_or("");
        ak.cmp(bk)
    });

    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("{} managed python env(s)", envs.len())
        }],
        "structuredContent": {
            "envs_dir": base_dir,
            "envs": envs,
        },
        "isError": false
    }))
}

async fn python_env_create_call(
    cfg: &WorkerConfig,
    args: &Value,
) -> std::result::Result<Value, Value> {
    let alias = env_alias_from_args(args)?;
    let recreate = parse_bool(args, "recreate", false);
    let without_pip = parse_bool(args, "without_pip", false);

    let envs_dir = managed_envs_base_dir();
    std::fs::create_dir_all(&envs_dir).map_err(|e| {
        json!({
            "code": -32000,
            "message": format!("Failed to create env base dir: {}", e),
            "data": { "envs_dir": envs_dir }
        })
    })?;

    let env_dir = managed_env_dir(&alias);
    if env_dir.exists() {
        if recreate {
            std::fs::remove_dir_all(&env_dir).map_err(|e| {
                json!({
                    "code": -32000,
                    "message": format!("Failed to remove existing env for recreate: {}", e),
                    "data": { "alias": alias, "env_dir": env_dir }
                })
            })?;
        } else {
            let (_, python_path) = resolve_existing_managed_env(&alias)?;
            let metadata = read_env_metadata(&env_dir);
            return Ok(json!({
                "content": [{
                    "type": "text",
                    "text": format!("Managed env '{}' already exists", alias)
                }],
                "structuredContent": {
                    "alias": alias,
                    "env_dir": env_dir,
                    "python_path": python_path,
                    "created": false,
                    "metadata": metadata,
                },
                "isError": false
            }));
        }
    }

    let python_path_override = parse_optional_string(args, "python_path")
        .or_else(|| parse_optional_string(args, "pythonPath"));
    let base_python = if let Some(raw) = python_path_override {
        resolve_maybe_relative(cfg.plugin_dir.as_deref(), Path::new(&raw))
    } else {
        which::which("python3")
            .or_else(|_| which::which("python"))
            .map_err(|_| {
                json!({
                    "code": -32000,
                    "message": "Python not found in PATH for venv creation"
                })
            })?
    };

    let mut create_cmd = Command::new(&base_python);
    create_cmd.arg("-m").arg("venv").arg(&env_dir);
    if without_pip {
        create_cmd.arg("--without-pip");
    }
    let create_output = run_cmd_capture(&mut create_cmd, ENV_TOOL_TIMEOUT_SECS).await?;
    if !create_output.status.success() {
        let stderr = String::from_utf8_lossy(&create_output.stderr);
        return Err(json!({
            "code": -32000,
            "message": format!("Failed to create managed env '{}'", alias),
            "data": {
                "stderr": stderr,
                "stdout": String::from_utf8_lossy(&create_output.stdout),
                "base_python": base_python,
                "env_dir": env_dir
            }
        }));
    }

    let python_path = managed_env_python_path(&env_dir);
    if !python_path.exists() {
        return Err(json!({
            "code": -32000,
            "message": "Venv created but python interpreter is missing",
            "data": { "alias": alias, "env_dir": env_dir, "python_path": python_path }
        }));
    }

    if !without_pip {
        let mut pip_check_cmd = Command::new(&python_path);
        pip_check_cmd.arg("-m").arg("pip").arg("--version");
        let pip_check = run_cmd_capture(&mut pip_check_cmd, 60).await?;
        if !pip_check.status.success() {
            let mut ensurepip_cmd = Command::new(&python_path);
            ensurepip_cmd.arg("-m").arg("ensurepip").arg("--upgrade");
            let ensurepip_output = run_cmd_capture(&mut ensurepip_cmd, 120).await?;
            if !ensurepip_output.status.success() {
                let stderr = String::from_utf8_lossy(&ensurepip_output.stderr);
                return Err(json!({
                    "code": -32000,
                    "message": format!("Managed env '{}' created but pip setup failed", alias),
                    "data": {
                        "stderr": stderr,
                        "stdout": String::from_utf8_lossy(&ensurepip_output.stdout),
                        "python_path": python_path
                    }
                }));
            }
        }
    }

    let metadata = write_env_metadata(&alias, &env_dir, &python_path, Some(&base_python))?;
    Ok(json!({
        "content": [{
            "type": "text",
            "text": format!("Created managed env '{}'", alias)
        }],
        "structuredContent": {
            "alias": alias,
            "env_dir": env_dir,
            "python_path": python_path,
            "base_python": base_python,
            "created": true,
            "metadata": metadata,
        },
        "isError": false
    }))
}

async fn python_env_install_call(args: &Value) -> std::result::Result<Value, Value> {
    let alias = env_alias_from_args(args)?;
    let (env_dir, python_path) = resolve_existing_managed_env(&alias)?;

    let mut package_targets = parse_string_list(args, "packages")?;
    if package_targets.is_empty() {
        package_targets = parse_string_list(args, "package")?;
    }

    let requirements_file = parse_optional_string(args, "requirements_file")
        .or_else(|| parse_optional_string(args, "requirementsFile"));
    let upgrade = parse_bool(args, "upgrade", false);
    let no_deps = parse_bool(args, "no_deps", false);
    let index_url = parse_optional_string(args, "index_url");
    let extra_index_url = parse_optional_string(args, "extra_index_url");

    if package_targets.is_empty() && requirements_file.is_none() {
        return Err(json!({
            "code": -32602,
            "message": "python_env.install requires packages or requirements_file"
        }));
    }

    let mut cmd = Command::new(&python_path);
    cmd.arg("-m").arg("pip").arg("install");
    if upgrade {
        cmd.arg("--upgrade");
    }
    if no_deps {
        cmd.arg("--no-deps");
    }
    if let Some(index_url) = &index_url {
        cmd.arg("--index-url").arg(index_url);
    }
    if let Some(extra_index_url) = &extra_index_url {
        cmd.arg("--extra-index-url").arg(extra_index_url);
    }
    if let Some(req) = requirements_file {
        let req_path = resolve_maybe_relative(None, Path::new(&req));
        cmd.arg("-r").arg(req_path);
    }
    for package in &package_targets {
        cmd.arg(package);
    }

    let args_for_result: Vec<String> = cmd
        .as_std()
        .get_args()
        .map(|s| s.to_string_lossy().to_string())
        .collect();

    let output = run_cmd_capture(&mut cmd, ENV_TOOL_TIMEOUT_SECS).await?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Ok(json!({
            "content": [{ "type": "text", "text": format!("pip install failed for env '{}'", alias) }],
            "structuredContent": {
                "alias": alias,
                "env_dir": env_dir,
                "python_path": python_path,
                "ok": false,
                "command_args": args_for_result,
                "stdout": stdout,
                "stderr": stderr
            },
            "isError": true
        }));
    }

    let metadata = read_env_metadata(&env_dir);
    Ok(json!({
        "content": [{ "type": "text", "text": format!("Installed dependencies in env '{}'", alias) }],
        "structuredContent": {
            "alias": alias,
            "env_dir": env_dir,
            "python_path": python_path,
            "ok": true,
            "command_args": args_for_result,
            "stdout": stdout,
            "stderr": stderr,
            "metadata": metadata,
        },
        "isError": false
    }))
}

fn resolve_python_path(
    cfg: &WorkerConfig,
    runtime: PythonRuntime,
    python_path_override: Option<&str>,
) -> std::result::Result<(Option<PathBuf>, Value), Value> {
    // 1) If explicit path is provided, use it (relative to plugin dir when possible).
    if let Some(path_raw) = python_path_override {
        let path = resolve_maybe_relative(cfg.plugin_dir.as_deref(), Path::new(path_raw));
        return Ok((
            Some(path.clone()),
            json!({ "kind": "explicit", "path": path }),
        ));
    }
    if let Some(path) = &cfg.python_path_override {
        return Ok((
            Some(path.clone()),
            json!({ "kind": "explicit_env", "path": path }),
        ));
    }

    // 2) Resolve bundled python from plugin dir/resources.
    let bundled = cfg
        .plugin_dir
        .as_deref()
        .and_then(|dir| bundled_python_path(dir));

    match runtime {
        PythonRuntime::System => Ok((None, json!({ "kind": "system" }))),
        PythonRuntime::Bundled => {
            let Some(path) = bundled else {
                return Err(json!({
                    "code": -32000,
                    "message": "Bundled Python not found (resources/python/...).",
                    "data": { "plugin_dir": cfg.plugin_dir }
                }));
            };
            Ok((
                Some(path.clone()),
                json!({ "kind": "bundled", "path": path }),
            ))
        }
        PythonRuntime::Auto => {
            if let Some(path) = bundled {
                Ok((
                    Some(path.clone()),
                    json!({ "kind": "bundled", "path": path }),
                ))
            } else {
                Ok((None, json!({ "kind": "system" })))
            }
        }
    }
}

fn bundled_python_path(plugin_dir: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let p = plugin_dir.join("resources/python/python.exe");
        if p.exists() {
            return Some(p);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        for cand in [
            plugin_dir.join("resources/python/bin/python3"),
            plugin_dir.join("resources/python/bin/python"),
        ] {
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    None
}

fn resolve_maybe_relative(base: Option<&Path>, p: &Path) -> PathBuf {
    if p.is_absolute() {
        return p.to_path_buf();
    }
    if let Some(b) = base {
        return b.join(p);
    }
    p.to_path_buf()
}

async fn python_sandbox_call(
    cfg: &WorkerConfig,
    args: &Value,
) -> std::result::Result<Value, Value> {
    let code = args
        .get("code")
        .and_then(|v| v.as_str())
        .ok_or_else(|| json!({ "code": -32602, "message": "Missing required argument: code" }))?;
    let inputs = args.get("inputs").cloned().unwrap_or_else(|| json!({}));

    let policy_id = policy_id_from_args(args);
    let security_profile = map_policy_to_profile(&policy_id);
    let execution_mode = execution_mode_from_args(args, &policy_id);

    let timeout_seconds = args
        .get("timeout_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(30)
        .clamp(1, 600);

    let managed_env_alias = parse_python_env_alias(args)?;
    if managed_env_alias.is_some() && policy_id != "yolo" {
        return Err(json!({
            "code": -32602,
            "message": "python_env is only allowed with policy_id=yolo",
            "data": { "policy_id": policy_id }
        }));
    }

    let runtime = effective_python_runtime(cfg, args, &policy_id);
    let python_path_override = parse_optional_string(args, "python_path")
        .or_else(|| parse_optional_string(args, "pythonPath"));
    let (python_path_opt, python_resolution) = if let Some(alias) = managed_env_alias.as_ref() {
        let (env_dir, python_path) = resolve_existing_managed_env(alias)?;
        (
            Some(python_path.clone()),
            json!({
                "kind": "managed_env",
                "alias": alias,
                "env_dir": env_dir,
                "path": python_path
            }),
        )
    } else {
        resolve_python_path(cfg, runtime, python_path_override.as_deref())?
    };
    let network_allowlist = parse_network_allowlist(args)?;

    let limits = security_profile.resource_limits();

    let engine: Box<dyn PythonEngine> = match (execution_mode, python_path_opt) {
        (ExecutionMode::Native, Some(p)) => Box::new(
            NativePythonEngine::with_python_path_and_limits(p, limits.clone())
                .map_err(|e| json!({ "code": -32000, "message": e.to_string() }))?,
        ),
        (ExecutionMode::Native, None) => Box::new(
            NativePythonEngine::with_limits(limits.clone())
                .map_err(|e| json!({ "code": -32000, "message": e.to_string() }))?,
        ),
        (ExecutionMode::WorkspaceIsolated | ExecutionMode::PlatformSandboxed, Some(p)) => {
            let config = SandboxConfig {
                python_path: p,
                sandbox_profile: cfg.sandbox_profile_path.clone(),
                limits: limits.clone(),
                ..Default::default()
            };
            Box::new(
                SandboxedPythonEngine::new(config)
                    .map_err(|e| json!({ "code": -32000, "message": e.to_string() }))?,
            )
        }
        (ExecutionMode::WorkspaceIsolated | ExecutionMode::PlatformSandboxed, None) => {
            // If we're in "system" mode, resolve a concrete python executable path.
            let sys_path = which::which("python3")
                .or_else(|_| which::which("python"))
                .map_err(|_| {
                    json!({
                        "code": -32000,
                        "message": "Python not found in PATH",
                        "data": { "runtime": "system" }
                    })
                })?;
            let config = SandboxConfig {
                python_path: sys_path,
                sandbox_profile: cfg.sandbox_profile_path.clone(),
                limits: limits.clone(),
                ..Default::default()
            };
            Box::new(
                SandboxedPythonEngine::new(config)
                    .map_err(|e| json!({ "code": -32000, "message": e.to_string() }))?,
            )
        }
    };

    let sandbox = PythonSandbox::new(vec![engine]);
    let options = ExecutionOptions {
        timeout: std::time::Duration::from_secs(timeout_seconds),
        import_policy: security_profile.to_import_policy(),
        network_allowlist: network_allowlist.clone(),
        env_vars: HashMap::new(),
        ..Default::default()
    };

    let exec = sandbox.execute(code, inputs, options).await;

    match exec {
        Ok(payload) => {
            let summary = summarize_payload(&payload);
            Ok(json!({
                "content": [{ "type": "text", "text": summary }],
                "structuredContent": {
                    "policy_id": policy_id,
                    "security_profile": format!("{:?}", security_profile).to_ascii_lowercase(),
                    "execution_mode": format!("{:?}", execution_mode).to_ascii_lowercase(),
                    "python": python_resolution,
                    "runtime": format!("{:?}", runtime).to_ascii_lowercase(),
                    "python_env": managed_env_alias.clone(),
                    "network_allowlist": network_allowlist,
                    "output": payload
                },
                "metadata": {
                    "policy_id": policy_id,
                    "execution_mode": format!("{:?}", execution_mode),
                    "python": python_resolution,
                    "runtime": format!("{:?}", runtime),
                    "python_env": managed_env_alias.clone(),
                },
                "isError": false
            }))
        }
        Err(e) => Ok(json!({
            "content": [{ "type": "text", "text": format!("error: {}", e) }],
            "structuredContent": {
                "policy_id": policy_id,
                "python": python_resolution,
                "error": e.to_string()
            },
            "isError": true
        })),
    }
}

fn summarize_payload(payload: &Value) -> String {
    let mut out = String::new();
    if let Some(stdout) = payload.get("stdout").and_then(|v| v.as_str()) {
        if !stdout.trim().is_empty() {
            out.push_str("stdout:\n");
            out.push_str(stdout.trim_end());
            out.push('\n');
        }
    }
    if let Some(stderr) = payload.get("stderr").and_then(|v| v.as_str()) {
        if !stderr.trim().is_empty() {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str("stderr:\n");
            out.push_str(stderr.trim_end());
            out.push('\n');
        }
    }
    if out.is_empty() {
        if let Some(result) = payload.get("result") {
            out.push_str("result:\n");
            out.push_str(&result.to_string());
        } else {
            out.push_str("ok");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_cfg(runtime: PythonRuntime, explicit: bool) -> WorkerConfig {
        WorkerConfig {
            plugin_dir: None,
            python_runtime: runtime,
            python_runtime_explicit: explicit,
            python_path_override: None,
            sandbox_profile_path: None,
        }
    }

    #[test]
    fn env_alias_validation_enforces_charset() {
        assert!(validate_env_alias("team-alpha_1").is_ok());
        assert!(validate_env_alias("bad alias").is_err());
        assert!(validate_env_alias("../escape").is_err());
    }

    #[test]
    fn network_allowlist_parses_and_normalizes() {
        let parsed = parse_network_allowlist(&json!({
            "network_allowlist": [" API.OpenAI.com ", "*.GitHub.com", "api.openai.com"]
        }))
        .unwrap();
        assert_eq!(
            parsed,
            Some(vec![
                "api.openai.com".to_string(),
                "*.github.com".to_string()
            ])
        );
    }

    #[test]
    fn yolo_auto_defaults_to_system_when_not_explicit() {
        let cfg = mk_cfg(PythonRuntime::Auto, false);
        let runtime = effective_python_runtime(&cfg, &json!({}), "yolo");
        assert_eq!(runtime, PythonRuntime::System);
    }

    #[test]
    fn explicit_runtime_is_respected_for_yolo() {
        let cfg = mk_cfg(PythonRuntime::Bundled, true);
        let runtime = effective_python_runtime(&cfg, &json!({}), "yolo");
        assert_eq!(runtime, PythonRuntime::Bundled);
    }

    #[test]
    fn policy_id_is_case_normalized() {
        let policy = policy_id_from_args(&json!({ "policy_id": "YOLO" }));
        assert_eq!(policy, "yolo");
    }
}
