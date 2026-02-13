use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Execution mode determines which engine and sandboxing approach to use
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// Native Python with import guardrails only
    /// Fastest, lowest isolation, security level 5/10
    /// Use for trusted code or when performance is critical
    Native,

    /// Workspace-isolated execution
    /// Files are copied to/from an isolated temp directory
    /// Python can't access files outside the workspace
    /// Security level 6/10
    #[default]
    WorkspaceIsolated,

    /// Platform-sandboxed execution (macOS sandbox-exec, Windows Job Objects)
    /// Full OS-level sandboxing + workspace isolation
    /// No network, restricted file access, no subprocess spawning
    /// Security level 7-8/10 depending on platform
    PlatformSandboxed,
}

impl ExecutionMode {
    /// Get the security level for this mode
    pub fn security_level(&self) -> u8 {
        match self {
            ExecutionMode::Native => 5,
            ExecutionMode::WorkspaceIsolated => 6,
            ExecutionMode::PlatformSandboxed => 7,
        }
    }

    /// Description for logging/debugging
    pub fn description(&self) -> &'static str {
        match self {
            ExecutionMode::Native => "Native Python with import guardrails",
            ExecutionMode::WorkspaceIsolated => "Workspace-isolated Python",
            ExecutionMode::PlatformSandboxed => "Platform-sandboxed Python (OS-level isolation)",
        }
    }
}

/// Security profile for Python sandbox execution
/// Determines the level of restrictions applied to code execution
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SecurityProfile {
    /// YOLO mode - no restrictions, full system access
    /// WARNING: Only use for trusted code or in isolated environments
    Yolo,

    /// Blacklist mode - blocks known dangerous modules (os, subprocess, socket, etc.)
    /// Allows most imports, good balance of functionality and safety
    #[default]
    Blacklist,

    /// Whitelist mode - only allows specific data science modules
    /// More restrictive, good for untrusted data analysis code
    DataScience,

    /// Strict mode - whitelist + read-only filesystem + resource limits
    /// Maximum security for untrusted code
    Strict,
}

impl SecurityProfile {
    /// Convert profile to import policy
    pub fn to_import_policy(&self) -> ImportPolicy {
        match self {
            SecurityProfile::Yolo => ImportPolicy::Blacklist(HashSet::new()), // Empty blacklist = allow all
            SecurityProfile::Blacklist => ImportPolicy::default(),
            SecurityProfile::DataScience => ImportPolicy::data_science_whitelist(),
            SecurityProfile::Strict => ImportPolicy::data_science_whitelist(),
        }
    }

    /// Get resource limits for this profile
    pub fn resource_limits(&self) -> ResourceLimits {
        match self {
            SecurityProfile::Yolo => ResourceLimits {
                memory_mb: 8192,
                cpu_seconds: 300,
                max_processes: 50,
                max_threads: 16,
            },
            SecurityProfile::Blacklist => ResourceLimits::default(),
            SecurityProfile::DataScience => ResourceLimits::default(),
            SecurityProfile::Strict => ResourceLimits {
                memory_mb: 512,
                cpu_seconds: 10,
                max_processes: 1,
                max_threads: 2,
            },
        }
    }

    /// Whether to restrict file write access
    pub fn restrict_file_writes(&self) -> bool {
        !matches!(self, SecurityProfile::Yolo)
    }

    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            SecurityProfile::Yolo => "No restrictions - full system access (dangerous!)",
            SecurityProfile::Blacklist => "Blocks dangerous modules (os, subprocess, socket, etc.)",
            SecurityProfile::DataScience => {
                "Only allows data science modules (numpy, pandas, etc.)"
            }
            SecurityProfile::Strict => "Maximum security - whitelist + strict resource limits",
        }
    }
}

/// Resource limits for Python execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum memory in MB
    pub memory_mb: usize,
    /// Maximum CPU time in seconds
    pub cpu_seconds: u64,
    /// Maximum number of processes
    pub max_processes: u64,
    /// Maximum number of threads for scientific libraries
    pub max_threads: u32,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            memory_mb: 2048,
            cpu_seconds: 30,
            max_processes: 10,
            max_threads: 4,
        }
    }
}

/// Import control policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImportPolicy {
    /// Block specific modules
    Blacklist(HashSet<String>),
    /// Only allow specific modules
    Whitelist(HashSet<String>),
    /// Both whitelist and blacklist
    Both {
        whitelist: HashSet<String>,
        blacklist: HashSet<String>,
    },
}

impl Default for ImportPolicy {
    fn default() -> Self {
        // Default blacklist of dangerous modules
        let mut blacklist = HashSet::new();
        blacklist.insert("subprocess".to_string());
        blacklist.insert("multiprocessing".to_string());
        blacklist.insert("os".to_string());
        blacklist.insert("socket".to_string());
        blacklist.insert("urllib".to_string());
        blacklist.insert("requests".to_string());
        blacklist.insert("ctypes".to_string());
        blacklist.insert("pty".to_string());
        blacklist.insert("fcntl".to_string());
        blacklist.insert("resource".to_string());
        blacklist.insert("shutil".to_string());
        blacklist.insert("tempfile".to_string());
        blacklist.insert("commands".to_string());
        blacklist.insert("popen2".to_string());
        blacklist.insert("cgi".to_string());
        blacklist.insert("cgitb".to_string());
        blacklist.insert("webbrowser".to_string());
        blacklist.insert("antigravity".to_string());

        ImportPolicy::Blacklist(blacklist)
    }
}

impl ImportPolicy {
    /// Check if an import is allowed
    pub fn is_allowed(&self, module: &str) -> bool {
        let root_module = module.split('.').next().unwrap_or(module);

        match self {
            ImportPolicy::Blacklist(blacklist) => !blacklist.contains(root_module),
            ImportPolicy::Whitelist(whitelist) => whitelist.contains(root_module),
            ImportPolicy::Both {
                whitelist,
                blacklist,
            } => whitelist.contains(root_module) && !blacklist.contains(root_module),
        }
    }

    /// Create a whitelist policy for data science
    pub fn data_science_whitelist() -> Self {
        let mut whitelist = HashSet::new();
        // Core data science libraries
        whitelist.insert("numpy".to_string());
        whitelist.insert("pandas".to_string());
        whitelist.insert("matplotlib".to_string());
        whitelist.insert("scipy".to_string());
        whitelist.insert("sklearn".to_string());
        whitelist.insert("seaborn".to_string());
        whitelist.insert("statsmodels".to_string());
        // Document processing
        whitelist.insert("fitz".to_string()); // PyMuPDF
        whitelist.insert("pymupdf".to_string()); // PyMuPDF alternative import
        whitelist.insert("PIL".to_string()); // Pillow
        whitelist.insert("openpyxl".to_string()); // Excel files
        whitelist.insert("docx".to_string()); // python-docx (Word documents)
        whitelist.insert("pptx".to_string()); // python-pptx (PowerPoint)
        whitelist.insert("lxml".to_string()); // XML processing (used by docx/pptx)
        whitelist.insert("zipfile".to_string()); // ZIP archives (docx are zips)
        whitelist.insert("xml".to_string()); // XML (used by docx internals)
                                             // Core Python modules (always needed)
        whitelist.insert("sys".to_string());
        whitelist.insert("builtins".to_string());
        whitelist.insert("_frozen_importlib".to_string());
        whitelist.insert("_frozen_importlib_external".to_string());
        whitelist.insert("_imp".to_string());
        whitelist.insert("_sre".to_string());
        whitelist.insert("_codecs".to_string());
        whitelist.insert("codecs".to_string());
        whitelist.insert("_signal".to_string());
        // Standard library
        whitelist.insert("json".to_string());
        whitelist.insert("math".to_string());
        whitelist.insert("random".to_string());
        whitelist.insert("datetime".to_string());
        whitelist.insert("collections".to_string());
        whitelist.insert("itertools".to_string());
        whitelist.insert("io".to_string());
        whitelist.insert("base64".to_string());
        whitelist.insert("time".to_string());
        // Internal modules needed by the libraries
        whitelist.insert("_io".to_string());
        whitelist.insert("_json".to_string());
        whitelist.insert("_datetime".to_string());
        whitelist.insert("_collections".to_string());
        whitelist.insert("_random".to_string());
        whitelist.insert("_bisect".to_string());
        whitelist.insert("_heapq".to_string());
        whitelist.insert("_pickle".to_string());
        whitelist.insert("_struct".to_string());
        whitelist.insert("_warnings".to_string());
        whitelist.insert("_weakref".to_string());
        whitelist.insert("_abc".to_string());
        whitelist.insert("_thread".to_string());
        whitelist.insert("_functools".to_string());
        whitelist.insert("_operator".to_string());
        whitelist.insert("_stat".to_string());
        whitelist.insert("array".to_string());
        whitelist.insert("binascii".to_string());
        whitelist.insert("copy".to_string());
        whitelist.insert("copyreg".to_string());
        whitelist.insert("encodings".to_string());
        whitelist.insert("errno".to_string());
        whitelist.insert("gc".to_string());
        whitelist.insert("genericpath".to_string());
        whitelist.insert("hashlib".to_string());
        whitelist.insert("inspect".to_string());
        whitelist.insert("keyword".to_string());
        whitelist.insert("linecache".to_string());
        whitelist.insert("locale".to_string());
        whitelist.insert("logging".to_string());
        whitelist.insert("numbers".to_string());
        whitelist.insert("operator".to_string());
        whitelist.insert("pickle".to_string());
        whitelist.insert("platform".to_string());
        whitelist.insert("posixpath".to_string());
        whitelist.insert("re".to_string());
        whitelist.insert("reprlib".to_string());
        whitelist.insert("select".to_string());
        whitelist.insert("signal".to_string());
        whitelist.insert("site".to_string());
        whitelist.insert("stat".to_string());
        whitelist.insert("string".to_string());
        whitelist.insert("struct".to_string());
        whitelist.insert("sysconfig".to_string());
        whitelist.insert("threading".to_string());
        whitelist.insert("token".to_string());
        whitelist.insert("tokenize".to_string());
        whitelist.insert("traceback".to_string());
        whitelist.insert("types".to_string());
        whitelist.insert("typing".to_string());
        whitelist.insert("warnings".to_string());
        whitelist.insert("weakref".to_string());
        whitelist.insert("abc".to_string());
        whitelist.insert("functools".to_string());
        whitelist.insert("contextlib".to_string());
        whitelist.insert("dataclasses".to_string());
        whitelist.insert("decimal".to_string());
        whitelist.insert("enum".to_string());
        whitelist.insert("fractions".to_string());
        whitelist.insert("uuid".to_string());
        whitelist.insert("zlib".to_string());
        whitelist.insert("textwrap".to_string());
        whitelist.insert("unicodedata".to_string());
        // Common paths for imports
        whitelist.insert("matplotlib.pyplot".to_string());
        whitelist.insert("matplotlib.figure".to_string());
        whitelist.insert("matplotlib.axes".to_string());
        whitelist.insert("numpy.core".to_string());
        whitelist.insert("numpy.linalg".to_string());
        whitelist.insert("pandas.core".to_string());

        ImportPolicy::Whitelist(whitelist)
    }
}
