# CC Switch FIDO2 API Key 安全存储改造方案

## 1. 目标与边界

### 1.1 目标

在不破坏现有 provider 切换、proxy、usage、deeplink 功能的前提下，实现：

1. API Key 不再长期以明文持久化在 `providers.settings_config` 中。
2. 使用硬件凭证（FIDO2）做“解锁门禁”，仅在需要时解密密钥。
3. 不支持 FIDO2 的设备可自动降级到系统密钥链（Keychain/Credential Manager/libsecret）。

### 1.2 非目标（第一阶段不做）

1. 不要求“API Key 永不出现在内存”（运行请求时仍会短暂明文存在内存）。
2. 不强行改造第三方 CLI 配置格式（仍需兼容现有 live config 写入流程）。
3. 不做跨设备同步密钥（仅本机本用户）。

---

## 2. 现状分析（代码位置）

当前 API Key 以多种形式散布在 provider 配置中，并被多条业务链路直接读取：

- 数据模型：`src-tauri/src/provider.rs`、`src/types.ts`
- 数据落库：`src-tauri/src/database/schema.rs`（`providers.settings_config` / `meta`）
- Provider 切换与取 key：`src-tauri/src/services/provider/mod.rs`
- Usage 查询：`src-tauri/src/services/provider/usage.rs`、`src-tauri/src/usage_script.rs`
- Proxy 注入：`src-tauri/src/services/proxy.rs`
- 深链导入：`src-tauri/src/deeplink/parser.rs`、`src-tauri/src/deeplink/provider.rs`
- 前端 API 桥：`src/lib/api/providers.ts`

结论：必须做“统一取密钥入口”，否则改造会出现漏网点。

---

## 3. 总体设计

## 3.1 核心思路

采用 **“FIDO2 解锁 + 本地密文存储”** 模型：

1. API Key 密文存储在本地 secret store（不是 provider JSON）。
2. provider 中仅保存 `secretRef`（引用 ID）和策略字段。
3. 读取密钥时先执行解锁（FIDO2 用户验证）再解密返回。

## 3.2 分层

### A. SecretStore 抽象层（Rust）

新增模块建议：`src-tauri/src/services/secret_store/`

- `mod.rs`：trait 与 DTO
- `fido2.rs`：FIDO2 解锁实现
- `os_keychain.rs`：系统钥匙串实现（降级路径）
- `memory_cache.rs`：短时内存缓存（TTL）

统一接口：

```rust
pub trait SecretStore {
    fn enroll(&self, provider_id: &str, app: AppType) -> Result<EnrollResult, AppError>;
    fn put_secret(&self, secret_ref: &str, plaintext: &str) -> Result<(), AppError>;
    fn unlock(&self, secret_ref: &str, reason: UnlockReason) -> Result<UnlockTicket, AppError>;
    fn get_secret(&self, secret_ref: &str, ticket: &UnlockTicket) -> Result<String, AppError>;
    fn rotate(&self, secret_ref: &str, new_plaintext: &str) -> Result<(), AppError>;
    fn delete(&self, secret_ref: &str) -> Result<(), AppError>;
}
```

### B. Provider 数据模型（最小侵入）

在 `ProviderMeta` 增加字段（前后端同步）：

```ts
interface ProviderMeta {
  secretRef?: string;
  secretPolicy?: "plain" | "os_keychain" | "fido2_required";
  secretLastUnlockedAt?: number;
}
```

说明：

- `settingsConfig` 中保留兼容字段，但新写入不再保存明文 key。
- 历史数据按迁移策略逐步转为 `secretRef`。

### C. 统一取密钥入口

在 `ProviderService` 增加唯一入口：

```rust
fn resolve_provider_secret(state: &AppState, app: AppType, provider: &Provider, reason: UnlockReason) -> Result<String, AppError>
```

替换所有直接读 `apiKey`/`*_API_KEY` 的分散逻辑，至少覆盖：

- `services/provider/mod.rs`
- `services/provider/usage.rs`
- `services/proxy.rs`
- `services/stream_check.rs`

---

## 4. Tauri 命令接口草案

新增命令（注册到 `src-tauri/src/lib.rs` 的 `generate_handler![]`）：

1. `enroll_provider_secret_protection(app, providerId, policy)`
2. `bind_provider_secret(app, providerId, apiKey)`
3. `unlock_provider_secret(app, providerId, reason)`
4. `rotate_provider_secret(app, providerId, newApiKey)`
5. `get_provider_secret_status(app, providerId)`

前端 API 封装位置建议：`src/lib/api/providers.ts`

---

## 5. 分阶段实施计划

## Phase 0（准备）

1. 新增 `SecretStore` 抽象与默认实现（先 OS Keychain，确保跨平台可跑）。
2. 新增 `ProviderMeta.secretRef/secretPolicy` 字段定义（TS + Rust）。
3. 所有新增逻辑默认关闭，由设置开关控制（feature flag）。

验收：功能不变，编译/测试通过。

## Phase 1（切换写路径）

1. 新建/编辑 provider 时，将输入 API Key 写入 `SecretStore`。
2. `settings_config` 中 key 字段改为占位或删除。
3. `meta.secretRef` 持久化到 DB。

验收：新增 provider 不再在 DB 明文出现 API Key。

> 进展备注（Phase 0.5）：已提供命令与 UI 骨架，并在 `enroll/bind/unlock/rotate` 成功后自动写入 `provider.meta.secretRef/secretPolicy/secretLastUnlockedAt`，用于后续迁移与状态展示。

> 进展备注（Phase 1-1）：`SecretStore` 已接入系统 Keychain 持久化（`os_keychain` / `fido2_required` 策略），`plain` 策略仍走内存缓存；`get_status` 支持重启后通过 Keychain 识别已绑定状态。

> 进展备注（Phase 1-2）：`ProviderService.extract_credentials` 已接入 SecretStore 回退读取逻辑（配置字段缺失时尝试按 `meta.secretPolicy`/推断策略读取），实现 provider 链路第一阶段兼容过渡。

> 进展备注（Phase 1-3）：`services/provider/usage.rs` 的用量查询已接入同样策略，凭据优先级调整为 `UsageScript 显式值 > SecretStore > 旧配置字段`。

> 进展备注（Phase 1-4）：代理运行链路已接入 SecretStore 回退读取（`proxy/providers/{claude,codex,gemini}.rs`），请求认证读取优先配置字段，缺失时回退 Keychain/SecretStore。

> 进展备注（Phase 1-5）：`ProviderService.switch_normal` 已接入“懒迁移闭环”：检测到旧明文字段时自动写入 SecretStore、更新 `meta.secret*` 并清理数据库明文字段；写 live 配置前会从 SecretStore 临时回填，保持切换行为兼容。

> 进展备注（Phase 1-6）：`services/proxy.rs` 的 `sync_live_config_to_provider` 已由“写回 DB 明文 Token”改为“写入 SecretStore + 更新 `provider.meta.secret*` + 清理 `settings_config` 明文字段”，避免代理接管流程将明文再次持久化。

> 进展备注（Phase 1-7）：`deeplink/provider.rs` 在导入 Claude/Codex/Gemini Provider 时，已改为“先绑定 SecretStore，再清理 `settings_config` 明文字段并写入 `meta.secret*`”；同时默认继承主 API Key 的 `usage_script.api_key` 会在导入时清空，避免重复明文持久化。

> 进展备注（Phase 1-8）：当 deeplink 显式提供 `usageApiKey` 且与主 API Key 不同时，已改为写入独立 SecretStore 引用（`meta.usageSecretRef` / `meta.usageSecretPolicy`，键名为 `providerId::usage`）；`services/provider/usage.rs` 已优先读取该 usage 专用密钥，`usage_script.api_key` 明文将被清空。

> 进展备注（Phase 1-9）：`ProviderService.add/update` 已接入同样的 usage 密钥迁移逻辑：当编辑保存时 `meta.usage_script.api_key` 非空，会自动迁移到 SecretStore（`providerId::usage`）并清空明文，同时写入 `meta.usageSecretRef/usageSecretPolicy`，避免仅 deeplink 路径受保护。

> 进展备注（Phase 1-10）：Provider 删除路径已接入 SecretStore 清理：删除 provider（含统一供应商派生 provider 的删除场景）后会同时清理主密钥引用与 usage 密钥引用（默认 `providerId` / `providerId::usage` 及 meta 中显式引用），避免孤儿密钥残留。

> 进展备注（Phase 1-11）：`ProviderService` 与 `proxy/providers` 的 SecretStore 读取已统一为“优先 `meta.secretRef`，回退 `provider.id`”，避免引用键名变更时出现读取分叉；`stream_check` 通过 adapter 复用该统一读取逻辑。

> 进展备注（Phase 2-1）：`services/stream_check.rs` 已显式接入 `ProviderService::resolve_provider_secret`：检查前会在内存克隆上临时回填密钥，再交由 adapter 提取认证信息，避免读链路仅依赖“间接回退”行为。

> 进展备注（Phase 3-0）：`secret_store/fido2.rs` 已从硬编码占位升级为可探测后端骨架：支持环境变量切换 `CC_SWITCH_FIDO2_BACKEND=emulated`、统一 `backend_name`/`unavailable_reason`、并在 `unlock_secret` 的 `fido2_required` 路径接入 `verify_unlock` 门禁钩子（兼容现有流程，便于后续替换为原生硬件实现）。

> 进展备注（Phase 3-1）：`fido2.rs` 已重构为可插拔驱动结构（`DisabledBackend` / `EmulatedBackend`），并新增 challenge-response 接口骨架（`begin_assertion` / `verify_assertion`，含内存 challenge 会话与过期校验）；业务层仍保持现有 `verify_unlock` 兼容调用，后续接入 native 平台实现时可直接替换 driver。

> 进展备注（Phase 3-2）：已新增 Tauri 命令与前端 API 封装支持 FIDO2 challenge 流程：`begin_provider_secret_fido2_assertion`（发起挑战）与 `verify_provider_secret_fido2_assertion`（校验断言后解锁并回写 `secretLastUnlockedAt`），可用于后续 UI 接入真实触发-验证链路。

> 进展备注（Phase 3-3）：`ProviderSecretPanel` 已接入上述 challenge API，形成最小可用 FIDO2 交互：发起挑战、显示 challengeId、输入签名并验证解锁；当策略为 `fido2_required` 时，普通解锁按钮会提示走挑战流程（仿真模式可用 `emulated-ok` 联调）。

> 进展备注（Phase 3-4）：`ProviderSecretPanel` 增加 challenge 到期倒计时与过期拦截（过期后禁用验证按钮并提示重发挑战），完善 FIDO2 交互可观测性。

> 进展备注（Phase 3-5）：`ProviderSecretPanel` 在 challenge 过期或验证失败后会自动清理本地挑战状态，并按错误类型给出更明确提示（过期 / 签名错误 / 后端不可用），减少用户重试路径歧义。

> 进展备注（Phase 3-6）：`fido2.rs` 已预留 `Native` 后端分支与驱动占位（`CC_SWITCH_FIDO2_BACKEND=native`），并保持与现有 `verify_unlock/begin_assertion/verify_assertion` 调用面一致；后续接入 macOS/Windows/Linux 原生实现时无需修改业务层调用链。

> 进展备注（Phase 3-7）：已完成 Native 后端平台骨架拆分：`secret_store/fido2/native/{macos,windows,linux,unsupported}.rs` + 统一适配入口。`NativeBackend` 已改为转发到该适配层，后续接入平台 SDK 时可按文件逐步落地实现，且不影响现有命令/API/UI 调用面。

> 进展备注（Phase 3-8）：Native 适配层已补齐“能力探测 + 结构化错误码”基础契约：各平台模块统一返回 probe 结果（platform/available/code/reason），并由适配入口将不可用错误格式化为可解析 JSON（`code/context/suggestion`），为后续前端细分提示与平台 SDK 接入提供稳定语义。

> 进展备注（Phase 3-9）：已新增命令 `get_native_fido2_capability` 并注册到 Tauri handler，前端 `providersApi` 也已提供 `getNativeFido2Capability()` 封装；UI 可直接读取 native probe 结果并按 `code` 做分流提示（如平台不支持/实现未启用）。

> 进展备注（Phase 3-10）：`ProviderSecretPanel` 已接入 `getNativeFido2Capability()`，并按结构化错误码细分 FIDO2 提示文案（`FIDO2_NATIVE_PLATFORM_UNSUPPORTED` / `FIDO2_NATIVE_NOT_ENABLED` / 通用不可用）；挑战发起与验证失败路径也复用同一映射逻辑，减少提示歧义。

> 进展备注（Phase 3-11）：已将新增 FIDO2 native 提示词条补充到多语言资源（zh/en/ja）：`provider.secret.fido2UnsupportedPlatform` 与 `provider.secret.fido2NativeNotEnabled`，UI 不再仅依赖 `defaultValue` 回退文案。

> 进展备注（Phase 3-12）：`ProviderSecretPanel` 已将上述两类 native 错误码提示切换为“纯 i18n 键读取”（移除 `defaultValue`），确保多语言文案一致性由词条统一维护。

> 进展备注（Phase 3-13）：`ProviderSecretPanel` 已在开发环境（`import.meta.env.DEV`）额外显示 native probe 的 `reason` 原始信息，用于平台 SDK 联调排查；生产环境保持仅展示规范化用户提示，不暴露调试细节。

> 进展备注（Phase 3-14）：`ProviderSecretPanel` 已将 native probe 结果缓存到面板生命周期（首次打开探测后复用），并在开发环境提供“刷新 FIDO2 探测”手动触发按钮，便于不重开面板快速验证平台实现变更。

> 进展备注（Phase 3-15）：`get_native_fido2_capability` 命令已增加轻量诊断日志（`backend/platform/available/code`），用于回归时快速确认平台覆盖状态；日志不包含密钥或凭证信息。

> 进展备注（Phase 3-16）：`ProviderSecretPanel` 已增加仅开发环境可见的前端观测日志，联合记录 `policy` 与 native probe 结果（`backend/platform/available/code`），用于定位“策略与平台能力不匹配”的触发路径。

> 进展备注（Phase 3-17）：前端观测日志已加入轻量去重：同一 `app/provider/policy/backend/platform/available/code` 组合仅输出一次，减少开发态重复刷屏并保留关键路径可见性。

> 进展备注（Phase 3-18）：开发环境下新增“复制调试摘要”能力：`ProviderSecretPanel` 可一键复制 native probe 摘要（`app/provider/policy/backend/platform/available/code/reason`），便于快速回传联调信息且不涉及密钥数据。

> 进展备注（Phase 3-19）：复制的调试摘要已升级为标准 JSON（pretty print），新增 `schemaVersion`、`appVersion` 与 `generatedAt` 字段，便于直接粘贴到 issue/日志系统进行结构化分析。

> 进展备注（Phase 3-20）：调试摘要 JSON 已新增 `traceId` 字段（优先使用 `crypto.randomUUID()`），用于串联同一次前后端联调日志，提升问题定位效率。

> 进展备注（Phase 3-21）：`get_native_fido2_capability` 已支持接收可选 `traceId` 并写入后端诊断日志；前端 probe 调用同步透传同一 `traceId`，复制的调试摘要优先复用该值，实现前后端日志一键对齐。

> 进展备注（Phase 3-22）：复制的调试摘要 JSON 已补充 probe 时序字段（`probeRequestedAt` / `probeCompletedAt`），结合 `traceId` 可更直观地还原单次联调请求的时间线。

> 进展备注（Phase 3-23）：调试摘要 JSON 已新增 `probeDurationMs`（由 probe 请求/完成时间自动计算），用于快速横向比较不同平台或不同实现阶段的探测耗时。

> 进展备注（Phase 3-24）：probe 失败路径已纳入调试摘要：即使 native probe 调用报错，也可复制结构化 JSON（新增 `probeOutcome` / `probeError`），确保失败现场可直接回传并用于排障。

> 进展备注（Phase 3-25）：已修正 probe 失败后的状态一致性：失败时会清理旧的 native capability 快照，避免沿用过期成功结果；同时失败日志附带 `traceId`，便于与摘要 JSON 精确对齐。

> 进展备注（Phase 3-26）：调试摘要 JSON 已补充 `probeAttempt` 与 `probeErrorCode`（从错误信息中提取如 `FIDO2_*` 码），用于多次重试场景下按“尝试序号 + 错误码”进行快速聚合分析。

> 进展备注（Phase 3-27）：调试摘要 JSON 已新增 `probeSessionId`（面板打开期间固定、关闭后重置），用于将同一面板会话中的多次 probe 尝试聚合分析。

> 进展备注（Phase 3-28）：面板关闭时已重置 probe 调试状态（`probeSessionId/traceId/attempt/timeline/error/capability`），确保下次打开后的调试摘要不会混入上一会话残留数据。

> 进展备注（Phase 3-29）：调试摘要 JSON 已新增 `probeTrigger`（`auto` / `manual`），用于区分“自动首探测”与“手动刷新”来源，便于定位触发上下文。

> 进展备注（Phase 3-30）：调试摘要 JSON 已新增 `probeContext`（策略与挑战上下文），包含 `policy/isFido2Required/hasChallenge/challengeId/challengeExpired`，便于结合 `probeTrigger` 还原触发时的前端状态。


## Phase 2（切换读路径）

1. 通过 `resolve_provider_secret` 替换各业务链路直接读 key 的代码。
2. proxy / usage / stream check 全量走统一入口。
3. 加入内存缓存 TTL（例如 5 分钟）降低重复触发硬件验证。

验收：provider 切换、代理转发、用量查询均可正常工作。

## Phase 3（FIDO2）

1. 接入 FIDO2 用户验证流程（触碰/生物验证）。
2. `secretPolicy=fido2_required` 时强制硬件验证。
3. 无 FIDO2 能力时给出可解释降级（os_keychain）。

验收：开启 FIDO2 后未解锁无法发请求；解锁后链路正常。

## Phase 4（数据迁移）

1. 首次访问旧 provider 时执行“懒迁移”：明文 key -> SecretStore -> 删除明文。
2. 深链导入 `apiKey` 后立即入 SecretStore，禁止长驻 provider JSON。
3. 增加迁移统计与失败回退提示。

验收：历史数据逐步去明文，兼容旧版本数据。

---

## 6. 兼容策略

1. **旧数据兼容**：若 `secretRef` 不存在，按旧逻辑读取并触发迁移。
2. **无硬件兼容**：自动降级到 OS Keychain，不阻塞主流程。
3. **CLI 配置兼容**：继续支持写入现有 live config；后续可选“仅代理注入 token”增强模式。

---

## 7. 安全策略

1. 日志与崩溃信息中严禁输出密钥明文（延续 `redact_url_for_log` 思路）。
2. 解锁票据（UnlockTicket）仅保存在内存，不落盘。
3. 内存缓存设置 TTL + 主动清理（应用锁定/退出时销毁）。
4. deeplink 导入参数中的 `apiKey` 使用后立即覆盖/清理临时变量。

---

## 8. 风险与缓解

1. **FIDO2 平台差异**：先做 OS Keychain 基线，再逐平台引入 FIDO2。
2. **改造面广**：统一入口 + 编译器约束（禁止新代码直接读 `apiKey`）。
3. **用户体验抖动**：加 TTL 缓存、批量操作复用同一解锁会话。
4. **回归风险**：建立“provider switch / proxy / usage / deeplink”四条回归清单。

---

## 9. 测试计划

## 9.1 单元测试（Rust）

1. `SecretStore` put/get/rotate/delete。
2. `resolve_provider_secret` 在 plain/keychain/fido2 下行为。
3. 懒迁移逻辑（旧明文 -> 新 secretRef）。

## 9.2 集成测试

1. 切换 provider 成功并写入 live config。
2. usage 查询可读取解密 key。
3. proxy 请求注入 token 正常。
4. deeplink 导入后 DB 无明文 key。

## 9.3 手工验证

1. 首次绑定硬件钥匙。
2. 重启后触碰解锁。
3. 无钥匙环境降级提示与功能可用性。

---

## 10. 回滚方案

1. 保留 feature flag：`security.secretStore.enabled`。
2. 紧急回滚时退回旧读取路径（但不回写明文）。
3. 保留 `secretRef` 与旧字段兼容读取窗口，避免不可逆故障。

---

## 11. 建议的首批代码改造清单

1. 新增目录：`src-tauri/src/services/secret_store/*`
2. 修改模型：`src-tauri/src/provider.rs`、`src/types.ts`
3. 新增命令：`src-tauri/src/commands/provider.rs` + `src-tauri/src/lib.rs` 注册
4. 改读路径：
   - `src-tauri/src/services/provider/mod.rs`
   - `src-tauri/src/services/provider/usage.rs`
   - `src-tauri/src/services/proxy.rs`
   - `src-tauri/src/services/stream_check.rs`
5. 深链链路：`src-tauri/src/deeplink/provider.rs`

---

## 12. 里程碑与验收标准

### M1（1~2 周）

- SecretStore 基线（OS Keychain）完成
- 新建 provider 不再明文落库

### M2（1 周）

- 四条核心链路统一走 `resolve_provider_secret`
- 回归测试通过

### M3（1~2 周）

- FIDO2 解锁上线（含降级）
- 文档与用户提示完善

---

## 13. 一句话决策建议

优先采用“**SecretStore 抽象 + 统一取密钥入口 + 分阶段上 FIDO2**”路线：
先确保安全与兼容，再逐步提高硬件绑定强度，风险最可控、可持续演进。
