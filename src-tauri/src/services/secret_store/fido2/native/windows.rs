const UNAVAILABLE_REASON: &str =
    "当前版本尚未启用 Windows 原生 FIDO2 实现（native backend 占位中）";

use super::{default_platform_probe, PlatformProbe};

pub fn probe() -> PlatformProbe {
    default_platform_probe("windows", UNAVAILABLE_REASON)
}
