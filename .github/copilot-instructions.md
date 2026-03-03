# Project Guidelines

## Code Style
- TypeScript is strict; keep code compatible with `strict`, `noUnusedLocals`, and `noUnusedParameters` in `tsconfig.json`.
- Use the `@/*` path alias for imports from `src` (see `tsconfig.json` and `vitest.config.ts`).
- Prefer API wrappers in `src/lib/api/*` + hooks in `src/lib/query/*` over ad-hoc backend calls in UI components.
- Keep existing formatting and naming style (double quotes in TS files; Rust modules with command/service separation).
- UI styling uses Tailwind tokens and theme variables from `tailwind.config.cjs`; avoid introducing hard-coded new design tokens.

## Architecture
- Frontend entry is `src/main.tsx` (providers: React Query, theme, i18n, Tauri event bootstrap) and app composition lives in `src/App.tsx`.
- Frontend-to-backend bridge: Tauri `invoke` wrappers in `src/lib/api/*.ts` (example: `src/lib/api/providers.ts`).
- Query/state orchestration uses TanStack Query hooks in `src/lib/query/queries.ts` and `src/lib/query/mutations.ts`.
- Rust backend is modular: commands in `src-tauri/src/commands/*`, business logic in `src-tauri/src/services/*`, persistence in `src-tauri/src/database/*`.
- Tauri command registration is centralized in `src-tauri/src/lib.rs` via `tauri::generate_handler![...]`; add new commands there.

## Build and Test
- Install deps: `pnpm install`
- Run desktop app (Tauri + frontend): `pnpm dev`
- Run frontend only: `pnpm dev:renderer`
- Type-check: `pnpm typecheck`
- Unit tests: `pnpm test:unit`
- Frontend build: `pnpm build:renderer`
- App build: `pnpm build`
- Rust tests (when touching backend): `cargo test --manifest-path src-tauri/Cargo.toml`

## Project Conventions
- Maintain app routing by `AppId`/`AppType` strings (frontend passes `app`, backend parses via `AppType::from_str`, e.g. `src-tauri/src/commands/provider.rs`).
- For Tauri command args exposed to frontend camelCase, backend keeps compatibility with `#[allow(non_snake_case)]` on fields like `providerId`.
- Provider list behavior is intentional: sort by `sortIndex`, then `createdAt`, then localized name compare (`src/lib/query/queries.ts`).
- Keep event-driven flows aligned: frontend listens to backend events like `provider-switched` and `configLoadError` (`src/lib/api/providers.ts`, `src/main.tsx`).
- Do not bypass service/database layers in Rust; follow `commands -> services -> database` pattern.

## Integration Points
- Tauri APIs: `@tauri-apps/api/core` (`invoke`) and `@tauri-apps/api/event` (`listen`) are core integration paths.
- External integrations include deep links (`src-tauri/src/deeplink/*`), proxy/failover (`src-tauri/src/proxy/*`), WebDAV sync (`src-tauri/src/services/webdav*`), and session providers (`src-tauri/src/session_manager/*`).
- React Query cache keys and API modules should stay synchronized when adding new resources (`src/lib/query/queries.ts`, `src/lib/api/index.ts`).
- Tests rely on Vitest + jsdom + MSW + Tauri mocks (`vitest.config.ts`, `tests/setupTests.ts`, `tests/msw/tauriMocks`).

## Security
- Never log secrets or full credential-bearing URLs; follow redaction patterns like `redact_url_for_log` in `src-tauri/src/lib.rs`.
- Preserve crash/error reporting behavior (`src-tauri/src/panic_hook.rs`) and avoid exposing sensitive data in dialogs or logs.
- Treat local config/database paths under `~/.cc-switch` as sensitive user data; avoid destructive migrations or direct manual file rewrites.
- For provider switching/config writes, prefer existing atomic/managed flows in services/commands rather than custom file IO paths.