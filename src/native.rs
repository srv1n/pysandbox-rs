use crate::{
    config::{ImportPolicy, ResourceLimits},
    engine::{EngineCapabilities, ExecutionOptions, PythonEngine},
    errors::{Result, SandboxError},
};
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// Native Python engine with security guardrails
pub struct NativePythonEngine {
    python_path: PathBuf,
    limits: ResourceLimits,
}

impl NativePythonEngine {
    /// Create a new native Python engine (finds Python in PATH)
    pub fn new() -> Result<Self> {
        let python_path = which::which("python3")
            .or_else(|_| which::which("python"))
            .map_err(|_| SandboxError::PythonNotFound)?;

        Ok(Self {
            python_path,
            limits: ResourceLimits::default(),
        })
    }

    /// Create with explicit Python path (for bundled Python)
    ///
    /// Use this when shipping a bundled Python distribution with your app.
    ///
    /// # Example
    /// ```rust,ignore
    /// let python_path = PathBuf::from("/path/to/bundled/python/bin/python3");
    /// let engine = NativePythonEngine::with_python_path(python_path)?;
    /// ```
    pub fn with_python_path(python_path: PathBuf) -> Result<Self> {
        if !python_path.exists() {
            return Err(SandboxError::PythonNotFound);
        }

        Ok(Self {
            python_path,
            limits: ResourceLimits::default(),
        })
    }

    /// Create with custom resource limits
    pub fn with_limits(limits: ResourceLimits) -> Result<Self> {
        let mut engine = Self::new()?;
        engine.limits = limits;
        Ok(engine)
    }

    /// Create with bundled Python and custom limits
    pub fn with_python_path_and_limits(
        python_path: PathBuf,
        limits: ResourceLimits,
    ) -> Result<Self> {
        let mut engine = Self::with_python_path(python_path)?;
        engine.limits = limits;
        Ok(engine)
    }

    /// Get the Python executable path being used
    pub fn python_path(&self) -> &PathBuf {
        &self.python_path
    }

    /// Generate import control code based on policy
    fn generate_import_control(&self, policy: &ImportPolicy) -> String {
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
    # For relative imports (level > 0), allow them - they're within an already-imported package
    if level > 0:
        return original_import(name, globals, locals, fromlist, level)

    root_module = name.split('.')[0]
    if root_module in BLACKLIST:
        raise ImportError(f"Module '{{root_module}}' is blacklisted for safety")
    return original_import(name, globals, locals, fromlist, level)

builtins.__import__ = safe_import

# Restrict open to read-only
if hasattr(builtins, 'open'):
    _original_open = builtins.open
    def restricted_open(file, mode='r', *args, **kwargs):
        if 'w' in mode or 'a' in mode or 'x' in mode:
            raise PermissionError("Write access is not allowed")
        return _original_open(file, mode, *args, **kwargs)
    builtins.open = restricted_open

# Note: We keep exec, eval, compile as libraries need them
# The import restrictions provide the main security
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
# Pre-import essential modules BEFORE setting up the hook
# This ensures they're cached and won't trigger whitelist checks
import builtins
import sys
import json  # Needed by our wrapper for input/output handling
import re    # Common dependency

WHITELIST = {whitelist}

original_import = builtins.__import__

def safe_import(name, globals=None, locals=None, fromlist=(), level=0):
    # For relative imports (level > 0), allow them - they're within an already-imported package
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
# Pre-import essential modules BEFORE setting up the hook
import builtins
import sys
import json  # Needed by our wrapper for input/output handling
import re    # Common dependency

WHITELIST = {whitelist}
BLACKLIST = {blacklist}

original_import = builtins.__import__

def safe_import(name, globals=None, locals=None, fromlist=(), level=0):
    # For relative imports (level > 0), allow them - they're within an already-imported package
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

    /// Apply resource limits to the command
    #[cfg(unix)]
    fn apply_resource_limits(&self, cmd: &mut Command, limits: &ResourceLimits) {
        let cpu_seconds = limits.cpu_seconds;
        #[cfg(not(target_os = "macos"))]
        let memory_bytes = limits.memory_mb * 1024 * 1024;
        #[cfg(not(target_os = "macos"))]
        let max_processes = limits.max_processes;

        unsafe {
            cmd.pre_exec(move || {
                // Create new process group for easier cleanup
                libc::setpgid(0, 0);

                // Set memory limit (macOS specific handling)
                #[cfg(target_os = "macos")]
                {
                    // macOS doesn't support RLIMIT_AS properly, skip it
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let rlimit = libc::rlimit {
                        rlim_cur: memory_bytes as libc::rlim_t,
                        rlim_max: memory_bytes as libc::rlim_t,
                    };
                    if libc::setrlimit(libc::RLIMIT_AS, &rlimit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }

                // Set CPU time limit
                let rlimit = libc::rlimit {
                    rlim_cur: cpu_seconds as libc::rlim_t,
                    rlim_max: cpu_seconds as libc::rlim_t,
                };
                if libc::setrlimit(libc::RLIMIT_CPU, &rlimit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }

                // Set process limit (also problematic on macOS)
                #[cfg(not(target_os = "macos"))]
                {
                    let rlimit = libc::rlimit {
                        rlim_cur: max_processes as libc::rlim_t,
                        rlim_max: max_processes as libc::rlim_t,
                    };
                    if libc::setrlimit(libc::RLIMIT_NPROC, &rlimit) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }

                Ok(())
            });
        }
    }

    #[cfg(not(unix))]
    fn apply_resource_limits(&self, _cmd: &mut Command, _limits: &ResourceLimits) {
        // Windows implementation would use Job Objects
        // For now, we'll rely on timeout only
    }
}

#[async_trait]
impl PythonEngine for NativePythonEngine {
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
            code.replace("'''", "\\'''")
        );

        let output = Command::new(&self.python_path)
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

        // Create execution wrapper that captures stdout/stderr
        let wrapper_code = format!(
            r#"
# Security setup
{}

# Network setup
{}

# Input setup
import base64
import json
import sys
from io import StringIO

inputs = json.loads('''{}''')

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

# Output structured result
_output = {{
    "stdout": _captured_stdout.getvalue() or None,
    "stderr": _captured_stderr.getvalue() or None,
    "result": None,
    "error": _exec_error
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

        // Create command
        let mut cmd = Command::new(&self.python_path);
        cmd.arg("-c")
            .arg(&wrapper_code)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PYTHONIOENCODING", "utf-8");

        // Set thread limits
        cmd.env("OMP_NUM_THREADS", self.limits.max_threads.to_string())
            .env("OPENBLAS_NUM_THREADS", self.limits.max_threads.to_string())
            .env("MKL_NUM_THREADS", self.limits.max_threads.to_string());
        for (key, value) in &options.env_vars {
            cmd.env(key, value);
        }

        // Apply resource limits
        self.apply_resource_limits(&mut cmd, &self.limits);

        // Execute with timeout
        let child = cmd.spawn()?;
        let pid = child.id();

        match tokio::time::timeout(options.timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Extract structured output
                if let Some(start) = stdout.find("OUTPUT_JSON_START") {
                    if let Some(end) = stdout.find("OUTPUT_JSON_END") {
                        let json_str = &stdout[start + 17..end].trim();
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                            // Check if there was an execution error
                            if let Some(error) = parsed.get("error").and_then(|e| e.as_str()) {
                                if !error.is_empty() {
                                    return Err(SandboxError::RuntimeError(error.to_string()));
                                }
                            }
                            return Ok(parsed);
                        }
                    }
                }

                // Fallback: check for memory errors
                if !output.status.success() {
                    if stderr.contains("MemoryError") {
                        return Err(SandboxError::MemoryLimitExceeded);
                    }
                    return Err(SandboxError::RuntimeError(stderr.to_string()));
                }

                Ok(serde_json::Value::Null)
            }
            Ok(Err(e)) => Err(SandboxError::IoError(e)),
            Err(_) => {
                // Timeout - kill process
                #[cfg(unix)]
                {
                    if let Some(pid) = pid {
                        unsafe {
                            libc::kill(-(pid as i32), libc::SIGKILL);
                        }
                    }
                }
                Err(SandboxError::Timeout)
            }
        }
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            name: "Native Python (Guarded)".to_string(),
            numpy: true, // Assumes user has it installed
            matplotlib: true,
            pandas: true,
            max_memory_mb: self.limits.memory_mb,
            max_cpu_seconds: self.limits.cpu_seconds,
            security_level: 5, // Medium security with guardrails
        }
    }

    async fn shutdown(&mut self) -> Result<()> {
        // Cleanup temp files if any
        Ok(())
    }
}
