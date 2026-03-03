pub mod fido2;
pub mod memory_cache;
pub mod os_keychain;

use crate::app_config::AppType;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecretPolicy {
    Plain,
    OsKeychain,
    Fido2Required,
}

impl SecretPolicy {
    pub fn parse(input: Option<&str>) -> Self {
        match input.unwrap_or_default().trim().to_lowercase().as_str() {
            "fido2_required" | "fido2" => SecretPolicy::Fido2Required,
            "os_keychain" | "keychain" => SecretPolicy::OsKeychain,
            _ => SecretPolicy::Plain,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollResult {
    pub app: String,
    pub provider_id: String,
    pub requested_policy: SecretPolicy,
    pub supported: bool,
    pub backend: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnlockTicket {
    pub ticket: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretStatus {
    pub app: String,
    pub provider_id: String,
    pub policy: SecretPolicy,
    pub has_secret: bool,
    pub can_use_fido2: bool,
    pub last_unlocked_at: Option<i64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
struct SecretRecord {
    policy: SecretPolicy,
    last_unlocked_at: Option<i64>,
}

static SECRET_RECORDS: OnceLock<Mutex<HashMap<String, SecretRecord>>> = OnceLock::new();

fn records() -> &'static Mutex<HashMap<String, SecretRecord>> {
    SECRET_RECORDS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn record_key(app: &AppType, provider_id: &str) -> String {
    format!("{}::{provider_id}", app.as_str())
}

pub struct SecretStoreService;

impl SecretStoreService {
    fn is_policy_supported(policy: &SecretPolicy) -> bool {
        match policy {
            SecretPolicy::Fido2Required => fido2::is_available(),
            SecretPolicy::OsKeychain => os_keychain::is_available(),
            SecretPolicy::Plain => true,
        }
    }

    fn has_secret(app: &AppType, provider_id: &str, policy: &SecretPolicy) -> Result<bool, AppError> {
        let key = record_key(app, provider_id);
        match policy {
            SecretPolicy::Plain => memory_cache::has_secret(&key),
            SecretPolicy::OsKeychain | SecretPolicy::Fido2Required => {
                os_keychain::has_secret(app, provider_id)
            }
        }
    }

    fn write_secret(
        app: &AppType,
        provider_id: &str,
        api_key: &str,
        policy: &SecretPolicy,
    ) -> Result<(), AppError> {
        let key = record_key(app, provider_id);
        match policy {
            SecretPolicy::Plain => {
                memory_cache::set_secret(&key, api_key)?;
                let _ = os_keychain::delete_secret(app, provider_id);
                Ok(())
            }
            SecretPolicy::OsKeychain | SecretPolicy::Fido2Required => {
                os_keychain::set_secret(app, provider_id, api_key)?;
                let _ = memory_cache::delete_secret(&key);
                Ok(())
            }
        }
    }

    fn infer_policy(app: &AppType, provider_id: &str) -> SecretPolicy {
        let key = record_key(app, provider_id);
        if memory_cache::has_secret(&key).unwrap_or(false) {
            return SecretPolicy::Plain;
        }

        if os_keychain::has_secret(app, provider_id).unwrap_or(false) {
            return SecretPolicy::OsKeychain;
        }

        SecretPolicy::Plain
    }

    pub fn read_secret(
        app: AppType,
        provider_id: &str,
        policy_hint: Option<SecretPolicy>,
    ) -> Result<Option<String>, AppError> {
        if provider_id.trim().is_empty() {
            return Err(AppError::InvalidInput("providerId 不能为空".to_string()));
        }

        let policy = policy_hint.unwrap_or_else(|| Self::infer_policy(&app, provider_id));
        let key = record_key(&app, provider_id);
        match policy {
            SecretPolicy::Plain => memory_cache::get_secret(&key),
            SecretPolicy::OsKeychain | SecretPolicy::Fido2Required => {
                os_keychain::get_secret(&app, provider_id)
            }
        }
    }

    pub fn enroll(
        app: AppType,
        provider_id: &str,
        requested_policy: SecretPolicy,
    ) -> Result<EnrollResult, AppError> {
        if provider_id.trim().is_empty() {
            return Err(AppError::InvalidInput("providerId 不能为空".to_string()));
        }

        let can_use_fido2 = fido2::is_available();
        let supported = Self::is_policy_supported(&requested_policy);
        let message = if requested_policy == SecretPolicy::Fido2Required && !can_use_fido2 {
            fido2::unavailable_reason().to_string()
        } else if requested_policy == SecretPolicy::OsKeychain && !os_keychain::is_available() {
            "当前平台暂不可用系统钥匙串，将回退内存策略".to_string()
        } else {
            "SecretStore 已就绪".to_string()
        };

        Ok(EnrollResult {
            app: app.as_str().to_string(),
            provider_id: provider_id.to_string(),
            requested_policy,
            supported,
            backend: if can_use_fido2 && supported {
                fido2::backend_name().to_string()
            } else if os_keychain::is_available() {
                os_keychain::backend_name().to_string()
            } else {
                memory_cache::cache_name().to_string()
            },
            message,
        })
    }

    pub fn bind_secret(
        app: AppType,
        provider_id: &str,
        api_key: &str,
        policy: SecretPolicy,
    ) -> Result<SecretStatus, AppError> {
        if provider_id.trim().is_empty() {
            return Err(AppError::InvalidInput("providerId 不能为空".to_string()));
        }
        if api_key.trim().is_empty() {
            return Err(AppError::InvalidInput("apiKey 不能为空".to_string()));
        }

        let effective_policy = if Self::is_policy_supported(&policy) {
            policy
        } else if os_keychain::is_available() {
            SecretPolicy::OsKeychain
        } else {
            SecretPolicy::Plain
        };

        Self::write_secret(&app, provider_id, api_key, &effective_policy)?;

        let key = record_key(&app, provider_id);
        let mut guard = records().lock()?;
        guard.insert(
            key,
            SecretRecord {
                policy: effective_policy.clone(),
                last_unlocked_at: None,
            },
        );

        Ok(SecretStatus {
            app: app.as_str().to_string(),
            provider_id: provider_id.to_string(),
            policy: effective_policy,
            has_secret: true,
            can_use_fido2: fido2::is_available(),
            last_unlocked_at: None,
            message: Some("密钥已绑定到安全存储".to_string()),
        })
    }

    pub fn unlock_secret(
        app: AppType,
        provider_id: &str,
        reason: Option<&str>,
    ) -> Result<UnlockTicket, AppError> {
        let inferred_policy = Self::infer_policy(&app, provider_id);
        if !Self::has_secret(&app, provider_id, &inferred_policy)? {
            return Err(AppError::InvalidInput("该 provider 尚未绑定密钥".to_string()));
        }

        if inferred_policy == SecretPolicy::Fido2Required {
            fido2::verify_unlock(&app, provider_id, reason)?;
        }

        let key = record_key(&app, provider_id);
        let mut guard = records().lock()?;
        let record = guard.entry(key).or_insert(SecretRecord {
            policy: inferred_policy,
            last_unlocked_at: None,
        });

        let now = now_unix_seconds();
        record.last_unlocked_at = Some(now);

        Ok(UnlockTicket {
            ticket: format!("phase0:{}:{}:{now}", app.as_str(), provider_id),
            expires_at: now + 300,
        })
    }

    pub fn rotate_secret(
        app: AppType,
        provider_id: &str,
        new_api_key: &str,
    ) -> Result<SecretStatus, AppError> {
        if new_api_key.trim().is_empty() {
            return Err(AppError::InvalidInput("newApiKey 不能为空".to_string()));
        }

        let inferred_policy = Self::infer_policy(&app, provider_id);
        if !Self::has_secret(&app, provider_id, &inferred_policy)? {
            return Err(AppError::InvalidInput("该 provider 尚未绑定密钥".to_string()));
        }
        Self::write_secret(&app, provider_id, new_api_key, &inferred_policy)?;

        let key = record_key(&app, provider_id);
        let mut guard = records().lock()?;
        let record = guard.entry(key).or_insert(SecretRecord {
            policy: inferred_policy.clone(),
            last_unlocked_at: None,
        });
        record.policy = inferred_policy.clone();

        Ok(SecretStatus {
            app: app.as_str().to_string(),
            provider_id: provider_id.to_string(),
            policy: inferred_policy,
            has_secret: true,
            can_use_fido2: fido2::is_available(),
            last_unlocked_at: record.last_unlocked_at,
            message: Some("密钥已轮换".to_string()),
        })
    }

    pub fn delete_secret(app: AppType, provider_id: &str) -> Result<(), AppError> {
        if provider_id.trim().is_empty() {
            return Err(AppError::InvalidInput("providerId 不能为空".to_string()));
        }

        let key = record_key(&app, provider_id);
        let mut first_error: Option<AppError> = None;

        if let Err(e) = memory_cache::delete_secret(&key) {
            first_error = Some(e);
        }

        if let Err(e) = os_keychain::delete_secret(&app, provider_id) {
            if first_error.is_none() {
                first_error = Some(e);
            }
        }

        let mut guard = records().lock()?;
        guard.remove(&key);

        if let Some(e) = first_error {
            return Err(e);
        }

        Ok(())
    }

    pub fn get_status(app: AppType, provider_id: &str) -> Result<SecretStatus, AppError> {
        let key = record_key(&app, provider_id);
        let guard = records().lock()?;
        if let Some(record) = guard.get(&key) {
            let has_secret = Self::has_secret(&app, provider_id, &record.policy)?;
            return Ok(SecretStatus {
                app: app.as_str().to_string(),
                provider_id: provider_id.to_string(),
                policy: record.policy.clone(),
                has_secret,
                can_use_fido2: fido2::is_available(),
                last_unlocked_at: record.last_unlocked_at,
                message: None,
            });
        }

        let inferred_policy = Self::infer_policy(&app, provider_id);
        let has_secret = Self::has_secret(&app, provider_id, &inferred_policy)?;
        if has_secret {
            return Ok(SecretStatus {
                app: app.as_str().to_string(),
                provider_id: provider_id.to_string(),
                policy: inferred_policy,
                has_secret: true,
                can_use_fido2: fido2::is_available(),
                last_unlocked_at: None,
                message: None,
            });
        }

        Ok(SecretStatus {
            app: app.as_str().to_string(),
            provider_id: provider_id.to_string(),
            policy: SecretPolicy::Plain,
            has_secret: false,
            can_use_fido2: fido2::is_available(),
            last_unlocked_at: None,
            message: Some("未绑定密钥".to_string()),
        })
    }
}
