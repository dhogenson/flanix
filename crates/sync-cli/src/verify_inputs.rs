use std::path::PathBuf;

pub fn validate_namespace(namespace: &str) -> Result<String, String> {
    let trimmed = namespace.trim();
    if trimmed.is_empty() {
        return Err("namespace cannot be empty".into());
    }

    if trimmed.contains("/") || trimmed.contains("..") || trimmed.contains('\0') {
        return Err("namespace contains invalid characters".into());
    }

    Ok(trimmed.to_string())
}

pub fn validate_path(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    if !path.exists() {
        return Err(format!("path does not exist: {}", s));
    }
    if !path.is_dir() {
        return Err(format!("path is not a directory: {}", s));
    }
    Ok(path)
}
