use crate::error::AppError;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, String>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn cache_name() -> &'static str {
    "memory-cache"
}

pub fn set_secret(key: &str, value: &str) -> Result<(), AppError> {
    let mut guard = cache().lock()?;
    guard.insert(key.to_string(), value.to_string());
    Ok(())
}

pub fn has_secret(key: &str) -> Result<bool, AppError> {
    let guard = cache().lock()?;
    Ok(guard.contains_key(key))
}

pub fn get_secret(key: &str) -> Result<Option<String>, AppError> {
    let guard = cache().lock()?;
    Ok(guard.get(key).cloned())
}

pub fn delete_secret(key: &str) -> Result<(), AppError> {
    let mut guard = cache().lock()?;
    guard.remove(key);
    Ok(())
}
