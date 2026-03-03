use crate::app_config::AppType;
use crate::error::{format_skill_error, AppError};
use serde::{Deserialize, Serialize};

use super::{Fido2AssertionChallenge, Fido2AssertionResponse};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod unsupported;

#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(target_os = "windows")]
use windows as platform;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
use unsupported as platform;

const CODE_NATIVE_NOT_ENABLED: &str = "FIDO2_NATIVE_NOT_ENABLED";
const CODE_NATIVE_PLATFORM_UNSUPPORTED: &str = "FIDO2_NATIVE_PLATFORM_UNSUPPORTED";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeFido2Capability {
    pub backend: String,
    pub platform: String,
    pub available: bool,
    pub code: Option<String>,
    pub reason: Option<String>,
}

pub(super) struct PlatformProbe {
    pub platform: &'static str,
    pub available: bool,
    pub code: Option<&'static str>,
    pub reason: Option<&'static str>,
}

fn to_unavailable_error(probe: PlatformProbe) -> AppError {
    let code = probe.code.unwrap_or(CODE_NATIVE_NOT_ENABLED);
    let reason = probe.reason.unwrap_or("Native FIDO2 is currently unavailable");

    AppError::Message(format_skill_error(
        code,
        &[("platform", probe.platform), ("reason", reason)],
        Some("暂时切换到 emulated 后端联调，或等待当前平台原生实现发布"),
    ))
}

pub fn probe_capability() -> NativeFido2Capability {
    let probe = platform::probe();
    NativeFido2Capability {
        backend: "fido2-native".to_string(),
        platform: probe.platform.to_string(),
        available: probe.available,
        code: probe.code.map(|value| value.to_string()),
        reason: probe.reason.map(|value| value.to_string()),
    }
}

pub fn is_available() -> bool {
    platform::probe().available
}

pub fn unavailable_reason() -> &'static str {
    platform::probe().reason.unwrap_or("")
}

pub fn verify_unlock(
    app: &AppType,
    provider_id: &str,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let _ = (app, provider_id, reason);
    let probe = platform::probe();
    if probe.available {
        return Ok(());
    }
    Err(to_unavailable_error(probe))
}

pub fn begin_assertion(
    app: &AppType,
    provider_id: &str,
    reason: Option<&str>,
) -> Result<Fido2AssertionChallenge, AppError> {
    let _ = (app, provider_id, reason);
    let probe = platform::probe();
    if probe.available {
        return Err(AppError::Message(
            "Native FIDO2 available but begin_assertion not implemented".to_string(),
        ));
    }
    Err(to_unavailable_error(probe))
}

pub fn verify_assertion(
    app: &AppType,
    provider_id: &str,
    response: &Fido2AssertionResponse,
) -> Result<(), AppError> {
    let _ = (app, provider_id, response);
    let probe = platform::probe();
    if probe.available {
        return Err(AppError::Message(
            "Native FIDO2 available but verify_assertion not implemented".to_string(),
        ));
    }
    Err(to_unavailable_error(probe))
}

pub(super) fn default_platform_probe(platform: &'static str, reason: &'static str) -> PlatformProbe {
    PlatformProbe {
        platform,
        available: false,
        code: Some(CODE_NATIVE_NOT_ENABLED),
        reason: Some(reason),
    }
}

pub(super) fn unsupported_platform_probe(platform: &'static str, reason: &'static str) -> PlatformProbe {
    PlatformProbe {
        platform,
        available: false,
        code: Some(CODE_NATIVE_PLATFORM_UNSUPPORTED),
        reason: Some(reason),
    }
}
