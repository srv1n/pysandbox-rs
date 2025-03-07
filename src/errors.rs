use thiserror::Error;

#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("Syntax error in Python code: {0}")]
    SyntaxError(String),
    #[error("Disallowed operation detected: {0}")]
    DisallowedOperation(String),
    #[error("Runtime error during execution: {0}")]
    RuntimeError(String),
    #[error("Internal error: {0}")]
    InternalError(String),
}