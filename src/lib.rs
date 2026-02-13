pub mod config;
pub mod engine;
pub mod errors;
pub mod native;
pub mod policy;
pub mod sandbox_builder;
pub mod sandboxed;

#[cfg(feature = "microsandbox-engine")]
pub mod microsandbox_engine;

#[cfg(feature = "microsandbox-engine")]
pub mod microsandbox_setup;

#[cfg(feature = "microsandbox-engine")]
pub mod microsandbox_auth;

pub use config::{ExecutionMode, ImportPolicy, ResourceLimits, SecurityProfile};
pub use engine::{EngineCapabilities, ExecutionOptions, PythonEngine};
pub use errors::{Result, SandboxError};

use std::sync::Arc;
use tokio::sync::RwLock;

/// Main sandbox manager that handles multiple execution engines
pub struct PythonSandbox {
    engines: Vec<Arc<RwLock<Box<dyn PythonEngine>>>>,
    primary_engine: usize,
}

impl PythonSandbox {
    /// Create a new sandbox with the specified engines
    pub fn new(engines: Vec<Box<dyn PythonEngine>>) -> Self {
        Self {
            engines: engines
                .into_iter()
                .map(|e| Arc::new(RwLock::new(e)))
                .collect(),
            primary_engine: 0,
        }
    }

    /// Execute Python code using the primary engine with fallback support
    pub async fn execute(
        &self,
        code: &str,
        inputs: serde_json::Value,
        options: ExecutionOptions,
    ) -> Result<serde_json::Value> {
        let primary = &self.engines[self.primary_engine];

        match primary
            .write()
            .await
            .execute(code, inputs.clone(), &options)
            .await
        {
            Ok(result) => Ok(result),
            Err(e) if self.engines.len() > 1 => {
                tracing::warn!("Primary engine failed: {}, trying fallback", e);
                // Try fallback engines
                for (idx, engine) in self.engines.iter().enumerate() {
                    if idx != self.primary_engine {
                        match engine
                            .write()
                            .await
                            .execute(code, inputs.clone(), &options)
                            .await
                        {
                            Ok(result) => return Ok(result),
                            Err(e) => tracing::warn!("Fallback engine {} failed: {}", idx, e),
                        }
                    }
                }
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// Get capabilities of all engines
    pub async fn capabilities(&self) -> Vec<EngineCapabilities> {
        let mut caps = Vec::new();
        for engine in &self.engines {
            caps.push(engine.read().await.capabilities());
        }
        caps
    }
}

// Re-export sandbox creation functions
pub use sandbox_builder::{
    create_bundled_sandbox, create_default_sandbox, create_sandbox_interactive,
    create_sandbox_with_options, SandboxOptions,
};

// Re-export native engine for direct use
pub use native::NativePythonEngine;

// Re-export sandboxed engine and types
pub use sandboxed::{
    IsolatedWorkspace, SandboxConfig, SandboxedExecutionBuilder, SandboxedExecutionResult,
    SandboxedPythonEngine,
};

// Re-export policy system
pub use policy::{
    // Enterprise
    EnterprisePolicy,
    ExecutionEnvironment,
    FilesystemPolicy,
    ImportPolicyType,
    // Primitives
    NetworkPolicy,
    PolicyManager,
    ProcessPolicy,
    ResourceLimitsPolicy,
    // Policy
    SandboxPolicy,
    SandboxPolicyBuilder,
};
