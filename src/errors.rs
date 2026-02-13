use thiserror::Error;

pub type Result<T> = std::result::Result<T, SandboxError>;

#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("Python not installed or not found in PATH")]
    PythonNotFound,

    #[error("No execution engine available")]
    NoEngineAvailable,

    #[error("Syntax error in Python code: {0}")]
    SyntaxError(String),

    #[error("Disallowed operation detected: {0}")]
    DisallowedOperation(String),

    #[error("Runtime error during execution: {0}")]
    RuntimeError(String),

    #[error("Execution timeout exceeded")]
    Timeout,

    #[error("Memory limit exceeded")]
    MemoryLimitExceeded,

    #[error("Process limit exceeded")]
    ProcessLimitExceeded,

    #[error("Import not allowed: {0}")]
    ImportNotAllowed(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Microsandbox error: {0}")]
    MicrosandboxError(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Process exited with code {0}")]
    ProcessExitCode(i32),

    #[error("Process killed by signal")]
    ProcessKilled,
}
