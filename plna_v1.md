# AISDK Migration Plan v1

Status: Draft v1 (planning mode only, no implementation in this step)

## 1) Objective

Convert Oxydra into an embeddable AISDK that wraps the native Rust agent runtime and accepts per-run execution policy from SDK callers.

Primary policy controls:

- Max runtime (wall-clock limit for the full run)
- Max budget (cost ceiling for the full run)
- Tool policy (toolset exposure + allow/deny/approval behavior)

## 2) Why this plan

This v1 plan is based on:

- Oxydra current architecture and guidebook layering (`types -> gateway -> runtime -> tools -> provider`)
- Nanoclaw cue: execution config is resolved at admission and carried into run orchestration
- Claude Agent SDK cue: explicit SDK options for `max_budget_usd`, `tools`, `allowed_tools`, `disallowed_tools`, and callback/hook-based tool decisions

## 3) Current State (Oxydra)

Observed strengths already present:

- Runtime guardrails: turn timeout, max turns, optional max cost
- Tool execution boundaries and security policy support
- Gateway session lifecycle and cancellation controls
- Delegation/subagent runtime support

Observed gaps for SDK-first policy:

- No single per-run policy object flowing end-to-end from request boundary to runtime/tool dispatch
- No first-class wall-clock max runtime for full run lifecycle
- Tool policy is not modeled as SDK-level layered policy (toolset vs approval vs deny)
- Delegation currently notes allowlist not enforced for subagents

## 4) External Cues and What Changes

### 4.1 Nanoclaw cues adopted

- Resolve execution config at run admission (once), then propagate through runtime lifecycle
- Keep policy immutable during a run

### 4.2 Claude Agent SDK cues adopted

- Separate tool controls into layered semantics:
  - `tools` (base toolset)
  - `allowed_tools` (auto-approve behavior)
  - `disallowed_tools` (hard block)
- Preserve budget option ergonomics (`max_budget` as top-level run setting)
- Add optional callback/hook-style decision points for tool permission handling

### 4.3 Claude Agent SDK cues NOT adopted directly

- Do not rely on post-hoc runtime limits only
- Keep first-class `max_runtime` in Oxydra (Claude SDK does not provide this as a native run option)
- Keep hard runtime enforcement in Rust core, not only client-side wrappers

## 5) Target SDK Contract (v1)

```rust
pub struct RunRequest {
    pub session_id: Option<String>,
    pub prompt: String,
    pub policy: Option<RunPolicyInput>,
}

pub struct RunPolicyInput {
    pub max_runtime: Option<std::time::Duration>,
    pub max_budget_microusd: Option<u64>,
    pub tool_policy: Option<ToolPolicyInput>,
}

pub struct ToolPolicyInput {
    pub toolset: Option<Vec<ToolId>>,          // tools exposed to model
    pub auto_approve_tools: Option<Vec<ToolId>>, // permission UX policy
    pub disallowed_tools: Option<Vec<ToolId>>, // hard deny list
}

pub struct EffectiveRunPolicy {
    pub started_at: MonotonicInstant,
    pub deadline: Option<MonotonicInstant>,
    pub initial_budget_microusd: Option<u64>,
    pub remaining_budget_microusd: Option<u64>,
    pub toolset: std::collections::BTreeSet<ToolId>,
    pub auto_approve_tools: std::collections::BTreeSet<ToolId>,
    pub disallowed_tools: std::collections::BTreeSet<ToolId>,
    pub parent_run_id: Option<String>,
}

pub enum StopReason {
    Completed,
    Cancelled,
    MaxTurns,
    MaxRuntimeExceeded,
    MaxBudgetExceeded,
    ToolDisallowed { tool: ToolId },
    ToolPermissionDenied { tool: ToolId, reason: String },
    ProviderTimedOut,
}
```

Notes:

- `ToolId` must be canonicalized once at registry boundary to avoid alias bypasses
- Public SDK shape stays simple; internal `EffectiveRunPolicy` carries resolved values and clocks

## 6) Policy Invariants (must hold)

- Policy is resolved once at admission and persisted with run/session state
- `deadline` is immutable and cannot be widened after run start
- `remaining_budget_microusd` is monotonic non-increasing and never negative
- `disallowed_tools` always wins over any other tool setting
- Delegated/scheduled child runs can only narrow parent policy (time, budget, toolset)
- Resume path must rehydrate stored effective policy, never recompute from mutable global config

## 7) Enforcement Matrix (where checks must happen)

1. Admission boundary (SDK/gateway/scheduler)
   - Resolve `RunPolicyInput` -> `EffectiveRunPolicy`
   - Validate invariants and persist snapshot

2. Runtime loop
   - Check runtime/budget before each turn
   - Check before provider call and after provider usage settlement

3. Provider streaming/completion
   - Mid-stream cancellation/deadline checks
   - Post-call budget settlement checks

4. Tool schema exposure
   - Advertise only `toolset - disallowed_tools`

5. Tool dispatch
   - Hard deny disallowed tools even if model/tool-call payload is forged
   - Optional permission callback for auto-approve vs ask/deny behavior

6. Delegation/subagents
   - Parent-to-child policy narrowing only
   - Child must inherit remaining budget/time ceilings

7. Scheduler execution
   - Scheduled run admission must resolve and persist effective policy

8. Resume path
   - Rehydrate stored effective policy and prior ledger/deadline state

## 8) File/Layer Impact Plan (Oxydra)

Planned layers and likely touchpoints:

- `crates/types`
  - Add SDK-facing policy structs and stop reason surface updates
  - Extend gateway request frame(s) for optional policy input

- `crates/gateway`
  - Admission-time policy resolution
  - Persist policy snapshot in session/run state
  - Thread policy into turn runner

- `crates/runtime`
  - Add unified `PolicyGuard`
  - Runtime deadline and budget enforcement at all required seams
  - Integrate policy with provider and tool execution paths

- `crates/tools`
  - Add toolset filtering for advertised schemas
  - Canonicalized tool ID resolution for enforcement

- `crates/runtime/delegation`
  - Enforce inherited/narrowed tool policy and budget/runtime ceilings for subagents

## 9) Phased Delivery Plan

### Phase 0 - Spec and compatibility contract

Deliverables:

- RFC-like design doc finalized
- Backward compatibility matrix (`policy: None` behavior)
- StopReason taxonomy

Gate:

- Team sign-off on API shape + invariants

### Phase 1 - Types and protocol plumbing

Deliverables:

- `RunPolicyInput`, `ToolPolicyInput`, `EffectiveRunPolicy` types
- Gateway frame extension for optional policy

Gate:

- Serialization compatibility tests pass
- Existing clients function unchanged when policy is absent

### Phase 2 - Admission resolution and persistence

Deliverables:

- Policy resolution at run admission
- Persisted effective policy snapshot

Gate:

- No run starts without resolved policy snapshot
- Resume restores exact same effective policy state

### Phase 3 - Runtime and provider enforcement

Deliverables:

- Deadline checks before and during execution
- Budget ledger settlement around provider calls

Gate:

- Deterministic stop reasons for runtime and budget exceed cases
- Streaming timeout/deadline tests pass

### Phase 4 - Tool policy enforcement

Deliverables:

- Schema filtering based on toolset
- Dispatch-time hard deny for disallowed tools
- Optional permission callback/hook path

Gate:

- Forged tool call negative tests pass
- Disallow precedence tests pass

### Phase 5 - Delegation and scheduler inheritance

Deliverables:

- Parent->child narrow-only policy propagation
- Scheduler runs use same admission + guard path

Gate:

- Concurrent subagent budget race tests pass
- Child cannot widen any parent limit

### Phase 6 - Rollout and observability

Deliverables:

- Observe-only metrics mode
- Soft-fail diagnostics mode
- Hard enforcement rollout flags

Gate:

- Canary metrics stable
- No policy-missing or bypass events in target window

## 10) Test Plan (must-have)

- Compatibility tests: old payloads without `policy` remain valid
- Runtime tests: max runtime deterministic termination
- Budget tests: exact or bounded-overrun semantics explicitly tested
- Tool tests: toolset exposure + hard deny dispatch
- Delegation tests: child narrowing and aggregate budget correctness
- Resume tests: persisted effective policy replay
- Gateway tests: admission validation and policy propagation

## 11) Risk Register

1. Tool ID mismatch bypass
   - Mitigation: canonical `ToolId` map and single normalization path

2. Budget races in parallel delegation
   - Mitigation: shared atomic budget ledger with reservation/settlement model

3. Runtime checks only at turn boundaries
   - Mitigation: mid-stream and tool-exec checks plus cancellation token integration

4. Resume widening policy by accident
   - Mitigation: persisted snapshot is source of truth

5. Ambiguous default semantics when policy omitted
   - Mitigation: explicit fallback rules and telemetry for implicit/default paths

## 12) Open Decisions (to finalize before implementation)

- Budget strictness mode:
  - Strict hard-stop before exceeding, or
  - Claude-style bounded overrun (up to one provider call)

- Permission callback API surface:
  - Runtime callback trait only, or
  - Runtime callback + hook event bus for richer policy workflows

- StopReason external contract stability:
  - Freeze enum in v1, or
  - Mark as forward-extensible with unknown passthrough handling

## 13) Definition of Done for v1

- SDK caller can submit a run with `max_runtime`, `max_budget_microusd`, and tool policy
- Effective policy is resolved once, persisted, enforced everywhere, and resumed correctly
- Delegation and scheduler cannot bypass or widen policy
- Backward compatibility retained for existing callers without explicit policy
- Test matrix and observability gates are green
