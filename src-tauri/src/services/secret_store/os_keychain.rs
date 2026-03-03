use crate::app_config::AppType;
use crate::error::AppError;

const KEYCHAIN_SERVICE: &str = "cc-switch";

fn account(app: &AppType, provider_id: &str) -> String {
    format!("provider-secret:{}:{provider_id}", app.as_str())
}

fn entry(app: &AppType, provider_id: &str) -> Result<keyring::Entry, AppError> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &account(app, provider_id))
        .map_err(|e| AppError::Message(format!("创建 keychain 条目失败: {e}")))
}

pub fn backend_name() -> &'static str {
    "os-keychain"
}

pub fn is_available() -> bool {
    true
}

pub fn set_secret(app: &AppType, provider_id: &str, secret: &str) -> Result<(), AppError> {
    let keychain_entry = entry(app, provider_id)?;
    keychain_entry
        .set_password(secret)
        .map_err(|e| AppError::Message(format!("写入 keychain 失败: {e}")))
}

pub fn has_secret(app: &AppType, provider_id: &str) -> Result<bool, AppError> {
    let keychain_entry = entry(app, provider_id)?;
    match keychain_entry.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(AppError::Message(format!("读取 keychain 失败: {e}"))),
    }
}

pub fn get_secret(app: &AppType, provider_id: &str) -> Result<Option<String>, AppError> {
    let keychain_entry = entry(app, provider_id)?;
    match keychain_entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::Message(format!("读取 keychain 失败: {e}"))),
    }
}

pub fn delete_secret(app: &AppType, provider_id: &str) -> Result<(), AppError> {
    let keychain_entry = entry(app, provider_id)?;
    match keychain_entry.delete_password() {
        Ok(_) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Message(format!("删除 keychain 条目失败: {e}"))),
    }
}
