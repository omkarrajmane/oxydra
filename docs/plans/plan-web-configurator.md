# Web Configurator Plan (Per-User Embedded Instance)

## Status

- **State:** Draft (supersedes the current plan in issue #7)
- **Scope:** `types` + `runner` crates, plus documentation updates
- **Primary dependency:** Issue #17 logging infrastructure is already in place (`collect_logs_snapshot`, `RunnerLogEntry`, control-socket log operations)

## Locked Decisions

These decisions are fixed for this plan:

1. **Per-user web instance** (no multi-user dashboard in a single daemon instance).
2. **Editable source files with precedence awareness** in the UI.
3. **Effective value + provenance visibility** (show current effective value and why it won).
4. **Strong non-destructive write guarantees** (backup + atomic write + rollback for multi-file edits).
5. **Security hardening for local HTTP control surface**.
6. **Strict API behavior contracts** (PATCH semantics, restart/reload semantics, error model).
7. **Comprehensive verification gates** (unit + integration + CLI compatibility + security tests).
8. **Any new crates must use latest stable versions at implementation time** (do not follow pinned versions from issue #7).

## Goals

1. Give operators a local web dashboard for status, config, control, and logs.
2. Keep config files as the source of truth (no DB, no shadow config store).
3. Preserve existing runner/control-socket behavior and CLI compatibility.
4. Keep footprint minimal (embedded UI, no Node/npm toolchain).
5. Maintain Oxydra security posture and strict crate boundaries.

## Non-Goals (V1)

1. A multi-user control plane across multiple active daemons.
2. Full runtime hot-reload of all config without restart.
3. External observability stacks (OTel/Loki/etc.) beyond existing logs surface.
4. A complex frontend build pipeline.

## Architecture Overview

### Runtime Topology

The web configurator runs inside the same `runner` daemon process for one active user.

```text
runner start --user <id>
  ├─ launches runtime guests (existing)
  ├─ serves Unix control socket (existing)
  └─ serves local HTTP web configurator (new, same process)
```

### Why Per-User Instance

The current daemon lifecycle is already user-scoped. Keeping web scope per-user avoids introducing a cross-user supervisor refactor and prevents surprising control semantics.

### Planned Module Layout

```text
crates/runner/src/
  web/
    mod.rs                # router + startup wiring
    state.rs              # shared web state
    auth.rs               # bearer auth + origin checks
    response.rs           # common API envelope + error mapping
    status.rs             # health + metadata endpoints
    config_read.rs        # source/effective/provenance readers
    config_write.rs       # dry-run + safe write mutation endpoints
    control.rs            # stop/restart/reload endpoints
    logs.rs               # snapshot + SSE tail endpoints
    operations.rs         # async operation status (restart/reload jobs)
    masking.rs            # secret masking helpers for API responses
    provenance.rs         # precedence + winning-layer explanation engine
  web_static/
    index.html            # embedded single-file SPA (inline CSS + JS)
```

## Configuration Model

### Editable Source Files

The UI edits only concrete source files:

1. `runner.toml` (global runner config)
2. Active user's `<user>.toml` (runner user config)
3. Workspace `.oxydra/agent.toml` (workspace layer for agent config)

### Effective Value and Why It Won

The UI must show:

1. **Source value** (what the file contains)
2. **Effective value** (what runtime resolves to)
3. **Winning layer** (default/system/user/workspace/env/CLI)
4. **Reason/details** (for env/CLI, exact key/flag that overrode file)

### Precedence Rules to Surface

For agent config, use current runtime precedence:

```text
defaults < system file < user file < workspace file < env vars < CLI overrides
```

For runner/user config, effective value is primarily file-backed with defaults unless explicit runtime overrides are introduced.

### Provenance API Shape

Each leaf field should include machine-readable provenance:

```json
{
  "path": "memory.enabled",
  "effective": true,
  "source_value": false,
  "winning_layer": "env",
  "winning_source": "OXYDRA__MEMORY__ENABLED",
  "layers": [
    { "layer": "default", "value": false },
    { "layer": "workspace", "value": false, "source": ".oxydra/agent.toml" },
    { "layer": "env", "value": true, "source": "OXYDRA__MEMORY__ENABLED" }
  ]
}
```

## API Contract (V1)

All routes are scoped to the active user instance, prefixed with `/api/v1`.

### Read Endpoints

1. `GET /api/v1/meta` returns version, active user id, and enabled features.
2. `GET /api/v1/status` returns runner/guest health and degraded reasons.
3. `GET /api/v1/config/runner/source` returns parsed `runner.toml`.
4. `GET /api/v1/config/user/source` returns parsed active `<user>.toml`.
5. `GET /api/v1/config/agent/source` returns parsed workspace `.oxydra/agent.toml`.
6. `GET /api/v1/config/agent/effective` returns merged effective config.
7. `GET /api/v1/config/agent/provenance` returns field-level precedence metadata.
8. `GET /api/v1/logs/snapshot` returns bounded logs using existing log request model.
9. `GET /api/v1/logs/stream` streams log tail over SSE by polling bounded snapshots.

### Mutation Endpoints

1. `PATCH /api/v1/config/runner/source` applies **RFC 7396 JSON Merge Patch**.
2. `PATCH /api/v1/config/user/source` applies RFC 7396 JSON Merge Patch.
3. `PATCH /api/v1/config/agent/source` applies RFC 7396 JSON Merge Patch to workspace source file.
4. `POST /api/v1/config/*/validate` validates a proposed patch without writing.
5. `POST /api/v1/control/stop` gracefully stops active runtime (daemon exits after response flush).
6. `POST /api/v1/control/restart` schedules in-process restart operation and returns operation id.
7. `POST /api/v1/control/reload` attempts no-restart reload where supported; returns explicit unsupported fields when not possible.

`POST /api/v1/control/start` is intentionally omitted for V1 because this web instance exists only while the per-user daemon is already running.

### Operation Tracking

Restart/reload are asynchronous from HTTP caller perspective.

1. Mutating control endpoints return `202 Accepted` with `operation_id`.
2. `GET /api/v1/operations/{operation_id}` returns pending/success/failed with message and timestamps.

### Error Model

Standard envelope with stable codes:

```json
{
  "error": {
    "code": "config_validation_failed",
    "message": "memory.retrieval.vector_weight + fts_weight must equal 1.0",
    "details": { "path": "memory.retrieval" }
  }
}
```

## Write Safety and Consistency

### Single-File Write Contract

For every mutating config write:

1. Parse + apply patch in memory.
2. Validate typed struct (`validate()` and any cross-field checks).
3. Acquire exclusive file lock.
4. Create timestamped backup (`.bak.<ts>.<pid>`).
5. Write to sibling temp file.
6. `fsync` temp file (and parent directory where supported).
7. Atomic rename temp file over original.
8. Release lock and return write metadata (backup path, changed fields).

### Multi-File Transaction Contract

For endpoints that require multi-file consistency (future user create/delete workflows):

1. Stage all new contents in memory first.
2. Back up all affected files before first write.
3. Apply writes in deterministic order with atomic rename per file.
4. If any write fails, restore all already-written files from backups.
5. Return explicit rollback status in response.

### Backup Retention Policy

1. Keep last `N` backups per file (configurable, default 20).
2. Prune oldest backups after successful writes.
3. Never prune backups created during failed write attempts in same request.

## Security Plan

### New Runner Global Web Config

Add `[web]` to `RunnerGlobalConfig`:

```toml
[web]
enabled = true
bind = "127.0.0.1:9400"
auth_mode = "disabled"                # disabled | bearer_token
auth_token_env = "OXYDRA_WEB_TOKEN"   # preferred
auth_token = ""                        # optional fallback (discouraged)
trusted_origins = ["http://127.0.0.1:9400", "http://localhost:9400"]
max_requests_per_minute = 120
```

### Security Controls

1. Default bind is loopback only.
2. Bearer token auth for API routes when enabled.
3. Constant-time token comparison.
4. Strict `Origin` + `Host` validation for mutating endpoints.
5. CORS disabled by default; no wildcard origins.
6. Content-type enforcement (`application/json`) on mutating routes.
7. Rate limiting for API routes.
8. Sensitive field masking in all responses.
9. Audit log entries for each mutating request (who/when/what file/result).

### Secret Masking Rules

Mask these fields in API responses:

1. `providers.registry.*.api_key`
2. `memory.auth_token`
3. `web.auth_token`

Patch behavior for masked fields:

1. A sentinel value (`"__UNCHANGED__"`) means keep current secret.
2. Empty string clears value if allowed by schema.
3. Any non-sentinel value sets a new value.

## Frontend Plan (Single Embedded Page)

### UI Sections

1. Dashboard (`status`, degraded reasons, quick actions)
2. Agent config editor (workspace source + effective/provenance panel)
3. Runner config editor
4. Active user config editor
5. Control panel (restart/stop/reload)
6. Logs viewer (snapshot + live SSE tail)

### Precedence-Aware UX

Each editable field shows:

1. Current source value
2. Current effective value
3. Badge for winning layer (e.g., `env`, `workspace`, `default`)
4. Inline explanation (e.g., "Overridden by OXYDRA__MEMORY__ENABLED")

### Edit Flow

1. User edits source fields.
2. UI computes and shows diff preview.
3. UI runs server-side validation endpoint.
4. On apply, UI sends patch with optimistic `etag`.
5. UI shows restart-required hints based on server response.

## Dependency Policy (Latest Versions)

### Rule

Any new dependency introduced for this work must use the latest stable release available at implementation time.

### Practical Enforcement

1. Add dependencies with `cargo add <crate>` (no stale manual pin from issue text).
2. Run `cargo update` after additions.
3. Validate with `cargo tree -d` and existing `cargo deny` policy.
4. Mention selected versions in PR notes.

### Candidate Dependencies (only if needed)

1. `schemars` for schema endpoints.
2. `fd-lock` (or equivalent) for cross-process file locking.
3. `json-merge-patch` (or equivalent) for RFC 7396 patch behavior.
4. `tower-http` middleware extras only if not already covered.

If a dependency can be avoided with existing crates, prefer avoiding it.

## Implementation Phases

### Phase 0: Foundation Decisions and Contracts

1. Add this plan and align issue #7 scope to it.
2. Finalize endpoint contracts and error code taxonomy.
3. Finalize restart/reload semantics for per-user daemon instance.

**Gate:** Contracts reviewed and frozen before coding.

### Phase 1: Web Runtime Bootstrap (Read-Only)

1. Add `WebConfig` to runner config types.
2. Wire web server startup into daemon lifecycle.
3. Add health/meta/status endpoints.
4. Add embedded static SPA shell page.

**Gate:** `runner start` exposes local web UI and status endpoint without changing existing control-socket behavior.

### Phase 2: Config Read + Provenance Engine

1. Implement source readers for runner/user/agent workspace files.
2. Implement effective agent config reader using existing layered loader.
3. Implement field-level provenance engine and API.
4. Add UI rendering for provenance and effective values.

**Gate:** UI can explain effective value origin for every displayed agent config leaf.

### Phase 3: Safe Config Mutation

1. Implement atomic write helper and backup policy.
2. Implement patch validation endpoints.
3. Implement patch apply endpoints with optimistic concurrency (`etag`/`if-match`).
4. Emit structured restart-required metadata in mutation responses.

**Gate:** Mutation path is non-destructive, rollback-safe, and fully validated.

### Phase 4: Control + Logs

1. Implement stop/restart/reload endpoints with operation tracking.
2. Reuse `collect_logs_snapshot` for logs endpoint.
3. Implement SSE live tail by bounded polling (no custom streaming protocol rewrite).
4. Build UI control and logs pages.

**Gate:** UI can perform lifecycle actions and live-log viewing reliably.

### Phase 5: Security Hardening

1. Add auth middleware and token resolution rules.
2. Add origin/host checks for mutating endpoints.
3. Add rate limiting and audit logging.
4. Add secrets masking and sentinel update behavior.

**Gate:** Unauthorized/cross-origin mutation attempts are blocked with explicit errors.

### Phase 6: Quality, Docs, and Release Readiness

1. Complete unit/integration/security test matrix.
2. Update guidebook docs (config + runner lifecycle chapters).
3. Add operator docs for web auth/reverse proxy guidance.
4. Final pass for clippy/tests/deny checks in touched crates.

**Gate:** All tests pass and docs reflect final behavior.

## Verification Matrix

### Unit Tests

1. Provenance merge correctness.
2. RFC 7396 patch behavior.
3. Secret masking and sentinel rules.
4. Atomic write helper edge cases (permission error, disk-full simulation).
5. Origin and auth middleware behavior.

### Integration Tests (`runner`)

1. Web server boots with daemon and serves status.
2. Source/effective/provenance responses are consistent.
3. Valid patch writes config and creates backup.
4. Invalid patch leaves files unchanged.
5. Restart operation transitions through operation states and restores healthy runtime.
6. Logs snapshot and SSE tail respect bounds and filters.

### CLI Compatibility Tests

1. Existing `runner status/stop/logs/restart` behavior remains valid.
2. Control socket protocol remains backward compatible.

### Security Tests

1. Unauthorized request rejection.
2. Origin mismatch rejection on mutating routes.
3. Rate limiter activation.
4. Masked secret fields never leak raw values.

## Risks and Mitigations

1. **Risk:** Provenance engine complexity for deeply nested configs.
   **Mitigation:** Build JSON-path walker with deterministic flattening and exhaustive tests.
2. **Risk:** Restart sequencing races (web request in-flight while restarting).
   **Mitigation:** Use operation queue + explicit state machine (`Running`, `Restarting`, `Stopped`).
3. **Risk:** Partial writes on failures.
   **Mitigation:** Mandatory temp-write + rename + rollback helpers.
4. **Risk:** Localhost CSRF-style misuse.
   **Mitigation:** Enforce origin checks + JSON content-type + optional bearer auth.

## Acceptance Checklist

- [ ] Per-user embedded web configurator runs inside runner daemon.
- [ ] UI edits only source files and shows effective value + winning layer explanation.
- [ ] All config writes are validated, backed up, and atomic.
- [ ] PATCH semantics are RFC 7396 and documented.
- [ ] Stop/restart/reload semantics are explicit and tested.
- [ ] Log snapshot + live stream reuse existing runner log infrastructure.
- [ ] Security controls (auth/origin/rate-limit/masking/audit) are implemented.
- [ ] Existing CLI and control-socket workflows remain compatible.
- [ ] Any newly added crates use latest stable versions at implementation time.
- [ ] Guidebook docs are updated to reflect final web configurator behavior.
