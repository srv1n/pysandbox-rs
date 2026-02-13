//! Sandbox Policy System
//!
//! This module provides a flexible, layered configuration system for Python sandboxing:
//!
//! 1. **Primitives** - Individual configurable options (network, filesystem, imports, etc.)
//! 2. **Templates** - Pre-built policy configurations for common use cases
//! 3. **Enterprise Policies** - Org-wide enforcement with locked settings
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Enterprise Policy (optional)                                │
//! │  - Defines minimum security requirements                    │
//! │  - Locks certain primitives from user modification          │
//! │  - Applied org-wide via config file                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  User Policy (template or custom)                           │
//! │  - User selects a template or customizes primitives         │
//! │  - Must comply with enterprise policy if present            │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Primitives                                                  │
//! │  - Network, Filesystem, Imports, Resources, Execution       │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

// ============================================================================
// PRIMITIVES - Individual configurable options
// ============================================================================

/// Network access policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// All network access blocked (most secure)
    Blocked,
    /// Only localhost allowed
    LocalhostOnly,
    /// Specific domains/IPs allowed
    AllowList(Vec<String>),
    /// All network access allowed (least secure)
    Unrestricted,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        NetworkPolicy::Blocked
    }
}

/// Filesystem access policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FilesystemPolicy {
    /// No filesystem access (code runs in memory only)
    None,
    /// Read-only access to specified paths
    ReadOnly(Vec<PathBuf>),
    /// Read/write only within isolated workspace
    WorkspaceOnly,
    /// Read anywhere, write only to workspace
    ReadAnyWriteWorkspace,
    /// Full read/write access (least secure)
    Unrestricted,
}

impl Default for FilesystemPolicy {
    fn default() -> Self {
        FilesystemPolicy::WorkspaceOnly
    }
}

/// Process/subprocess execution policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessPolicy {
    /// No subprocess execution allowed
    Blocked,
    /// Only allow specific executables
    AllowList(Vec<String>),
    /// Allow all subprocess execution (dangerous!)
    Unrestricted,
}

impl Default for ProcessPolicy {
    fn default() -> Self {
        ProcessPolicy::Blocked
    }
}

/// Python import policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImportPolicyType {
    /// No restrictions on imports
    Unrestricted,
    /// Block specific modules
    Blacklist(HashSet<String>),
    /// Only allow specific modules
    Whitelist(HashSet<String>),
    /// Whitelist with additional blacklist for fine-tuning
    WhitelistWithBlacklist {
        whitelist: HashSet<String>,
        blacklist: HashSet<String>,
    },
}

impl Default for ImportPolicyType {
    fn default() -> Self {
        ImportPolicyType::Blacklist(Self::default_blacklist())
    }
}

impl ImportPolicyType {
    /// Default blacklist of dangerous modules
    pub fn default_blacklist() -> HashSet<String> {
        [
            "subprocess",
            "multiprocessing",
            "os",
            "socket",
            "urllib",
            "requests",
            "ctypes",
            "pty",
            "fcntl",
            "resource",
            "shutil",
            "tempfile",
            "commands",
            "popen2",
            "cgi",
            "cgitb",
            "webbrowser",
            "antigravity",
            "asyncio.subprocess",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// Data science whitelist
    pub fn data_science_whitelist() -> HashSet<String> {
        [
            // Data science
            "numpy",
            "pandas",
            "matplotlib",
            "scipy",
            "sklearn",
            "seaborn",
            "statsmodels",
            "plotly",
            // Document processing
            "fitz",
            "pymupdf",
            "PIL",
            "openpyxl",
            "xlrd",
            "docx",
            "pptx",
            // Core Python
            "sys",
            "builtins",
            "json",
            "math",
            "random",
            "datetime",
            "collections",
            "itertools",
            "io",
            "base64",
            "time",
            "re",
            "copy",
            "functools",
            "operator",
            "string",
            "textwrap",
            "decimal",
            "fractions",
            "numbers",
            "statistics",
            // Typing and inspection
            "typing",
            "types",
            "abc",
            "dataclasses",
            "enum",
            // Encoding
            "codecs",
            "encodings",
            "unicodedata",
            "hashlib",
            "base64",
            // Data structures
            "array",
            "struct",
            "pickle",
            // Internal modules needed by libraries
            "_io",
            "_json",
            "_datetime",
            "_collections",
            "_random",
            "_bisect",
            "_heapq",
            "_pickle",
            "_struct",
            "_warnings",
            "_weakref",
            "_abc",
            "_thread",
            "_functools",
            "_operator",
            "_stat",
            "_sre",
            "_codecs",
            "_signal",
            "_frozen_importlib",
            "_frozen_importlib_external",
            "_imp",
            // Other common modules
            "warnings",
            "weakref",
            "contextlib",
            "logging",
            "traceback",
            "linecache",
            "inspect",
            "dis",
            "gc",
            "platform",
            "locale",
            "zlib",
            "gzip",
            "bz2",
            "uuid",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    /// Convert to the ImportPolicy used by the engine
    pub fn to_import_policy(&self) -> crate::config::ImportPolicy {
        match self {
            ImportPolicyType::Unrestricted => {
                crate::config::ImportPolicy::Blacklist(HashSet::new())
            }
            ImportPolicyType::Blacklist(bl) => crate::config::ImportPolicy::Blacklist(bl.clone()),
            ImportPolicyType::Whitelist(wl) => crate::config::ImportPolicy::Whitelist(wl.clone()),
            ImportPolicyType::WhitelistWithBlacklist {
                whitelist,
                blacklist,
            } => crate::config::ImportPolicy::Both {
                whitelist: whitelist.clone(),
                blacklist: blacklist.clone(),
            },
        }
    }
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceLimitsPolicy {
    /// Maximum memory in MB
    pub max_memory_mb: usize,
    /// Maximum CPU time in seconds
    pub max_cpu_seconds: u64,
    /// Maximum wall-clock timeout in seconds
    pub max_timeout_seconds: u64,
    /// Maximum output size in bytes
    pub max_output_bytes: usize,
    /// Maximum number of threads
    pub max_threads: u32,
}

impl Default for ResourceLimitsPolicy {
    fn default() -> Self {
        Self {
            max_memory_mb: 2048,
            max_cpu_seconds: 30,
            max_timeout_seconds: 60,
            max_output_bytes: 10 * 1024 * 1024, // 10MB
            max_threads: 4,
        }
    }
}

/// Execution environment policy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionEnvironment {
    /// Native Python with guardrails only
    Native,
    /// Workspace-isolated execution
    WorkspaceIsolated,
    /// Full platform sandbox (sandbox-exec on macOS, etc.)
    PlatformSandboxed,
}

impl Default for ExecutionEnvironment {
    fn default() -> Self {
        ExecutionEnvironment::WorkspaceIsolated
    }
}

// ============================================================================
// SANDBOX POLICY - Combines all primitives
// ============================================================================

/// Complete sandbox policy combining all primitives
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPolicy {
    /// Human-readable name for this policy
    pub name: String,
    /// Description of what this policy is for
    pub description: String,
    /// Network access policy
    pub network: NetworkPolicy,
    /// Filesystem access policy
    pub filesystem: FilesystemPolicy,
    /// Process execution policy
    pub process: ProcessPolicy,
    /// Import policy
    pub imports: ImportPolicyType,
    /// Resource limits
    pub resources: ResourceLimitsPolicy,
    /// Execution environment
    pub environment: ExecutionEnvironment,
    /// Whether to log all executions for audit
    pub audit_logging: bool,
    /// Custom sandbox profile path (macOS .sb file)
    pub custom_sandbox_profile: Option<PathBuf>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self::balanced()
    }
}

impl SandboxPolicy {
    /// Calculate security level (0-10)
    pub fn security_level(&self) -> u8 {
        let mut level = 0u8;

        // Network (0-2 points)
        level += match &self.network {
            NetworkPolicy::Blocked => 2,
            NetworkPolicy::LocalhostOnly => 1,
            NetworkPolicy::AllowList(_) => 1,
            NetworkPolicy::Unrestricted => 0,
        };

        // Filesystem (0-2 points)
        level += match &self.filesystem {
            FilesystemPolicy::None => 2,
            FilesystemPolicy::WorkspaceOnly => 2,
            FilesystemPolicy::ReadOnly(_) => 1,
            FilesystemPolicy::ReadAnyWriteWorkspace => 1,
            FilesystemPolicy::Unrestricted => 0,
        };

        // Process (0-2 points)
        level += match &self.process {
            ProcessPolicy::Blocked => 2,
            ProcessPolicy::AllowList(_) => 1,
            ProcessPolicy::Unrestricted => 0,
        };

        // Imports (0-2 points)
        level += match &self.imports {
            ImportPolicyType::Whitelist(_) => 2,
            ImportPolicyType::WhitelistWithBlacklist { .. } => 2,
            ImportPolicyType::Blacklist(_) => 1,
            ImportPolicyType::Unrestricted => 0,
        };

        // Environment (0-2 points)
        level += match &self.environment {
            ExecutionEnvironment::PlatformSandboxed => 2,
            ExecutionEnvironment::WorkspaceIsolated => 1,
            ExecutionEnvironment::Native => 0,
        };

        level
    }

    // ========================================================================
    // TEMPLATES - Pre-built policies for common use cases
    // ========================================================================

    /// YOLO mode - No restrictions (security: 0/10)
    /// Use only for fully trusted code in isolated environments
    pub fn yolo() -> Self {
        Self {
            name: "YOLO".to_string(),
            description: "No restrictions - full system access. Use only for trusted code."
                .to_string(),
            network: NetworkPolicy::Unrestricted,
            filesystem: FilesystemPolicy::Unrestricted,
            process: ProcessPolicy::Unrestricted,
            imports: ImportPolicyType::Unrestricted,
            resources: ResourceLimitsPolicy {
                max_memory_mb: 8192,
                max_cpu_seconds: 300,
                max_timeout_seconds: 600,
                max_output_bytes: 100 * 1024 * 1024,
                max_threads: 16,
            },
            environment: ExecutionEnvironment::Native,
            audit_logging: false,
            custom_sandbox_profile: None,
        }
    }

    /// Balanced mode - Some guardrails but flexible (security: 7/10)
    /// Good default for development and semi-trusted code
    pub fn balanced() -> Self {
        Self {
            name: "Balanced".to_string(),
            description: "Import blacklist with reasonable limits. Good for development."
                .to_string(),
            network: NetworkPolicy::Blocked,
            filesystem: FilesystemPolicy::ReadAnyWriteWorkspace,
            process: ProcessPolicy::Blocked,
            imports: ImportPolicyType::default(),
            resources: ResourceLimitsPolicy::default(),
            environment: ExecutionEnvironment::WorkspaceIsolated,
            audit_logging: false,
            custom_sandbox_profile: None,
        }
    }

    /// Data Science mode - Optimized for analysis (security: 6/10)
    /// Allows common data science libraries, blocks dangerous modules
    pub fn data_science() -> Self {
        Self {
            name: "Data Science".to_string(),
            description: "Whitelist of data science libraries. Good for analysis tasks."
                .to_string(),
            network: NetworkPolicy::Blocked,
            filesystem: FilesystemPolicy::ReadAnyWriteWorkspace,
            process: ProcessPolicy::Blocked,
            imports: ImportPolicyType::Whitelist(ImportPolicyType::data_science_whitelist()),
            resources: ResourceLimitsPolicy {
                max_memory_mb: 4096,
                max_cpu_seconds: 60,
                max_timeout_seconds: 120,
                max_output_bytes: 50 * 1024 * 1024,
                max_threads: 8,
            },
            environment: ExecutionEnvironment::WorkspaceIsolated,
            audit_logging: true,
            custom_sandbox_profile: None,
        }
    }

    /// Document Processing mode - For PDF, DOCX, etc. (security: 6/10)
    /// Focused on document manipulation libraries
    pub fn document_processing() -> Self {
        let mut whitelist = ImportPolicyType::data_science_whitelist();
        // Add document-specific libraries
        whitelist.insert("docx".to_string());
        whitelist.insert("pptx".to_string());
        whitelist.insert("pdf".to_string());
        whitelist.insert("reportlab".to_string());

        Self {
            name: "Document Processing".to_string(),
            description: "For PDF, DOCX, PPTX editing. Workspace-isolated file access.".to_string(),
            network: NetworkPolicy::Blocked,
            filesystem: FilesystemPolicy::WorkspaceOnly,
            process: ProcessPolicy::Blocked,
            imports: ImportPolicyType::Whitelist(whitelist),
            resources: ResourceLimitsPolicy {
                max_memory_mb: 2048,
                max_cpu_seconds: 30,
                max_timeout_seconds: 60,
                max_output_bytes: 100 * 1024 * 1024, // Large for document output
                max_threads: 4,
            },
            environment: ExecutionEnvironment::WorkspaceIsolated,
            audit_logging: true,
            custom_sandbox_profile: None,
        }
    }

    /// Enterprise mode - Maximum security (security: 8-10/10)
    /// Full platform sandboxing with strict limits
    pub fn enterprise() -> Self {
        Self {
            name: "Enterprise".to_string(),
            description: "Maximum security with platform sandboxing and strict limits.".to_string(),
            network: NetworkPolicy::Blocked,
            filesystem: FilesystemPolicy::WorkspaceOnly,
            process: ProcessPolicy::Blocked,
            imports: ImportPolicyType::Whitelist(ImportPolicyType::data_science_whitelist()),
            resources: ResourceLimitsPolicy {
                max_memory_mb: 1024,
                max_cpu_seconds: 15,
                max_timeout_seconds: 30,
                max_output_bytes: 10 * 1024 * 1024,
                max_threads: 2,
            },
            environment: ExecutionEnvironment::PlatformSandboxed,
            audit_logging: true,
            custom_sandbox_profile: None,
        }
    }

    /// Create a custom policy starting from a template
    pub fn custom(base: Self) -> SandboxPolicyBuilder {
        SandboxPolicyBuilder { policy: base }
    }
}

/// Builder for creating custom policies
pub struct SandboxPolicyBuilder {
    policy: SandboxPolicy,
}

impl SandboxPolicyBuilder {
    pub fn name(mut self, name: &str) -> Self {
        self.policy.name = name.to_string();
        self
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.policy.description = desc.to_string();
        self
    }

    pub fn network(mut self, policy: NetworkPolicy) -> Self {
        self.policy.network = policy;
        self
    }

    pub fn filesystem(mut self, policy: FilesystemPolicy) -> Self {
        self.policy.filesystem = policy;
        self
    }

    pub fn process(mut self, policy: ProcessPolicy) -> Self {
        self.policy.process = policy;
        self
    }

    pub fn imports(mut self, policy: ImportPolicyType) -> Self {
        self.policy.imports = policy;
        self
    }

    pub fn resources(mut self, policy: ResourceLimitsPolicy) -> Self {
        self.policy.resources = policy;
        self
    }

    pub fn environment(mut self, env: ExecutionEnvironment) -> Self {
        self.policy.environment = env;
        self
    }

    pub fn audit_logging(mut self, enabled: bool) -> Self {
        self.policy.audit_logging = enabled;
        self
    }

    pub fn custom_sandbox_profile(mut self, path: PathBuf) -> Self {
        self.policy.custom_sandbox_profile = Some(path);
        self
    }

    pub fn build(self) -> SandboxPolicy {
        self.policy
    }
}

// ============================================================================
// ENTERPRISE POLICY - Org-wide enforcement
// ============================================================================

/// Enterprise policy that can lock certain settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnterprisePolicy {
    /// Minimum required security level (0-10)
    pub minimum_security_level: Option<u8>,

    /// Locked network policy (user cannot change)
    pub locked_network: Option<NetworkPolicy>,

    /// Locked filesystem policy
    pub locked_filesystem: Option<FilesystemPolicy>,

    /// Locked process policy
    pub locked_process: Option<ProcessPolicy>,

    /// Required audit logging
    pub require_audit_logging: bool,

    /// Required platform sandboxing
    pub require_platform_sandbox: bool,

    /// Blocked modules that are ALWAYS blacklisted
    pub always_blocked_modules: HashSet<String>,

    /// Maximum allowed resource limits
    pub max_allowed_resources: Option<ResourceLimitsPolicy>,

    /// Custom message to show users about this policy
    pub policy_message: Option<String>,
}

impl EnterprisePolicy {
    /// Apply enterprise policy to a user policy
    /// Returns Ok(modified_policy) or Err(violation_message)
    pub fn apply(&self, mut user_policy: SandboxPolicy) -> Result<SandboxPolicy, String> {
        // Check minimum security level
        if let Some(min_level) = self.minimum_security_level {
            if user_policy.security_level() < min_level {
                return Err(format!(
                    "Policy '{}' has security level {}/10, but minimum required is {}/10",
                    user_policy.name,
                    user_policy.security_level(),
                    min_level
                ));
            }
        }

        // Apply locked settings
        if let Some(ref network) = self.locked_network {
            user_policy.network = network.clone();
        }

        if let Some(ref filesystem) = self.locked_filesystem {
            user_policy.filesystem = filesystem.clone();
        }

        if let Some(ref process) = self.locked_process {
            user_policy.process = process.clone();
        }

        // Require audit logging
        if self.require_audit_logging {
            user_policy.audit_logging = true;
        }

        // Require platform sandbox
        if self.require_platform_sandbox {
            user_policy.environment = ExecutionEnvironment::PlatformSandboxed;
        }

        // Add always-blocked modules to blacklist
        if !self.always_blocked_modules.is_empty() {
            user_policy.imports = match user_policy.imports {
                ImportPolicyType::Unrestricted => {
                    ImportPolicyType::Blacklist(self.always_blocked_modules.clone())
                }
                ImportPolicyType::Blacklist(mut bl) => {
                    bl.extend(self.always_blocked_modules.clone());
                    ImportPolicyType::Blacklist(bl)
                }
                ImportPolicyType::Whitelist(wl) => {
                    // Remove blocked modules from whitelist
                    let filtered: HashSet<_> = wl
                        .into_iter()
                        .filter(|m| !self.always_blocked_modules.contains(m))
                        .collect();
                    ImportPolicyType::Whitelist(filtered)
                }
                ImportPolicyType::WhitelistWithBlacklist {
                    whitelist,
                    mut blacklist,
                } => {
                    blacklist.extend(self.always_blocked_modules.clone());
                    let filtered: HashSet<_> = whitelist
                        .into_iter()
                        .filter(|m| !self.always_blocked_modules.contains(m))
                        .collect();
                    ImportPolicyType::WhitelistWithBlacklist {
                        whitelist: filtered,
                        blacklist,
                    }
                }
            };
        }

        // Enforce maximum resource limits
        if let Some(ref max_resources) = self.max_allowed_resources {
            user_policy.resources.max_memory_mb = user_policy
                .resources
                .max_memory_mb
                .min(max_resources.max_memory_mb);
            user_policy.resources.max_cpu_seconds = user_policy
                .resources
                .max_cpu_seconds
                .min(max_resources.max_cpu_seconds);
            user_policy.resources.max_timeout_seconds = user_policy
                .resources
                .max_timeout_seconds
                .min(max_resources.max_timeout_seconds);
            user_policy.resources.max_output_bytes = user_policy
                .resources
                .max_output_bytes
                .min(max_resources.max_output_bytes);
            user_policy.resources.max_threads = user_policy
                .resources
                .max_threads
                .min(max_resources.max_threads);
        }

        Ok(user_policy)
    }

    /// Example enterprise policy for a financial services company
    pub fn financial_services_example() -> Self {
        Self {
            minimum_security_level: Some(7),
            locked_network: Some(NetworkPolicy::Blocked),
            locked_process: Some(ProcessPolicy::Blocked),
            require_audit_logging: true,
            require_platform_sandbox: true,
            always_blocked_modules: [
                "os",
                "subprocess",
                "socket",
                "urllib",
                "requests",
                "ftplib",
                "smtplib",
                "telnetlib",
                "http.client",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            max_allowed_resources: Some(ResourceLimitsPolicy {
                max_memory_mb: 2048,
                max_cpu_seconds: 30,
                max_timeout_seconds: 60,
                max_output_bytes: 10 * 1024 * 1024,
                max_threads: 4,
            }),
            policy_message: Some(
                "This system is configured for financial services compliance. \
                 Network access and subprocess execution are disabled."
                    .to_string(),
            ),
            ..Default::default()
        }
    }

    /// Example enterprise policy for healthcare
    pub fn healthcare_example() -> Self {
        Self {
            minimum_security_level: Some(8),
            locked_network: Some(NetworkPolicy::Blocked),
            locked_filesystem: Some(FilesystemPolicy::WorkspaceOnly),
            locked_process: Some(ProcessPolicy::Blocked),
            require_audit_logging: true,
            require_platform_sandbox: true,
            always_blocked_modules: [
                "os",
                "subprocess",
                "socket",
                "urllib",
                "requests",
                "ftplib",
                "smtplib",
                "telnetlib",
                "http.client",
                "pickle", // Prevent deserialization attacks
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            max_allowed_resources: Some(ResourceLimitsPolicy {
                max_memory_mb: 1024,
                max_cpu_seconds: 15,
                max_timeout_seconds: 30,
                max_output_bytes: 5 * 1024 * 1024,
                max_threads: 2,
            }),
            policy_message: Some(
                "HIPAA-compliant configuration. All code execution is audited and sandboxed."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

// ============================================================================
// POLICY MANAGER - Loads and manages policies
// ============================================================================

/// Manages policy loading, caching, and application
pub struct PolicyManager {
    /// Enterprise policy (loaded from config)
    enterprise_policy: Option<EnterprisePolicy>,
    /// Available templates
    templates: std::collections::HashMap<String, SandboxPolicy>,
    /// User's selected policy name
    selected_policy: String,
}

impl PolicyManager {
    /// Create a new policy manager
    pub fn new() -> Self {
        let mut templates = std::collections::HashMap::new();
        templates.insert("yolo".to_string(), SandboxPolicy::yolo());
        templates.insert("balanced".to_string(), SandboxPolicy::balanced());
        templates.insert("data_science".to_string(), SandboxPolicy::data_science());
        templates.insert(
            "document_processing".to_string(),
            SandboxPolicy::document_processing(),
        );
        templates.insert("enterprise".to_string(), SandboxPolicy::enterprise());

        Self {
            enterprise_policy: None,
            templates,
            selected_policy: "balanced".to_string(),
        }
    }

    /// Load enterprise policy from a JSON file
    pub fn load_enterprise_policy(&mut self, path: &PathBuf) -> Result<(), String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read enterprise policy: {}", e))?;
        let policy: EnterprisePolicy = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse enterprise policy: {}", e))?;
        self.enterprise_policy = Some(policy);
        Ok(())
    }

    /// Set enterprise policy directly
    pub fn set_enterprise_policy(&mut self, policy: EnterprisePolicy) {
        self.enterprise_policy = Some(policy);
    }

    /// Add a custom template
    pub fn add_template(&mut self, name: &str, policy: SandboxPolicy) {
        self.templates.insert(name.to_string(), policy);
    }

    /// List available templates
    pub fn list_templates(&self) -> Vec<(&str, &str, u8)> {
        self.templates
            .iter()
            .map(|(name, policy)| {
                (
                    name.as_str(),
                    policy.description.as_str(),
                    policy.security_level(),
                )
            })
            .collect()
    }

    /// Select a policy by name
    pub fn select_policy(&mut self, name: &str) -> Result<(), String> {
        if self.templates.contains_key(name) {
            self.selected_policy = name.to_string();
            Ok(())
        } else {
            Err(format!(
                "Unknown policy template: {}. Available: {:?}",
                name,
                self.templates.keys().collect::<Vec<_>>()
            ))
        }
    }

    /// Get the effective policy (user policy with enterprise constraints applied)
    pub fn get_effective_policy(&self) -> Result<SandboxPolicy, String> {
        let user_policy = self
            .templates
            .get(&self.selected_policy)
            .cloned()
            .unwrap_or_else(SandboxPolicy::balanced);

        match &self.enterprise_policy {
            Some(enterprise) => enterprise.apply(user_policy),
            None => Ok(user_policy),
        }
    }

    /// Get enterprise policy message if any
    pub fn get_enterprise_message(&self) -> Option<&str> {
        self.enterprise_policy
            .as_ref()
            .and_then(|p| p.policy_message.as_deref())
    }
}

impl Default for PolicyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_levels() {
        assert_eq!(SandboxPolicy::yolo().security_level(), 0);
        assert_eq!(SandboxPolicy::balanced().security_level(), 7);
        assert!(SandboxPolicy::enterprise().security_level() >= 8);
    }

    #[test]
    fn test_enterprise_enforcement() {
        let enterprise = EnterprisePolicy {
            minimum_security_level: Some(5),
            require_audit_logging: true,
            ..Default::default()
        };

        // YOLO should fail minimum security
        let result = enterprise.apply(SandboxPolicy::yolo());
        assert!(result.is_err());

        // Balanced should pass and get audit logging enabled
        let result = enterprise.apply(SandboxPolicy::balanced());
        assert!(result.is_ok());
        assert!(result.unwrap().audit_logging);
    }

    #[test]
    fn test_policy_builder() {
        let policy = SandboxPolicy::custom(SandboxPolicy::balanced())
            .name("My Custom Policy")
            .network(NetworkPolicy::LocalhostOnly)
            .audit_logging(true)
            .build();

        assert_eq!(policy.name, "My Custom Policy");
        assert_eq!(policy.network, NetworkPolicy::LocalhostOnly);
        assert!(policy.audit_logging);
    }
}
