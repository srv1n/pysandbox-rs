use crate::errors::{Result, SandboxError};
use std::fs;
use std::path::PathBuf;

/// Get the microsandbox server key
pub fn get_server_key() -> Result<String> {
    let key_path = get_server_key_path();

    if !key_path.exists() {
        return Err(SandboxError::InternalError(
            "Microsandbox server key not found. Is the server running?".to_string(),
        ));
    }

    fs::read_to_string(&key_path)
        .map(|s| s.trim().to_string())
        .map_err(|e| SandboxError::InternalError(format!("Failed to read server key: {}", e)))
}

/// Get the path to the server key file
fn get_server_key_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());

    PathBuf::from(home).join(".microsandbox").join("server.key")
}

/// Set up authentication for microsandbox client
pub fn setup_auth() -> Result<()> {
    let key = get_server_key()?;

    // The microsandbox crate v0.1.2 specifically looks for MSB_API_KEY
    std::env::set_var("MSB_API_KEY", &key);

    // Also set other variants for compatibility
    std::env::set_var("MICROSANDBOX_KEY", &key);
    std::env::set_var("MSB_KEY", &key);
    std::env::set_var("MICROSANDBOX_SERVER_KEY", &key);
    std::env::set_var("MICROSANDBOX_API_KEY", &key);

    Ok(())
}
