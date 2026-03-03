use crate::app_config::AppType;
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

mod native;
pub use native::NativeFido2Capability;

const ENV_FIDO2_BACKEND: &str = "CC_SWITCH_FIDO2_BACKEND";
const ENV_FIDO2_REQUIRE_REASON: &str = "CC_SWITCH_FIDO2_REQUIRE_REASON";
const EMULATED_ASSERTION_SIGNATURE: &str = "emulated-ok";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fido2Backend {
    Disabled,
    Emulated,
    Native,
}

impl Fido2Backend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Fido2Backend::Disabled => "fido2-disabled",
            Fido2Backend::Emulated => "fido2-emulated",
            Fido2Backend::Native => "fido2-native",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fido2AssertionChallenge {
    pub backend: String,
    pub challenge_id: String,
    pub challenge: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fido2AssertionResponse {
    pub challenge_id: String,
    pub signature: String,
}

#[derive(Debug, Clone)]
struct EmulatedChallengeState {
    expires_at: i64,
}

static EMULATED_CHALLENGES: OnceLock<Mutex<HashMap<String, EmulatedChallengeState>>> =
    OnceLock::new();

fn emulated_challenges() -> &'static Mutex<HashMap<String, EmulatedChallengeState>> {
    EMULATED_CHALLENGES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

trait Fido2BackendDriver {
    fn is_available(&self) -> bool;
    fn backend_name(&self) -> &'static str;
    fn unavailable_reason(&self) -> &'static str;
    fn verify_unlock(
        &self,
        app: &AppType,
        provider_id: &str,
        reason: Option<&str>,
    ) -> Result<(), AppError>;
    fn begin_assertion(
        &self,
        app: &AppType,
        provider_id: &str,
        reason: Option<&str>,
    ) -> Result<Fido2AssertionChallenge, AppError>;
    fn verify_assertion(
        &self,
        app: &AppType,
        provider_id: &str,
        response: &Fido2AssertionResponse,
    ) -> Result<(), AppError>;
}

struct DisabledBackend;

impl Fido2BackendDriver for DisabledBackend {
    fn is_available(&self) -> bool {
        false
    }

    fn backend_name(&self) -> &'static str {
        Fido2Backend::Disabled.as_str()
    }

    fn unavailable_reason(&self) -> &'static str {
        "当前未启用原生 FIDO2 后端；可通过环境变量 CC_SWITCH_FIDO2_BACKEND=emulated 启用仿真门禁用于联调"
    }

    fn verify_unlock(
        &self,
        _app: &AppType,
        _provider_id: &str,
        _reason: Option<&str>,
    ) -> Result<(), AppError> {
        Err(AppError::Message(self.unavailable_reason().to_string()))
    }

    fn begin_assertion(
        &self,
        _app: &AppType,
        _provider_id: &str,
        _reason: Option<&str>,
    ) -> Result<Fido2AssertionChallenge, AppError> {
        Err(AppError::Message(self.unavailable_reason().to_string()))
    }

    fn verify_assertion(
        &self,
        _app: &AppType,
        _provider_id: &str,
        _response: &Fido2AssertionResponse,
    ) -> Result<(), AppError> {
        Err(AppError::Message(self.unavailable_reason().to_string()))
    }
}

struct EmulatedBackend;
struct NativeBackend;

impl EmulatedBackend {
    fn ensure_reason_if_required(reason: Option<&str>) -> Result<(), AppError> {
        let require_reason = std::env::var(ENV_FIDO2_REQUIRE_REASON)
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);

        if !require_reason {
            return Ok(());
        }

        let has_reason = reason
            .map(str::trim)
            .map(|value| !value.is_empty())
            .unwrap_or(false);

        if has_reason {
            Ok(())
        } else {
            Err(AppError::InvalidInput(
                "FIDO2 仿真模式要求提供解锁原因（reason）".to_string(),
            ))
        }
    }
}

impl Fido2BackendDriver for EmulatedBackend {
    fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &'static str {
        Fido2Backend::Emulated.as_str()
    }

    fn unavailable_reason(&self) -> &'static str {
        ""
    }

    fn verify_unlock(
        &self,
        _app: &AppType,
        _provider_id: &str,
        reason: Option<&str>,
    ) -> Result<(), AppError> {
        Self::ensure_reason_if_required(reason)
    }

    fn begin_assertion(
        &self,
        _app: &AppType,
        _provider_id: &str,
        reason: Option<&str>,
    ) -> Result<Fido2AssertionChallenge, AppError> {
        Self::ensure_reason_if_required(reason)?;

        let challenge_id = Uuid::new_v4().to_string();
        let challenge = format!("emulated:{}", Uuid::new_v4());
        let expires_at = now_unix_seconds() + 120;

        let mut guard = emulated_challenges().lock()?;
        guard.insert(challenge_id.clone(), EmulatedChallengeState { expires_at });

        Ok(Fido2AssertionChallenge {
            backend: self.backend_name().to_string(),
            challenge_id,
            challenge,
            expires_at,
        })
    }

    fn verify_assertion(
        &self,
        _app: &AppType,
        _provider_id: &str,
        response: &Fido2AssertionResponse,
    ) -> Result<(), AppError> {
        let mut guard = emulated_challenges().lock()?;
        let Some(state) = guard.remove(&response.challenge_id) else {
            return Err(AppError::InvalidInput(
                "无效或已过期的 FIDO2 challengeId".to_string(),
            ));
        };

        if state.expires_at < now_unix_seconds() {
            return Err(AppError::InvalidInput(
                "FIDO2 challenge 已过期，请重新发起".to_string(),
            ));
        }

        if response.signature.trim() != EMULATED_ASSERTION_SIGNATURE {
            return Err(AppError::InvalidInput(
                "FIDO2 仿真断言签名无效".to_string(),
            ));
        }

        Ok(())
    }
}

impl Fido2BackendDriver for NativeBackend {
    fn is_available(&self) -> bool {
        native::is_available()
    }

    fn backend_name(&self) -> &'static str {
        Fido2Backend::Native.as_str()
    }

    fn unavailable_reason(&self) -> &'static str {
        native::unavailable_reason()
    }

    fn verify_unlock(
        &self,
        app: &AppType,
        provider_id: &str,
        reason: Option<&str>,
    ) -> Result<(), AppError> {
        native::verify_unlock(app, provider_id, reason)
    }

    fn begin_assertion(
        &self,
        app: &AppType,
        provider_id: &str,
        reason: Option<&str>,
    ) -> Result<Fido2AssertionChallenge, AppError> {
        native::begin_assertion(app, provider_id, reason)
    }

    fn verify_assertion(
        &self,
        app: &AppType,
        provider_id: &str,
        response: &Fido2AssertionResponse,
    ) -> Result<(), AppError> {
        native::verify_assertion(app, provider_id, response)
    }
}

fn current_driver() -> Box<dyn Fido2BackendDriver> {
    match current_backend() {
        Fido2Backend::Disabled => Box::new(DisabledBackend),
        Fido2Backend::Emulated => Box::new(EmulatedBackend),
        Fido2Backend::Native => Box::new(NativeBackend),
    }
}

pub fn current_backend() -> Fido2Backend {
    match std::env::var(ENV_FIDO2_BACKEND)
        .ok()
        .unwrap_or_default()
        .trim()
        .to_lowercase()
        .as_str()
    {
        "emulated" | "emulator" | "mock" => Fido2Backend::Emulated,
        "native" => Fido2Backend::Native,
        _ => Fido2Backend::Disabled,
    }
}

pub fn backend_name() -> &'static str {
    current_driver().backend_name()
}

pub fn is_available() -> bool {
    current_driver().is_available()
}

pub fn unavailable_reason() -> &'static str {
    current_driver().unavailable_reason()
}

pub fn verify_unlock(
    app: &AppType,
    provider_id: &str,
    reason: Option<&str>,
) -> Result<(), AppError> {
    current_driver().verify_unlock(app, provider_id, reason)
}

pub fn begin_assertion(
    app: &AppType,
    provider_id: &str,
    reason: Option<&str>,
) -> Result<Fido2AssertionChallenge, AppError> {
    current_driver().begin_assertion(app, provider_id, reason)
}

pub fn verify_assertion(
    app: &AppType,
    provider_id: &str,
    response: &Fido2AssertionResponse,
) -> Result<(), AppError> {
    current_driver().verify_assertion(app, provider_id, response)
}

pub fn probe_native_capability() -> NativeFido2Capability {
    native::probe_capability()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_defaults_to_disabled() {
        std::env::remove_var(ENV_FIDO2_BACKEND);
        assert_eq!(current_backend(), Fido2Backend::Disabled);
        assert!(!is_available());
    }

    #[test]
    fn backend_emulated_is_available() {
        std::env::set_var(ENV_FIDO2_BACKEND, "emulated");
        assert_eq!(current_backend(), Fido2Backend::Emulated);
        assert!(is_available());
        std::env::remove_var(ENV_FIDO2_BACKEND);
    }

    #[test]
    fn backend_native_currently_reports_unavailable() {
        std::env::set_var(ENV_FIDO2_BACKEND, "native");
        assert_eq!(current_backend(), Fido2Backend::Native);
        assert!(!is_available());
        assert!(unavailable_reason().contains("尚未启用"));
        std::env::remove_var(ENV_FIDO2_BACKEND);
    }

    #[test]
    fn backend_native_unavailable_error_is_structured_json() {
        std::env::set_var(ENV_FIDO2_BACKEND, "native");

        let err = verify_unlock(&AppType::Claude, "provider-a", Some("unlock test"))
            .expect_err("native backend should currently be unavailable");
        let msg = err.to_string();
        assert!(msg.contains("FIDO2_NATIVE_"));
        assert!(msg.contains("\"context\""));

        std::env::remove_var(ENV_FIDO2_BACKEND);
    }

    #[test]
    fn emulated_assertion_challenge_roundtrip() {
        std::env::set_var(ENV_FIDO2_BACKEND, "emulated");

        let challenge = begin_assertion(&AppType::Claude, "provider-a", Some("unlock test"))
            .expect("begin assertion should succeed");

        let response = Fido2AssertionResponse {
            challenge_id: challenge.challenge_id,
            signature: EMULATED_ASSERTION_SIGNATURE.to_string(),
        };

        verify_assertion(&AppType::Claude, "provider-a", &response)
            .expect("verify assertion should succeed");

        std::env::remove_var(ENV_FIDO2_BACKEND);
    }
}
