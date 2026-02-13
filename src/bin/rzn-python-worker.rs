use pysandbox::{
    ExecutionMode, ExecutionOptions, NativePythonEngine, PythonEngine, PythonSandbox, Result,
    SandboxConfig, SandboxedPythonEngine, SecurityProfile,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};

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

        let mut python_runtime = std::env::var("RZN_PYTHON_RUNTIME")
            .ok()
            .and_then(|v| PythonRuntime::parse(&v))
            .unwrap_or(PythonRuntime::Auto);
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
            python_path_override,
            sandbox_profile_path,
        }
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
            "name": "python_sandbox",
            "description": "Execute Python code with policy-selected sandboxing and runtime selection.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "description": "Python code to execute. You may set a `result` variable to return structured data." },
                    "inputs": { "type": "object", "description": "JSON object available as `inputs` in the Python program." },
                    "policy_id": { "type": "string", "description": "Host-selected policy id (e.g. yolo, balanced, data_science, enterprise)." },
                    "policyId": { "type": "string", "description": "Alias of policy_id (legacy)." },
                    "approved_folders": { "type": "array", "items": { "type": "string" }, "description": "Host-managed allowlist of approved folders (informational; OS boundary enforces in Secure mode)." },
                    "python_runtime": { "type": "string", "enum": ["auto","bundled","system"], "description": "Select which Python interpreter to use." },
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
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("RZN_PYTHON_POLICY_ID").ok())
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

fn python_runtime_from_args(default_rt: PythonRuntime, args: &Value) -> PythonRuntime {
    args.get("python_runtime")
        .or_else(|| args.get("pythonRuntime"))
        .and_then(|v| v.as_str())
        .and_then(PythonRuntime::parse)
        .unwrap_or(default_rt)
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

    let runtime = python_runtime_from_args(cfg.python_runtime, args);
    let python_path_override = args.get("python_path").and_then(|v| v.as_str());
    let (python_path_opt, python_resolution) =
        resolve_python_path(cfg, runtime, python_path_override)?;

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
                    "output": payload
                },
                "metadata": {
                    "policy_id": policy_id,
                    "execution_mode": format!("{:?}", execution_mode),
                    "python": python_resolution,
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
