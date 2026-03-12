# Draft: AISDK Migration V3

## Source Plans
- V1: `plna_v1.md` — strong on policy semantics, enforcement, types, risks
- V2: `plan_v2.md` — strong on SDK surface, crate architecture, repo strategy

## Open Decisions (from V1 Section 12) — RESOLVED

### 1. Budget strictness mode
**Decision: Bounded overrun (Claude-style)**
- Rationale: Hard-stop mid-provider-call is fragile — you may get partial responses, broken streaming state, or lost context. Allow one provider call to complete after budget threshold, then stop before the next turn. This matches real-world SDK ergonomics (Claude Agent SDK does this) and avoids the complexity of mid-stream abort for budget reasons.
- Implementation: Check budget BEFORE each provider call. If remaining < 0 after settlement, set StopReason::MaxBudgetExceeded before next turn.

### 2. Permission callback API surface
**Decision: Runtime callback trait only (v1), hook event bus deferred to v1.1**
- Rationale: A callback trait is simpler, testable, and sufficient for the initial SDK. An event bus adds complexity (ordering, async dispatch, error handling) that isn't justified until there's a real use case. Keep the trait extensible so event bus can be layered on later without breaking changes.
- Implementation: `trait ToolPermissionHandler { async fn check(&self, tool_id: &ToolId, context: &RunContext) -> ToolPermissionDecision; }`

### 3. StopReason external contract stability
**Decision: Forward-extensible with `#[non_exhaustive]`**
- Rationale: Freezing the enum in v1 is premature — we'll discover new stop reasons as the SDK matures. Rust's `#[non_exhaustive]` is designed for exactly this: callers must handle the wildcard arm, so adding variants is non-breaking.

## V3 Scope Decisions

### IN SCOPE (V3 plan covers)
- New `crates/sdk` facade crate (from V2)
- Policy types in `crates/types` (from V1)
- SDK surface: one-shot + streaming + control plane (from V2)
- Policy enforcement pipeline: admission → runtime → tools → delegation (from V1)
- Session accounting / budget ledger (from V1)
- Delegation policy inheritance (from V1+V2)
- Backward compatibility guarantee (from V1)
- Repository strategy + PR breakdown (from V2)
- Rollout/observability phase (from V1)
- Risk mitigations (from V1)
- Test plan (combined V1+V2)

### DEFERRED (explicitly out of V3)
- Hook event bus (v1.1 — only callback trait in v1)
- MCP tool server enable/disable at runtime (v1.1 — static config is fine for v1)
- Model hot-swap during session (v1.1)
- In-process extension/tool registration (v1.1)
- Web configurator SDK config UI (v1.1)

### ADDED (neither V1 nor V2 had)
- Streaming event model (what the SDK emits during a run)
- Config system integration (how SDK policy merges with existing figment config)
- Error handling contract (failure modes vs policy-triggered stops)

## Research Findings (from 4 explore agents)

### Runtime Enforcement (what already exists)
- Turn loop: `crates/runtime/src/lib.rs:363-620` — `run_session_internal()` with core `loop {}`
- `RuntimeLimits` struct (lib.rs:100-108): `turn_timeout`, `max_turns`, `max_cost`
- Budget: `budget.rs:579-596` — `enforce_cost_budget()` accumulates per-turn, returns `RuntimeError::BudgetExceeded`
- Turn timeout: enforced via `tokio::time::timeout()` on provider calls in `provider_response.rs`
- Cancellation: `tokio_util::sync::CancellationToken` used throughout (turn loop, provider, tools, scheduler)
- `TurnState` enum: Streaming, ToolExecution, Yielding, Cancelled
- **GAP**: No wall-clock session-level deadline (only per-turn timeout)
- **GAP**: No centralized policy evaluation point — checks scattered across files
- **GAP**: No `StopReason` enum (only `RuntimeError::BudgetExceeded` and `RuntimeError::Cancelled`)

### Crate Dependency Graph
```
types (foundation, no internal deps)
  ↑
  ├─ provider → types
  ├─ memory → types
  ├─ tools → types, memory
  ├─ channels → types
  ↑
  ├─ runtime → types, tools, memory
  ↑
  ├─ gateway → types, runtime, tools
  ↑
  └─ runner → types, provider, runtime, memory, tools, gateway, channels
```
SDK crate should sit at same level as runner: `sdk → types, runtime, tools, gateway`

### Gateway Admission & Streaming
- Entry: `GatewayServer::create_or_get_session()` → `submit_turn()` → `GatewayTurnRunner::run_turn()` → `AgentRuntime`
- Streaming: `StreamItem` (Text, Progress, Media) via mpsc channel → `GatewayServerFrame` (TurnStarted, AssistantDelta, TurnProgress, TurnCompleted, etc.) via broadcast
- Session: `SessionState` with broadcast channel, `SessionStore` trait for persistence
- Config: figment layered config (defaults → toml files → env vars → CLI)

### Tool System
- `ToolRegistry`: `BTreeMap<String, Box<dyn Tool>>` in `crates/tools/src/registry.rs`
- Schema exposure: `ToolRegistry::schemas()` — NO filtering, all tools advertised to LLM
- Dispatch: `execute_tool_call()` (runtime) → `execute_with_policy_and_context()` (registry)
- Tool IDs: plain strings with constants (FILE_READ_TOOL_NAME, SHELL_EXEC_TOOL_NAME, etc.)
- `SafetyTier` enum: ReadOnly, SideEffecting, Privileged
- `SecurityPolicy` trait: path-based/sandbox enforcement in `tools/src/sandbox/policy.rs`
- `AgentDefinition.tools: Option<Vec<String>>` — EXISTS BUT NOT ENFORCED
- **No MCP integration in current codebase**

### Delegation & Scheduler
- `DelegationRequest`: parent_session_id, goal, max_turns, max_cost — NO tool policy inheritance
- Comment at `delegation.rs:22-23`: "tool allowlists are not enforced (all tools available to subagents)"
- Scheduler: separate `ScheduledTurnRunner` trait, global SchedulerConfig, no per-schedule policy
- **GAP**: No budget cascading — subagent budgets don't sum-check against parent remaining
- **GAP**: No DelegationPolicy type — parameters scattered

### Test Infrastructure
- Standard `cargo test` + tokio async tests
- Patterns: FakeProvider (scripted steps), Mockall mocks, temp_workspace_root helper
- Test files: `crates/runtime/src/tests.rs`, `crates/tools/src/tests.rs`, `crates/gateway/src/tests.rs`

## Key SDK Design Decisions (grounded in codebase)

1. **Policy insertion**: New `RunPolicy` resolves at admission (SDK/gateway), carried as `EffectiveRunPolicy` through runtime
2. **Tool filtering at two layers**: schema exposure (before LLM sees tools) + dispatch blocking (defense-in-depth)
3. **Budget model**: Bounded overrun — check before provider call, settle after, stop before next turn if exceeded
4. **Delegation narrowing**: New `EffectiveRunPolicy` field on delegation path, child policy = min(parent_remaining, child_requested)
5. **Streaming events**: Extend existing `StreamItem`/`GatewayServerFrame` with policy events (budget warning, policy stop)
6. **Config merge**: SDK per-run policy is strictest-wins merge with global AgentConfig/RuntimeConfig
