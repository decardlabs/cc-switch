const UNAVAILABLE_REASON: &str =
    "当前平台尚未支持原生 FIDO2 实现（native backend 占位中）";

use super::{unsupported_platform_probe, PlatformProbe};

pub fn probe() -> PlatformProbe {
    unsupported_platform_probe(std::env::consts::OS, UNAVAILABLE_REASON)
}
