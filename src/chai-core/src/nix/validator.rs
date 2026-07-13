use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[allow(dead_code)]
    #[error("Nix parse error: {0}")]
    NixParse(String),
    #[allow(dead_code)]
    #[error("nix-instantiate binary not found")]
    NixNotFound,
}

#[cfg(unix)]
pub fn validate_and_write(content: &str, name: &str) -> Result<PathBuf, ValidationError> {
    use std::path::Path;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    const GENERATION_DIR: &str = "/tmp/chai-generation";

    let dir = Path::new(GENERATION_DIR);
    std::fs::create_dir_all(dir)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let filename = format!("{}-{}.nix", name, timestamp);
    let path = dir.join(&filename);
    std::fs::write(&path, content)?;

    let output = Command::new("nix-instantiate")
        .arg("--parse")
        .arg(&path)
        .output()
        .map_err(|_| ValidationError::NixNotFound)?;

    if output.status.success() {
        tracing::info!(target: "chai-core::nix", "Nix file validated: {}", path.display());
        Ok(path)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        tracing::error!(target: "chai-core::nix", "Nix validation failed for {}: {}", path.display(), stderr);
        let _ = std::fs::remove_file(&path);
        Err(ValidationError::NixParse(stderr))
    }
}

#[cfg(not(unix))]
pub fn validate_and_write(content: &str, _name: &str) -> Result<PathBuf, ValidationError> {
    tracing::warn!(target: "chai-core::nix", "Nix validation skipped: not on Unix");
    let dir = std::env::temp_dir().join("chai-generation");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("generated.nix");
    std::fs::write(&path, content)?;
    tracing::info!(target: "chai-core::nix", "Nix file written (no validation): {}", path.display());
    Ok(path)
}
