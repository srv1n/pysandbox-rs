use crate::{
    config::{ImportPolicy, ResourceLimits},
    engine::{EngineCapabilities, ExecutionOptions, PythonEngine},
    errors::{Result, SandboxError},
};
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{error, info, warn};

/// Configuration for the sandboxed Python engine
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Path to the Python executable
    pub python_path: PathBuf,
    /// Path to the sandbox profile file (macOS .sb file)
    pub sandbox_profile: Option<PathBuf>,
    /// Base directory for creating isolated workspaces
    pub workspace_base: PathBuf,
    /// Resource limits
    pub limits: ResourceLimits,
    /// Files to copy into the workspace before execution
    pub input_files: Vec<(PathBuf, String)>, // (source_path, workspace_name)
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            python_path: PathBuf::from("python3"),
            sandbox_profile: None,
            workspace_base: std::env::temp_dir().join("pysandbox-workspaces"),
            limits: ResourceLimits::default(),
            input_files: Vec::new(),
        }
    }
}

/// A workspace-isolated execution context
pub struct IsolatedWorkspace {
    /// Path to the workspace directory
    pub path: PathBuf,
    /// Whether to clean up on drop
    cleanup_on_drop: bool,
}

impl IsolatedWorkspace {
    /// Create a new isolated workspace
    pub fn new(base: &PathBuf) -> Result<Self> {
        let id = uuid::Uuid::new_v4().to_string();
        let path = base.join(&id);
        std::fs::create_dir_all(&path)?;

        // Create subdirectories
        std::fs::create_dir_all(path.join("input"))?;
        std::fs::create_dir_all(path.join("output"))?;

        info!("[SANDBOX] Created workspace: {:?}", path);

        Ok(Self {
            path,
            cleanup_on_drop: true,
        })
    }

    /// Get the input directory path
    pub fn input_dir(&self) -> PathBuf {
        self.path.join("input")
    }

    /// Get the output directory path
    pub fn output_dir(&self) -> PathBuf {
        self.path.join("output")
    }

    /// Copy a file into the workspace input directory
    pub fn copy_input(&self, source: &PathBuf, name: &str) -> Result<PathBuf> {
        let dest = self.input_dir().join(name);
        std::fs::copy(source, &dest)?;
        info!("[SANDBOX] Copied input file: {:?} -> {:?}", source, dest);
        Ok(dest)
    }

    /// Copy a file from the workspace output directory
    pub fn copy_output(&self, name: &str, dest: &PathBuf) -> Result<()> {
        let source = self.output_dir().join(name);
        if source.exists() {
            std::fs::copy(&source, dest)?;
            info!("[SANDBOX] Copied output file: {:?} -> {:?}", source, dest);
        } else {
            warn!("[SANDBOX] Output file not found: {:?}", source);
        }
        Ok(())
    }

    /// List all files in the output directory
    pub fn list_outputs(&self) -> Result<Vec<String>> {
        let mut outputs = Vec::new();
        for entry in std::fs::read_dir(self.output_dir())? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                outputs.push(name.to_string());
            }
        }
        Ok(outputs)
    }

    /// Keep the workspace (don't cleanup on drop)
    pub fn keep(&mut self) {
        self.cleanup_on_drop = false;
    }
}

impl Drop for IsolatedWorkspace {
    fn drop(&mut self) {
        if self.cleanup_on_drop {
            if let Err(e) = std::fs::remove_dir_all(&self.path) {
                warn!(
                    "[SANDBOX] Failed to cleanup workspace {:?}: {}",
                    self.path, e
                );
            } else {
                info!("[SANDBOX] Cleaned up workspace: {:?}", self.path);
            }
        }
    }
}

/// Platform-sandboxed Python engine with workspace isolation
///
/// On macOS: Uses sandbox-exec with a restrictive profile
/// On Windows: Uses Job Objects + Restricted Token (TODO)
/// On Linux: Uses seccomp/bubblewrap (TODO)
pub struct SandboxedPythonEngine {
    config: SandboxConfig,
}

impl SandboxedPythonEngine {
    /// Create a new sandboxed Python engine
    pub fn new(config: SandboxConfig) -> Result<Self> {
        // Verify Python exists
        if !config.python_path.exists() {
            return Err(SandboxError::PythonNotFound);
        }

        // Ensure workspace base exists
        std::fs::create_dir_all(&config.workspace_base)?;

        Ok(Self { config })
    }

    /// Create with just a Python path (uses defaults for everything else)
    pub fn with_python_path(python_path: PathBuf) -> Result<Self> {
        Self::new(SandboxConfig {
            python_path,
            ..Default::default()
        })
    }

    /// Create with Python path and sandbox profile
    pub fn with_python_and_profile(python_path: PathBuf, sandbox_profile: PathBuf) -> Result<Self> {
        Self::new(SandboxConfig {
            python_path,
            sandbox_profile: Some(sandbox_profile),
            ..Default::default()
        })
    }

    /// Generate import control code based on policy
    fn generate_import_control(&self, policy: &ImportPolicy) -> String {
        // Same implementation as NativePythonEngine
        match policy {
            ImportPolicy::Blacklist(blacklist) => {
                let blacklist_str = if blacklist.is_empty() {
                    "set()".to_string()
                } else {
                    format!(
                        "{{{}}}",
                        blacklist
                            .iter()
                            .map(|s| format!("'{}'", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!(
                    r#"
import builtins
import sys

BLACKLIST = {blacklist}

original_import = builtins.__import__

def safe_import(name, globals=None, locals=None, fromlist=(), level=0):
    if level > 0:
        return original_import(name, globals, locals, fromlist, level)
    root_module = name.split('.')[0]
    if root_module in BLACKLIST:
        raise ImportError(f"Module '{{root_module}}' is blacklisted for safety")
    return original_import(name, globals, locals, fromlist, level)

builtins.__import__ = safe_import
"#,
                    blacklist = blacklist_str
                )
            }
            ImportPolicy::Whitelist(whitelist) => {
                let whitelist_str = if whitelist.is_empty() {
                    "set()".to_string()
                } else {
                    format!(
                        "{{{}}}",
                        whitelist
                            .iter()
                            .map(|s| format!("'{}'", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!(
                    r#"
import builtins
import sys
import json
import re

WHITELIST = {whitelist}

original_import = builtins.__import__

def safe_import(name, globals=None, locals=None, fromlist=(), level=0):
    if level > 0:
        return original_import(name, globals, locals, fromlist, level)
    root_module = name.split('.')[0]
    if root_module not in WHITELIST and root_module != 'builtins':
        raise ImportError(f"Module '{{root_module}}' is not in whitelist")
    return original_import(name, globals, locals, fromlist, level)

builtins.__import__ = safe_import
"#,
                    whitelist = whitelist_str
                )
            }
            ImportPolicy::Both {
                whitelist,
                blacklist,
            } => {
                let whitelist_str = if whitelist.is_empty() {
                    "set()".to_string()
                } else {
                    format!(
                        "{{{}}}",
                        whitelist
                            .iter()
                            .map(|s| format!("'{}'", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                let blacklist_str = if blacklist.is_empty() {
                    "set()".to_string()
                } else {
                    format!(
                        "{{{}}}",
                        blacklist
                            .iter()
                            .map(|s| format!("'{}'", s))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!(
                    r#"
import builtins
import sys
import json
import re

WHITELIST = {whitelist}
BLACKLIST = {blacklist}

original_import = builtins.__import__

def safe_import(name, globals=None, locals=None, fromlist=(), level=0):
    if level > 0:
        return original_import(name, globals, locals, fromlist, level)
    root_module = name.split('.')[0]
    if root_module in BLACKLIST:
        raise ImportError(f"Module '{{root_module}}' is blacklisted")
    if root_module not in WHITELIST and root_module != 'builtins':
        raise ImportError(f"Module '{{root_module}}' is not in whitelist")
    return original_import(name, globals, locals, fromlist, level)

builtins.__import__ = safe_import
"#,
                    whitelist = whitelist_str,
                    blacklist = blacklist_str
                )
            }
        }
    }

    /// Generate network control code based on optional host allowlist
    fn generate_network_control(&self, allowlist: Option<&[String]>) -> String {
        let Some(allowlist) = allowlist else {
            return String::new();
        };
        if allowlist.is_empty() {
            return String::new();
        }

        let allowlist_str = format!(
            "[{}]",
            allowlist
                .iter()
                .map(|s| format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")))
                .collect::<Vec<_>>()
                .join(", ")
        );

        format!(
            r#"
_RZN_NETWORK_ALLOWLIST = {allowlist}

if _RZN_NETWORK_ALLOWLIST:
    try:
        import socket
    except Exception:
        socket = None

    if socket is not None:
        def _rzn_norm_host(value):
            if value is None:
                return ""
            return str(value).strip().lower().rstrip(".")

        def _rzn_host_allowed(host):
            h = _rzn_norm_host(host)
            if not h:
                return True
            for pattern in _RZN_NETWORK_ALLOWLIST:
                p = _rzn_norm_host(pattern)
                if not p:
                    continue
                if p == "*":
                    return True
                if p.startswith("*."):
                    base = p[2:]
                    if h == base or h.endswith("." + base):
                        return True
                elif h == p:
                    return True
            return False

        def _rzn_host_from_address(address):
            if isinstance(address, tuple) and len(address) > 0:
                return address[0]
            return None

        _rzn_orig_getaddrinfo = socket.getaddrinfo
        def _rzn_guarded_getaddrinfo(host, *args, **kwargs):
            if not _rzn_host_allowed(host):
                raise PermissionError(f"Network host not allowed: {{host}}")
            return _rzn_orig_getaddrinfo(host, *args, **kwargs)
        socket.getaddrinfo = _rzn_guarded_getaddrinfo

        _rzn_orig_create_connection = socket.create_connection
        def _rzn_guarded_create_connection(address, *args, **kwargs):
            host = _rzn_host_from_address(address)
            if not _rzn_host_allowed(host):
                raise PermissionError(f"Network host not allowed: {{host}}")
            return _rzn_orig_create_connection(address, *args, **kwargs)
        socket.create_connection = _rzn_guarded_create_connection

        _rzn_orig_socket_connect = socket.socket.connect
        def _rzn_guarded_socket_connect(sock, address):
            host = _rzn_host_from_address(address)
            if not _rzn_host_allowed(host):
                raise PermissionError(f"Network host not allowed: {{host}}")
            return _rzn_orig_socket_connect(sock, address)
        socket.socket.connect = _rzn_guarded_socket_connect
"#,
            allowlist = allowlist_str
        )
    }

    /// Build the command to execute Python in a sandbox
    #[cfg(target_os = "macos")]
    fn build_sandboxed_command(&self, workspace: &IsolatedWorkspace) -> Command {
        if let Some(profile) = &self.config.sandbox_profile {
            if profile.exists() {
                // Use sandbox-exec with the profile
                let mut cmd = Command::new("sandbox-exec");
                cmd.arg("-f").arg(profile);

                // Set sandbox parameters
                cmd.arg("-D").arg(format!(
                    "PYTHON_HOME={}",
                    self.config
                        .python_path
                        .parent()
                        .unwrap()
                        .parent()
                        .unwrap()
                        .display()
                ));
                cmd.arg("-D")
                    .arg(format!("WORKSPACE={}", workspace.path.display()));
                cmd.arg("-D")
                    .arg(format!("TMPDIR={}", std::env::temp_dir().display()));

                // Add Python executable
                cmd.arg(&self.config.python_path);

                info!(
                    "[SANDBOX] Using macOS sandbox-exec with profile: {:?}",
                    profile
                );
                return cmd;
            } else {
                warn!(
                    "[SANDBOX] Sandbox profile not found at {:?}, falling back to unsandboxed",
                    profile
                );
            }
        }

        // Fallback: no sandbox-exec, just run Python directly
        warn!("[SANDBOX] Running without platform sandbox (no profile configured)");
        Command::new(&self.config.python_path)
    }

    #[cfg(target_os = "windows")]
    fn build_sandboxed_command(&self, _workspace: &IsolatedWorkspace) -> Command {
        // TODO: Implement Windows Job Objects + Restricted Token
        // For now, just run Python directly
        warn!("[SANDBOX] Windows sandboxing not yet implemented, running unsandboxed");
        Command::new(&self.config.python_path)
    }

    #[cfg(target_os = "linux")]
    fn build_sandboxed_command(&self, workspace: &IsolatedWorkspace) -> Command {
        // TODO: Implement bubblewrap or seccomp sandboxing
        // For now, just run Python directly
        warn!("[SANDBOX] Linux sandboxing not yet implemented, running unsandboxed");
        Command::new(&self.config.python_path)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    fn build_sandboxed_command(&self, _workspace: &IsolatedWorkspace) -> Command {
        Command::new(&self.config.python_path)
    }
}

#[async_trait]
impl PythonEngine for SandboxedPythonEngine {
    async fn validate(&self, code: &str, _options: &ExecutionOptions) -> Result<()> {
        // Basic syntax validation
        let validation_code = format!(
            r#"
import ast
try:
    ast.parse('''{}''')
    print("OK")
except SyntaxError as e:
    print(f"SYNTAX_ERROR: {{e}}")
"#,
            code.replace("'''", "\\'\\'\\'")
        );

        let output = Command::new(&self.config.python_path)
            .arg("-c")
            .arg(&validation_code)
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.starts_with("SYNTAX_ERROR:") {
            return Err(SandboxError::SyntaxError(
                stdout.trim_start_matches("SYNTAX_ERROR: ").to_string(),
            ));
        }

        Ok(())
    }

    async fn execute(
        &mut self,
        code: &str,
        inputs: serde_json::Value,
        options: &ExecutionOptions,
    ) -> Result<serde_json::Value> {
        // Validate first
        self.validate(code, options).await?;

        // Create isolated workspace
        let workspace = IsolatedWorkspace::new(&self.config.workspace_base)?;

        // Copy any configured input files to the workspace
        for (source, name) in &self.config.input_files {
            workspace.copy_input(source, name)?;
        }

        // Create execution wrapper
        let wrapper_code = format!(
            r#"
# Security setup
{}

# Network setup
{}

# Input setup
import json
import sys
import os
import base64
from io import StringIO

inputs = json.loads('''{}''')

# Set workspace paths as environment variables for the code
workspace_path = os.environ.get('SANDBOX_WORKSPACE', '.')
input_dir = os.path.join(workspace_path, 'input')
output_dir = os.path.join(workspace_path, 'output')

# Make these available to user code
WORKSPACE = workspace_path
INPUT_DIR = input_dir
OUTPUT_DIR = output_dir

# Capture stdout/stderr
_captured_stdout = StringIO()
_captured_stderr = StringIO()
_original_stdout = sys.stdout
_original_stderr = sys.stderr
sys.stdout = _captured_stdout
sys.stderr = _captured_stderr

_exec_result = None
_exec_error = None

# User code execution
try:
    {}

    # Capture result variable if set
    if 'result' in dir() or 'result' in locals():
        _exec_result = result
except Exception as e:
    _exec_error = f"{{type(e).__name__}}: {{e}}"

# Restore stdout/stderr
sys.stdout = _original_stdout
sys.stderr = _original_stderr

# Collect output files
_output_files = []
if os.path.exists(output_dir):
    _output_files = os.listdir(output_dir)

# Output structured result
_output = {{
    "stdout": _captured_stdout.getvalue() or None,
    "stderr": _captured_stderr.getvalue() or None,
    "result": None,
    "error": _exec_error,
    "output_files": _output_files,
    "workspace": workspace_path
}}

if _exec_result is not None:
    if isinstance(_exec_result, (dict, list, str, int, float, bool, type(None))):
        _output["result"] = _exec_result
    elif isinstance(_exec_result, (bytes, bytearray, memoryview)):
        _bytes = bytes(_exec_result)
        _output["result"] = {{
            "type": "bytes",
            "encoding": "base64",
            "data": base64.b64encode(_bytes).decode("utf-8")
        }}
    else:
        _output["result"] = {{"type": str(type(_exec_result).__name__), "repr": str(_exec_result)}}

print("OUTPUT_JSON_START")
print(json.dumps(_output))
print("OUTPUT_JSON_END")

if _exec_error:
    sys.exit(1)
"#,
            self.generate_import_control(&options.import_policy),
            self.generate_network_control(options.network_allowlist.as_deref()),
            serde_json::to_string(&inputs)?.replace("'", "\\'"),
            code.replace('\n', "\n    ")
        );

        // Build sandboxed command
        let mut cmd = self.build_sandboxed_command(&workspace);

        cmd.arg("-c")
            .arg(&wrapper_code)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONIOENCODING", "utf-8")
            .env("SANDBOX_WORKSPACE", &workspace.path);

        // Set thread limits
        cmd.env(
            "OMP_NUM_THREADS",
            self.config.limits.max_threads.to_string(),
        )
        .env(
            "OPENBLAS_NUM_THREADS",
            self.config.limits.max_threads.to_string(),
        )
        .env(
            "MKL_NUM_THREADS",
            self.config.limits.max_threads.to_string(),
        );
        for (key, value) in &options.env_vars {
            cmd.env(key, value);
        }

        fn resolve_export_base_dir() -> Option<PathBuf> {
            if let Ok(v) = std::env::var("RZN_PYTHON_EXPORT_DIR") {
                let trimmed = v.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(PathBuf::from(trimmed));
                }
            }
            if let Ok(v) = std::env::var("RZN_APP_BASE_DIR") {
                let trimmed = v.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(
                        PathBuf::from(trimmed)
                            .join("generated")
                            .join("python_exports"),
                    );
                }
            }
            None
        }

        fn maybe_export_outputs(
            workspace: &IsolatedWorkspace,
        ) -> Option<(PathBuf, Vec<serde_json::Value>)> {
            let export_base = resolve_export_base_dir()?;
            if let Err(e) = std::fs::create_dir_all(&export_base) {
                warn!(
                    "[SANDBOX] Failed to create export base dir {:?}: {}",
                    export_base, e
                );
                return None;
            }

            let workspace_id = workspace
                .path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("workspace");
            let export_dir_path = export_base.join(workspace_id);
            let mut export_dir: Option<PathBuf> = None;

            let mut exported: Vec<serde_json::Value> = Vec::new();
            let mut total_bytes: u64 = 0;
            const MAX_FILES: usize = 32;
            const MAX_TOTAL_BYTES: u64 = 200 * 1024 * 1024; // 200MB guard

            let entries = match std::fs::read_dir(workspace.output_dir()) {
                Ok(v) => v,
                Err(e) => {
                    warn!(
                        "[SANDBOX] Failed to read workspace output dir {:?}: {}",
                        workspace.output_dir(),
                        e
                    );
                    return None;
                }
            };

            for entry in entries {
                let entry = match entry {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if exported.len() >= MAX_FILES {
                    break;
                }
                let Ok(ft) = entry.file_type() else { continue };
                if !ft.is_file() || ft.is_symlink() {
                    continue;
                }
                let file_name = entry.file_name();
                let Some(file_name_str) = file_name.to_str() else {
                    continue;
                };
                let src = entry.path();

                if export_dir.is_none() {
                    if let Err(e) = std::fs::create_dir_all(&export_dir_path) {
                        warn!(
                            "[SANDBOX] Failed to create export dir {:?}: {}",
                            export_dir_path, e
                        );
                        return None;
                    }
                    export_dir = Some(export_dir_path.clone());
                }

                let Some(export_dir) = export_dir.as_ref() else {
                    continue;
                };
                let dest = export_dir.join(&file_name);

                if let Ok(meta) = std::fs::metadata(&src) {
                    total_bytes = total_bytes.saturating_add(meta.len());
                    if total_bytes > MAX_TOTAL_BYTES {
                        warn!(
                            "[SANDBOX] Output export size limit exceeded ({} bytes), stopping export",
                            total_bytes
                        );
                        break;
                    }
                }

                match std::fs::copy(&src, &dest) {
                    Ok(_) => {
                        let size = std::fs::metadata(&dest).ok().map(|m| m.len());
                        exported.push(serde_json::json!({
                            "name": file_name_str,
                            "path": dest.to_string_lossy().to_string(),
                            "size_bytes": size,
                        }));
                    }
                    Err(e) => warn!("[SANDBOX] Failed to export {:?}: {}", src, e),
                }
            }

            export_dir.map(|dir| (dir, exported))
        }

        // Execute with timeout
        let child = cmd.spawn()?;

        match tokio::time::timeout(options.timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Check for sandbox violations (macOS)
                if stderr.contains("deny") || stderr.contains("Sandbox") {
                    error!("[SANDBOX] Sandbox violation detected: {}", stderr);
                    return Err(SandboxError::SecurityViolation(
                        "Operation blocked by sandbox".to_string(),
                    ));
                }

                // Extract structured output
                if let Some(start) = stdout.find("OUTPUT_JSON_START") {
                    if let Some(end) = stdout.find("OUTPUT_JSON_END") {
                        let json_str = &stdout[start + 17..end].trim();
                        if let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(json_str)
                        {
                            // Check if there was an execution error
                            if let Some(error) = parsed.get("error").and_then(|e| e.as_str()) {
                                if !error.is_empty() {
                                    return Err(SandboxError::RuntimeError(error.to_string()));
                                }
                            }

                            // Optional export: copy OUTPUT_DIR files into an app-controlled directory
                            // (e.g., host-managed generated folder) and annotate the output.
                            if let Some((export_dir, exported_files)) =
                                maybe_export_outputs(&workspace)
                            {
                                if let Some(obj) = parsed.as_object_mut() {
                                    obj.insert(
                                        "export_dir".to_string(),
                                        serde_json::Value::String(
                                            export_dir.to_string_lossy().to_string(),
                                        ),
                                    );
                                    obj.insert(
                                        "exported_files".to_string(),
                                        serde_json::Value::Array(exported_files),
                                    );
                                }
                            }
                            return Ok(parsed);
                        }
                    }
                }

                // Fallback: check for errors
                if !output.status.success() {
                    if stderr.contains("MemoryError") {
                        return Err(SandboxError::MemoryLimitExceeded);
                    }
                    return Err(SandboxError::RuntimeError(stderr.to_string()));
                }

                Ok(serde_json::Value::Null)
            }
            Ok(Err(e)) => Err(SandboxError::IoError(e)),
            Err(_) => Err(SandboxError::Timeout),
        }
    }

    fn capabilities(&self) -> EngineCapabilities {
        let has_sandbox = self
            .config
            .sandbox_profile
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false);

        EngineCapabilities {
            name: if has_sandbox {
                "Platform-Sandboxed Python".to_string()
            } else {
                "Workspace-Isolated Python".to_string()
            },
            numpy: true,
            matplotlib: true,
            pandas: true,
            max_memory_mb: self.config.limits.memory_mb,
            max_cpu_seconds: self.config.limits.cpu_seconds,
            security_level: if has_sandbox { 7 } else { 5 },
        }
    }

    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Builder for creating sandboxed execution contexts with file I/O
pub struct SandboxedExecutionBuilder {
    config: SandboxConfig,
    input_files: Vec<(PathBuf, String)>,
    output_files: Vec<(String, PathBuf)>,
}

impl SandboxedExecutionBuilder {
    /// Create a new builder with the given Python path
    pub fn new(python_path: PathBuf) -> Self {
        Self {
            config: SandboxConfig {
                python_path,
                ..Default::default()
            },
            input_files: Vec::new(),
            output_files: Vec::new(),
        }
    }

    /// Set the sandbox profile (macOS .sb file)
    pub fn with_sandbox_profile(mut self, profile: PathBuf) -> Self {
        self.config.sandbox_profile = Some(profile);
        self
    }

    /// Add an input file to copy into the workspace
    pub fn with_input_file(mut self, source: PathBuf, workspace_name: &str) -> Self {
        self.input_files.push((source, workspace_name.to_string()));
        self
    }

    /// Specify an output file to copy out of the workspace after execution
    pub fn with_output_file(mut self, workspace_name: &str, destination: PathBuf) -> Self {
        self.output_files
            .push((workspace_name.to_string(), destination));
        self
    }

    /// Set resource limits
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.config.limits = limits;
        self
    }

    /// Execute code and return results, handling file I/O
    pub async fn execute(
        self,
        code: &str,
        inputs: serde_json::Value,
        options: ExecutionOptions,
    ) -> Result<SandboxedExecutionResult> {
        // Create workspace
        let workspace = IsolatedWorkspace::new(&self.config.workspace_base)?;

        // Copy input files
        for (source, name) in &self.input_files {
            workspace.copy_input(source, name)?;
        }

        // Create engine with the workspace
        let mut config = self.config.clone();
        config.input_files = self.input_files.clone();

        let mut engine = SandboxedPythonEngine::new(config)?;

        // Execute
        let result = engine.execute(code, inputs, &options).await?;

        // Copy output files
        for (name, dest) in &self.output_files {
            workspace.copy_output(name, dest)?;
        }

        // List all outputs
        let output_files = workspace.list_outputs()?;

        Ok(SandboxedExecutionResult {
            result,
            output_files,
            workspace_path: workspace.path.clone(),
        })
    }
}

/// Result of a sandboxed execution
#[derive(Debug)]
pub struct SandboxedExecutionResult {
    /// The JSON result from Python
    pub result: serde_json::Value,
    /// List of files created in the output directory
    pub output_files: Vec<String>,
    /// Path to the workspace (for manual file retrieval)
    pub workspace_path: PathBuf,
}
