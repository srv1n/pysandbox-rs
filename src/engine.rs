use crate::errors::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Options for Python code execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOptions {
    /// Maximum memory in MB
    pub memory_mb: usize,
    /// Maximum CPU time in seconds
    pub cpu_seconds: u64,
    /// Wall clock timeout
    pub timeout: Duration,
    /// Import policy
    pub import_policy: crate::config::ImportPolicy,
    /// Environment variables to set
    pub env_vars: std::collections::HashMap<String, String>,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            memory_mb: 2048,
            cpu_seconds: 30,
            timeout: Duration::from_secs(35),
            import_policy: crate::config::ImportPolicy::default(),
            env_vars: std::collections::HashMap::new(),
        }
    }
}

/// Capabilities of a Python execution engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineCapabilities {
    /// Engine name
    pub name: String,
    /// Whether numpy is available
    pub numpy: bool,
    /// Whether matplotlib is available  
    pub matplotlib: bool,
    /// Whether pandas is available
    pub pandas: bool,
    /// Maximum memory in MB
    pub max_memory_mb: usize,
    /// Maximum CPU time in seconds
    pub max_cpu_seconds: u64,
    /// Security level (0-10, 10 being most secure)
    pub security_level: u8,
}

/// Trait for Python execution engines
#[async_trait]
pub trait PythonEngine: Send + Sync {
    /// Validate code before execution
    async fn validate(&self, code: &str, options: &ExecutionOptions) -> Result<()>;

    /// Execute Python code
    async fn execute(
        &mut self,
        code: &str,
        inputs: serde_json::Value,
        options: &ExecutionOptions,
    ) -> Result<serde_json::Value>;

    /// Get engine capabilities
    fn capabilities(&self) -> EngineCapabilities;

    /// Shutdown the engine
    async fn shutdown(&mut self) -> Result<()>;
}
