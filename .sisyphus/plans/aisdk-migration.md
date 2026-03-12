# Oxydra AI SDK Migration Plan (V3)

## TL;DR

> **Quick Summary**: Add a `crates/sdk` facade crate that wraps Oxydra's existing runtime, gateway, and tools crates, introducing a per-run `RunPolicy` with budget, timeout, and tool allow/deny controls. Enforcement at 8 choke points. Delegation inherits and narrows parent policy. Backward compatible — `policy: None` preserves existing behavior.
>
> **Deliverables**:
> - `crates/sdk` — SDK facade with one-shot + streaming + control plane APIs
> - `RunPolicyInput` / `EffectiveRunPolicy` / `ToolPolicyInput` types in `crates/types`
> - `StopReason` enum (#[non_exhaustive]) for all termination modes
> - Policy enforcement pipeline: admission → runtime → provider → tools → delegation → scheduler
> - Session budget ledger with atomic accounting
> - Delegation/scheduler policy inheritance (narrowing only)
> - Streaming policy events (BudgetWarning, PolicyStop)
> - Rollout flags (observe-only → soft-fail → hard-enforce)
>
> **Estimated Effort**: Large
> **Parallel Execution**: YES — 5 waves + final verification
> **Critical Path**: Task 1 → Task 8 → Task 10 → Task 14 → Task 18 → Task 22 → F1-F4

---

## Context

### Original Request
Combine two existing AISDK migration plans (V1: strong on policy semantics, enforcement types, risk management; V2: strong on SDK surface design, crate architecture, repo strategy) into a comprehensive V3 plan.

### Source Plans
- **V1** (`plna_v1.md`): Concrete Rust types (RunPolicyInput, EffectiveRunPolicy, ToolPolicyInput, StopReason), enforcement matrix (8 points), policy invariants (6 rules), risk register (5 risks with mitigations), open decisions (3 — now resolved), rollout strategy, backward compat guarantee
- **V2** (`plan_v2.md`): `crates/sdk` facade crate, one-shot + streaming + control plane SDK surface, repository strategy, PR breakdown, external pattern cues, "additive, no rewrite" principle

### Open Decisions (from V1) — Resolved
1. **Budget strictness**: Bounded overrun (Claude-style) — check before provider call, settle after, stop before next turn if exceeded. Avoids mid-stream abort complexity.
2. **Permission callback API**: Runtime callback trait only (`ToolPermissionHandler`). Hook event bus deferred to v1.1.
3. **StopReason stability**: `#[non_exhaustive]` for forward extensibility. Callers must handle wildcard arm.

### Codebase Research Findings (4 exploration agents)

**What already exists (reusable)**:
- `RuntimeLimits` struct (lib.rs:100-108): turn_timeout, max_turns, max_cost
- `CancellationToken` pattern throughout runtime, scheduler, tool execution
- `enforce_cost_budget()` (budget.rs:579-596): per-turn cost accumulation
- `SafetyTier` enum + `SecurityPolicy` trait for sandbox enforcement
- `AgentDefinition.tools: Option<Vec<String>>` — field exists (NOT enforced yet)
- `DelegationRequest` with max_turns/max_cost fields
- `StreamItem` / `GatewayServerFrame` streaming pipeline via mpsc+broadcast

**What needs to be built**:
- No centralized per-run policy object flowing end-to-end
- No wall-clock session-level deadline (only per-turn timeout)
- No tool schema filtering (schemas() returns ALL tools)
- No tool dispatch blocking by policy (only safety tier)
- Tool allowlists NOT enforced for delegation (explicit comment at delegation.rs:22-23)
- No budget cascading for delegated runs
- No StopReason enum (only RuntimeError::BudgetExceeded)
- No streaming policy events
- No SDK facade crate

### Metis Review
**Identified Gaps** (addressed):
- Added Phase 0 validation spikes before implementation
- Added performance guardrails (<1ms policy check, <1KB per RunPolicy)
- Locked down 10+ scope creep areas explicitly
- Added edge case handling decisions for budget, delegation, and tool scenarios
- Added backward compat verification as explicit task
- Added property-based tests for delegation inheritance

---

## Work Objectives

### Core Objective
Convert Oxydra into an embeddable AI SDK with first-class per-run policy controls (budget, timeout, tool allow/deny), enforced at every execution boundary, with delegation inheritance and backward compatibility.

### Concrete Deliverables
- `crates/sdk/` — New facade crate with public API
- `crates/types/src/policy.rs` — RunPolicyInput, EffectiveRunPolicy, ToolPolicyInput, StopReason
- `crates/types/src/policy_merge.rs` — Strictest-wins merge logic
- Modified `crates/runtime/src/lib.rs` — Policy-aware turn loop
- Modified `crates/runtime/src/tool_execution.rs` — Tool dispatch blocking
- Modified `crates/runtime/src/delegation.rs` — Policy inheritance
- Modified `crates/tools/src/registry.rs` — Schema filtering
- Modified `crates/gateway/src/lib.rs` — Admission resolution
- Modified `crates/runtime/src/scheduler_executor.rs` — Scheduler policy

### Definition of Done
- [ ] `cargo test --workspace` passes with zero failures
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean
- [ ] SDK can run one-shot and streaming with policy enforcement
- [ ] Delegation child cannot escalate parent policy
- [ ] Existing tests unchanged and passing (backward compat)
- [ ] All 8 enforcement points verified with dedicated tests

### Must Have
- Per-run policy resolved once at admission, immutable during run
- Tool allow/deny at both schema and dispatch layers
- Budget bounded-overrun semantics
- Wall-clock session deadline
- Delegation narrowing-only inheritance
- StopReason for every termination mode
- `policy: None` = exact current behavior
- All new public enums are `#[non_exhaustive]`

### Must NOT Have (Guardrails)
- No policy DSL or rule engine — simple types only
- No dynamic policy updates mid-run — immutable after admission
- No hook event bus — callback trait only (v1.1)
- No MCP runtime toggle — static config (v1.1)
- No model hot-swap during session (v1.1)
- No in-process extension registration (v1.1)
- No policy templates/presets library
- No distributed policy enforcement
- No fine-grained permissions beyond tool allow/deny
- No policy analytics/metrics dashboard
- No allocation in hot path (turn loop policy checks)
- No breaking changes to ToolRegistry public API
- No new crate dependencies increasing compile time >10%
- Do NOT rewrite existing core crates (runtime, tools, provider, memory) — additive only
- Do NOT use `as any` or `#[allow(clippy::*)]` — fix properly
- Do NOT introduce process-global delegation state

### Policy Invariants (must hold — from V1)
1. Policy resolved once at admission, persisted with run/session state
2. Deadline is immutable and cannot be widened after run start
3. Remaining budget is monotonic non-increasing and never negative
4. `disallowed_tools` always wins over any other tool setting
5. Delegated/scheduled child runs can only narrow parent policy
6. Resume path must rehydrate stored effective policy, never recompute from mutable global config

### Risk Register (from V1 + Metis)
1. **Tool ID mismatch bypass** — Mitigation: canonical tool name resolution via `canonical_tool_names()` in tools/src/lib.rs
2. **Budget races in parallel delegation** — Mitigation: atomic budget ledger with reservation/settlement
3. **Runtime checks only at turn boundaries** — Mitigation: mid-stream + tool-exec checks + cancellation token integration
4. **Resume widening policy** — Mitigation: persisted EffectiveRunPolicy snapshot is sole source of truth
5. **Ambiguous defaults when policy omitted** — Mitigation: explicit `policy: None` = current behavior, logged as default path
6. **Schema filtering breaks existing code** — Mitigation: validation spike (Task 3) before implementation

### Edge Case Decisions
- **Zero budget (max_cost = 0)**: Reject at admission with validation error
- **Partial token in bounded overrun**: Complete current provider call, stop before next turn
- **Nested tool calls**: Internal calls not subject to SDK-level policy (security layer handles those)
- **Delegation cycles**: Max delegation depth = 5, enforced at admission
- **Policy conflict in delegation**: Strictest-wins — parent always wins
- **Cancellation race during policy check**: CancellationToken is atomic; policy check is best-effort, cancellation takes precedence
- **Dynamic tool registration mid-run**: Not supported in v1 — tools fixed at run start

---

## Verification Strategy (MANDATORY)

> **ZERO HUMAN INTERVENTION** — ALL verification is agent-executed. No exceptions.

### Test Decision
- **Infrastructure exists**: YES — standard `cargo test` + tokio async tests
- **Automated tests**: YES (TDD for policy types and enforcement; tests-after for SDK API)
- **Framework**: `cargo test` with tokio, Mockall mocks, FakeProvider pattern
- **TDD targets**: Policy types, merge logic, enforcement checks, delegation inheritance
- **Tests-after targets**: SDK builder API, streaming events, examples

### QA Policy
Every task MUST include agent-executed QA scenarios.
Evidence saved to `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`.

- **Policy enforcement**: Use `cargo test` — run specific test modules, assert pass/fail counts
- **SDK API**: Use `cargo test -p sdk` — compile and test SDK crate
- **Integration**: Use `cargo test --workspace` — full workspace verification
- **Backward compat**: Use `cargo test --workspace` before AND after changes — diff results

### Verification Commands
```bash
cargo fmt --all -- --check                                    # Formatting
cargo clippy --workspace --all-targets -- -D warnings         # Linting
cargo test --workspace                                        # Full test suite
cargo test -p sdk                                             # SDK crate tests
cargo test -p types -- policy                                 # Policy type tests
cargo test -p runtime -- policy                               # Runtime enforcement tests
cargo test -p tools -- policy                                 # Tool policy tests
```

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 0 (Validation Spikes — verify assumptions before building):
├── Task 1: Verify runtime injection points at all 8 enforcement locations [deep]
├── Task 2: Test delegation depth (3-level parent→child→grandchild) [deep]
└── Task 3: Verify tool registry schema filtering compatibility [quick]

Wave 1 (Foundation Types — all parallel after Wave 0):
├── Task 4: SDK crate scaffolding (Cargo.toml, lib.rs, re-exports) [quick]
├── Task 5: RunPolicyInput + ToolPolicyInput types in crates/types [quick]
├── Task 6: EffectiveRunPolicy + StopReason types in crates/types [quick]
├── Task 7: Policy merge logic (strictest-wins) in crates/types [deep]
├── Task 8: ToolPermissionHandler callback trait in crates/types [quick]
└── Task 9: Backward compat test baseline (snapshot current behavior) [quick]

Wave 2 (Enforcement Pipeline — parallel after Wave 1):
├── Task 10: Admission resolution (RunPolicyInput → EffectiveRunPolicy) [deep]
├── Task 11: Runtime turn-loop enforcement (deadline + budget checks) [deep]
├── Task 12: Tool schema filtering in ToolRegistry [unspecified-high]
├── Task 13: Tool dispatch blocking at execute_with_policy_and_context [unspecified-high]
└── Task 14: SDK builder API (OxydraClient with one_shot + stream methods) [deep]

Wave 3 (Delegation + Accounting — parallel after Wave 2):
├── Task 15: Session budget ledger (atomic remaining counters) [deep]
├── Task 16: Delegation policy inheritance (parent→child narrowing) [deep]
├── Task 17: Scheduler policy integration [unspecified-high]
├── Task 18: Streaming policy events (BudgetWarning, PolicyStop) [unspecified-high]
└── Task 19: Control plane basics (interrupt/cancel via SDK) [unspecified-high]

Wave 4 (Hardening — parallel after Wave 3):
├── Task 20: Rollout mode flags (observe → soft-fail → hard-enforce) [unspecified-high]
├── Task 21: Edge case + property-based tests [deep]
├── Task 22: SDK examples (one-shot, streaming, delegation with policy) [quick]
└── Task 23: Backward compat verification suite [deep]

Wave FINAL (Independent review — 4 parallel after ALL tasks):
├── Task F1: Plan compliance audit [oracle]
├── Task F2: Code quality review [unspecified-high]
├── Task F3: Integration QA [unspecified-high]
└── Task F4: Scope fidelity check [deep]
```

### Dependency Matrix

| Task | Depends On | Blocks | Wave |
|------|-----------|--------|------|
| 1-3 | — | 4-9 | 0 |
| 4 | 1-3 | 14 | 1 |
| 5 | 1-3 | 7, 8, 10 | 1 |
| 6 | 1-3 | 7, 10, 11, 18 | 1 |
| 7 | 5, 6 | 10 | 1 |
| 8 | 5 | 13 | 1 |
| 9 | 1-3 | 23 | 1 |
| 10 | 6, 7 | 11, 12, 13, 14, 15, 16, 17 | 2 |
| 11 | 6, 10 | 15, 18 | 2 |
| 12 | 5, 10 | 16, 17 | 2 |
| 13 | 8, 10 | 16 | 2 |
| 14 | 4, 10 | 19, 22 | 2 |
| 15 | 10, 11 | 16, 20 | 3 |
| 16 | 10, 12, 13, 15 | 21 | 3 |
| 17 | 10, 12 | 21 | 3 |
| 18 | 6, 11 | 22 | 3 |
| 19 | 14 | 22 | 3 |
| 20 | 15 | 23 | 4 |
| 21 | 16, 17 | F1-F4 | 4 |
| 22 | 14, 18, 19 | F1-F4 | 4 |
| 23 | 9, 20 | F1-F4 | 4 |
| F1-F4 | 20-23 | — | FINAL |

### Agent Dispatch Summary

| Wave | Tasks | Categories |
|------|-------|-----------|
| 0 | 3 | T1→`deep`, T2→`deep`, T3→`quick` |
| 1 | 6 | T4→`quick`, T5→`quick`, T6→`quick`, T7→`deep`, T8→`quick`, T9→`quick` |
| 2 | 5 | T10→`deep`, T11→`deep`, T12→`unspecified-high`, T13→`unspecified-high`, T14→`deep` |
| 3 | 5 | T15→`deep`, T16→`deep`, T17→`unspecified-high`, T18→`unspecified-high`, T19→`unspecified-high` |
| 4 | 4 | T20→`unspecified-high`, T21→`deep`, T22→`quick`, T23→`deep` |
| FINAL | 4 | F1→`oracle`, F2→`unspecified-high`, F3→`unspecified-high`, F4→`deep` |

---

## TODOs


### Wave 0 — Validation Spikes (verify assumptions before building)

- [x] 1. **Verify Runtime Injection Points**

  **What to do**:
  - Read `crates/runtime/src/lib.rs` (turn loop), `budget.rs` (cost enforcement), `provider_response.rs` (timeout), `tool_execution.rs` (dispatch), `delegation.rs` (subagent), `scheduler_executor.rs` (scheduled runs)
  - For each of the 8 enforcement points (admission, turn-loop, provider-call, tool-schema, tool-dispatch, delegation, scheduler, resume): verify a code seam exists where a policy check can be inserted without rewriting the function
  - Document each point: file, line range, function name, how to inject (parameter addition, trait method, wrapper)
  - If any point requires a non-trivial rewrite, flag it with a recommended approach

  **Must NOT do**:
  - Do NOT modify any code — read-only analysis
  - Do NOT create any new files

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: []
  - **Reason**: Requires careful code reading across 6+ files with architectural judgment

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 0 (with Tasks 2, 3)
  - **Blocks**: Tasks 4-9 (all Wave 1)
  - **Blocked By**: None

  **References**:
  - `crates/runtime/src/lib.rs:363-620` — Turn loop `run_session_internal()`, check line 431 (cancellation), 437 (max_turns)
  - `crates/runtime/src/budget.rs:565-596` — `validate_guard_preconditions()` and `enforce_cost_budget()`
  - `crates/runtime/src/provider_response.rs:4-51` — `request_provider_response()` with timeout
  - `crates/runtime/src/tool_execution.rs:48-129` — `execute_tool_call()` dispatch path
  - `crates/runtime/src/delegation.rs:22-23,62-178` — Delegation executor, allowlist gap comment
  - `crates/runtime/src/scheduler_executor.rs:23-201` — Scheduler execution with child cancellation
  - `crates/types/src/config.rs:100-108` — RuntimeLimits struct
  - `docs/guidebook/05-agent-runtime.md` — Runtime architecture documentation

  **Acceptance Criteria**:
  - [ ] Document produced listing all 8 enforcement points with file:line references
  - [ ] Each point has: current code seam, injection method (param/trait/wrapper), risk level (low/medium/high)
  - [ ] Any high-risk points flagged with recommended approach

  **QA Scenarios**:

  ```
  Scenario: All 8 enforcement points have documented injection seams
    Tool: Bash (cargo test)
    Preconditions: Clean checkout of current main branch
    Steps:
      1. Read each of the 6 runtime files listed in references
      2. For each enforcement point, identify the exact function and line where policy check can be inserted
      3. Verify the function signature can accept an additional policy parameter or the check can be added inline
      4. Write findings to .sisyphus/evidence/task-1-injection-points.md
    Expected Result: 8 documented injection points, each with file:line:function and injection method
    Failure Indicators: Any enforcement point with no viable injection seam, or requiring complete function rewrite
    Evidence: .sisyphus/evidence/task-1-injection-points.md
  ```

  **Commit**: NO (read-only analysis)

---

- [x] 2. **Test Delegation Depth**

  **What to do**:
  - Write a minimal test that verifies 3-level delegation works: parent → child → grandchild
  - Use the existing FakeProvider pattern from `crates/runtime/src/tests.rs`
  - The test should: create parent runtime, delegate to child agent, child delegates to grandchild agent
  - Verify: each level receives correct context, cancellation propagates, session IDs chain correctly
  - Test max delegation depth enforcement (currently no limit — document what depth causes issues)
  - This is a spike test to validate the delegation infrastructure can support policy inheritance

  **Must NOT do**:
  - Do NOT implement policy inheritance yet — this is just validating the delegation infrastructure
  - Do NOT modify delegation.rs production code

  **Recommended Agent Profile**:
  - **Category**: `deep`
  - **Skills**: []
  - **Reason**: Needs to understand delegation wiring and write multi-level async tests

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 0 (with Tasks 1, 3)
  - **Blocks**: Tasks 4-9 (all Wave 1), especially Task 16 (delegation policy)
  - **Blocked By**: None

  **References**:
  - `crates/runtime/src/delegation.rs:25-178` — `RuntimeDelegationExecutor::delegate()` implementation
  - `crates/runtime/src/tests.rs:70-160` — FakeProvider pattern for deterministic testing
  - `crates/runtime/src/tests.rs:34-60` — Mockall mock pattern
  - `crates/types/src/delegation.rs:15-56` — DelegationRequest, DelegationResult, DelegationExecutor trait
  - `crates/types/src/config.rs:577-592` — AgentDefinition with tools, max_turns, max_cost

  **Acceptance Criteria**:
  - [ ] Test file created with 3-level delegation spike
  - [ ] Test passes: `cargo test -p runtime -- delegation_depth_spike`
  - [ ] Documented: max observed delegation depth before issues, session ID chain format

  **QA Scenarios**:

  ```
  Scenario: 3-level delegation completes successfully
    Tool: Bash (cargo test)
    Preconditions: Current codebase compiles cleanly
    Steps:
      1. Run `cargo test -p runtime -- delegation_depth_spike --nocapture`
      2. Verify test output shows 3 levels of delegation completing
      3. Verify session ID chain: parent → subagent:parent:uuid → subagent:subagent:parent:uuid:uuid
    Expected Result: Test passes, 3 levels complete without panic or timeout
    Evidence: .sisyphus/evidence/task-2-delegation-depth.txt

  Scenario: Deep delegation (>5 levels) behavior documented
    Tool: Bash (cargo test)
    Preconditions: Spike test created
    Steps:
      1. Run spike with 6+ levels if possible, or document infrastructure limitations
      2. Record findings: max depth, failure mode (stack overflow, timeout, error)
    Expected Result: Documented behavior for deep delegation
    Evidence: .sisyphus/evidence/task-2-deep-delegation.txt
  ```

  **Commit**: YES (spike test)
  - Message: `test(runtime): add delegation depth spike test`
  - Files: `crates/runtime/src/tests.rs`
  - Pre-commit: `cargo test -p runtime -- delegation`

---

- [x] 3. **Verify Tool Registry Schema Filtering Compatibility**

  **What to do**:
  - Read `crates/tools/src/registry.rs` — understand `schemas()` method (line 59-61)
  - Read `crates/runtime/src/lib.rs:371-373` — where `context.tools` is populated from registry
  - Search codebase for all callers of `ToolRegistry::schemas()` and `context.tools`
  - Determine: if `schemas()` returned a filtered subset, would any caller break?
  - Check: does the provider layer assume all tools are present? Does any test hard-code tool counts?
  - Document findings: safe to filter (yes/no), any callers that need adjustment

  **Must NOT do**:
  - Do NOT modify any code — read-only analysis
  - Do NOT implement schema filtering yet

  **Recommended Agent Profile**:
  - **Category**: `quick`
  - **Skills**: []
  - **Reason**: Straightforward reference tracing with LSP

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 0 (with Tasks 1, 2)
  - **Blocks**: Tasks 4-9 (all Wave 1), especially Task 12 (tool schema filtering)
  - **Blocked By**: None

  **References**:
  - `crates/tools/src/registry.rs:59-61` — `schemas()` method returning all tool schemas
  - `crates/runtime/src/lib.rs:371-373` — `context.tools = self.tool_registry.schemas()` population
  - `crates/types/src/model.rs:114-122` — `Context` struct with `tools: Vec<FunctionDecl>`
  - `crates/tools/src/registry.rs:105-141` — `execute_with_policy_and_context()` dispatch path

  **Acceptance Criteria**:
  - [ ] All callers of `schemas()` documented
  - [ ] All callers of `context.tools` documented
  - [ ] Compatibility assessment: safe to filter YES/NO with rationale
  - [ ] Any needed adjustments listed

  **QA Scenarios**:

  ```
  Scenario: Schema filtering compatibility assessed
    Tool: Bash (grep + LSP)
    Preconditions: Current codebase
    Steps:
      1. Use lsp_find_references on ToolRegistry::schemas() to find all callers
      2. Use lsp_find_references on Context.tools to find all consumers
      3. For each caller/consumer, determine if it assumes the full tool set
      4. Write assessment to .sisyphus/evidence/task-3-schema-compat.md
    Expected Result: Compatibility report with YES/NO verdict and caller list
    Evidence: .sisyphus/evidence/task-3-schema-compat.md
  ```

  **Commit**: NO (read-only analysis)


### Wave 1 — Foundation Types (all parallel after Wave 0)

- [x] 4. **SDK Crate Scaffolding**

  **What to do**:
  - Create `crates/sdk/Cargo.toml` with dependencies on `types`, `runtime`, `tools`, `gateway`
  - Create `crates/sdk/src/lib.rs` with module structure: `pub mod client;`, `pub mod policy;`, `pub mod events;`
  - Add `sdk` to workspace `Cargo.toml` members
  - Create stub modules with `// TODO: implementation` placeholders
  - Verify `cargo build -p sdk` compiles
  - Re-export key types from `types` crate for SDK consumers

  **Must NOT do**:
  - Do NOT implement any business logic yet
  - Do NOT add external dependencies beyond workspace crates
  - Do NOT create a separate repo

  **Recommended Agent Profile**:
  - **Category**: `quick` — Boilerplate crate setup
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES — Wave 1 (with Tasks 5-9)
  - **Blocks**: Task 14
  - **Blocked By**: Tasks 1-3

  **References**:
  - `Cargo.toml` (workspace root) — `[workspace] members` to add sdk
  - `crates/runner/Cargo.toml` — Reference for internal crate dependency pattern
  - `crates/types/src/lib.rs` — Types to re-export

  **Acceptance Criteria**:
  - [ ] `crates/sdk/Cargo.toml` exists with correct deps
  - [ ] `cargo build -p sdk` succeeds
  - [ ] `cargo clippy -p sdk -- -D warnings` clean
  - [ ] `cargo test --workspace` no regressions

  **QA Scenarios**:

  ```
  Scenario: SDK crate compiles cleanly
    Tool: Bash
    Steps: 1. `cargo build -p sdk` 2. `cargo clippy -p sdk -- -D warnings` 3. `cargo test --workspace`
    Expected Result: All exit code 0
    Evidence: .sisyphus/evidence/task-4-sdk-scaffold.txt
  ```

  **Commit**: YES — `feat(sdk): add crate scaffolding with module stubs`

---

- [x] 5. **RunPolicyInput + ToolPolicyInput Types**

  **What to do**:
  - Create `crates/types/src/policy.rs` with `RunPolicyInput` and `ToolPolicyInput`
  - `RunPolicyInput { max_runtime: Option<Duration>, max_budget_microusd: Option<u64>, max_turns: Option<usize>, tool_policy: Option<ToolPolicyInput> }`
  - `ToolPolicyInput { toolset: Option<Vec<String>>, auto_approve_tools: Option<Vec<String>>, disallowed_tools: Option<Vec<String>> }`
  - Both: `Clone + Debug + Serialize + Deserialize + Default`
  - Add `pub mod policy;` to `crates/types/src/lib.rs`
  - Tests: construction, serde roundtrip, Default = all None

  **Must NOT do**: No merge logic (Task 7), no EffectiveRunPolicy (Task 6)

  **Recommended Agent Profile**: `quick` — Simple type definitions with derive macros
  **Skills**: []

  **Parallelization**: YES — Wave 1 | Blocks: 7, 8, 10 | Blocked By: 1-3

  **References**:
  - `crates/types/src/config.rs:152-164` — RuntimeConfig naming conventions
  - `crates/types/src/delegation.rs:15-29` — DelegationRequest for max_turns/max_cost pattern
  - `plna_v1.md:63-78` — V1 type definitions
  - `crates/types/src/tool.rs:13-19` — SafetyTier serialization pattern

  **Acceptance Criteria**:
  - [ ] `crates/types/src/policy.rs` exists
  - [ ] `cargo test -p types -- policy` passes
  - [ ] Both types `Clone + Debug + Serialize + Deserialize + Default`

  **QA Scenarios**:

  ```
  Scenario: Policy input types compile and serialize
    Tool: Bash
    Steps: 1. `cargo test -p types -- policy::tests --nocapture`
    Expected Result: Serde roundtrip and Default tests pass
    Evidence: .sisyphus/evidence/task-5-policy-types.txt
  ```

  **Commit**: YES — `feat(types): add RunPolicyInput and ToolPolicyInput types`

---

- [x] 6. **EffectiveRunPolicy + StopReason Types**

  **What to do**:
  - Add to `crates/types/src/policy.rs`:
  - `EffectiveRunPolicy { started_at: Instant, deadline: Option<Instant>, initial_budget_microusd: Option<u64>, remaining_budget_microusd: Option<u64>, toolset: BTreeSet<String>, auto_approve_tools: BTreeSet<String>, disallowed_tools: BTreeSet<String>, parent_run_id: Option<String>, max_turns: usize, rollout_mode: RolloutMode }`
  - `#[non_exhaustive] StopReason { Completed, Cancelled, MaxTurns, MaxRuntimeExceeded, MaxBudgetExceeded, ToolDisallowed { tool: String }, ToolPermissionDenied { tool: String, reason: String }, ProviderTimedOut }`
  - `RolloutMode { Enforce, SoftFail, ObserveOnly }`
  - StopReason: `Clone + Debug + PartialEq + Serialize`, `#[non_exhaustive]`
  - Tests: variant construction, wildcard matching

  **Must NOT do**: No merge logic (Task 7), no runtime wiring yet

  **Recommended Agent Profile**: `quick` — Type defs with Instant/atomic complexity
  **Skills**: []

  **Parallelization**: YES — Wave 1 | Blocks: 7, 10, 11, 18 | Blocked By: 1-3

  **References**:
  - `plna_v1.md:82-102` — V1 EffectiveRunPolicy and StopReason
  - `crates/types/src/delegation.rs:41-46` — DelegationStatus termination pattern
  - `crates/types/src/error.rs:96-110` — RuntimeError existing variants
  - `crates/types/src/model.rs:160-175` — StreamItem non_exhaustive pattern

  **Acceptance Criteria**:
  - [ ] StopReason `#[non_exhaustive]` verified
  - [ ] `cargo test -p types -- policy` passes

  **QA Scenarios**:

  ```
  Scenario: StopReason non_exhaustive and all variants work
    Tool: Bash
    Steps: 1. `cargo test -p types -- policy::tests::stop_reason --nocapture`
    Expected Result: All variants tested, wildcard required
    Evidence: .sisyphus/evidence/task-6-stop-reason.txt
  ```

  **Commit**: YES — `feat(types): add EffectiveRunPolicy, StopReason, RolloutMode`

---

- [x] 7. **Policy Merge Logic (strictest-wins)**

  **What to do**:
  - Create `crates/types/src/policy_merge.rs` with `merge_policy(global: &RuntimeConfig, agent: &AgentDefinition, per_run: &RunPolicyInput) -> EffectiveRunPolicy`
  - Merge semantics: strictest-wins across all layers
    - `max_turns`: min(global, agent, per_run)
    - `max_cost`: min(global, agent, per_run)
    - `max_runtime`: min(global turn_timeout * max_turns, per_run.max_runtime)
    - `toolset`: intersection of available tools and agent allowlist and per_run toolset
    - `disallowed_tools`: union of all disallow lists (disallow ALWAYS wins)
    - `auto_approve_tools`: intersection with allowed toolset
  - TDD: write failing tests first, then implement
  - Tests: each merge rule in isolation, combined merge, edge cases (empty sets, None = no restriction)

  **Must NOT do**: No runtime wiring, no admission integration

  **Recommended Agent Profile**: `deep` — Complex merge logic with edge cases
  **Skills**: []

  **Parallelization**: YES — Wave 1 | Blocks: 10 | Blocked By: 5, 6

  **References**:
  - `crates/types/src/config.rs:152-164` — RuntimeConfig (global limits)
  - `crates/types/src/config.rs:577-592` — AgentDefinition (agent-level limits + tool allowlist)
  - `plna_v1.md:110-118` — V1 policy invariants (strictest-wins, disallow precedence)
  - `plan_v2.md:83-85` — V2 merge rule description

  **Acceptance Criteria**:
  - [ ] Merge function exists and is tested
  - [ ] `disallowed_tools` always wins (test: tool in both allowed and disallowed → disallowed)
  - [ ] `None` = no restriction (test: None max_cost + Some max_cost → Some)
  - [ ] Empty toolset = all tools allowed (test)
  - [ ] `cargo test -p types -- policy_merge` all pass

  **QA Scenarios**:

  ```
  Scenario: Strictest-wins merge produces correct effective policy
    Tool: Bash
    Steps:
      1. `cargo test -p types -- policy_merge::tests --nocapture`
      2. Verify: disallowed_tools precedence test passes
      3. Verify: min(global, agent, per_run) for budget/turns
    Expected Result: All merge rule tests pass
    Evidence: .sisyphus/evidence/task-7-merge-logic.txt

  Scenario: None means no restriction
    Tool: Bash
    Steps: 1. `cargo test -p types -- policy_merge::tests::none_means_no_restriction`
    Expected Result: None field does not constrain merge output
    Evidence: .sisyphus/evidence/task-7-none-handling.txt
  ```

  **Commit**: YES — `feat(types): add strictest-wins policy merge logic`

---

- [x] 8. **ToolPermissionHandler Callback Trait**

  **What to do**:
  - Add to `crates/types/src/policy.rs`:
    - `#[async_trait] trait ToolPermissionHandler: Send + Sync { async fn check_permission(&self, tool_name: &str, arguments: &serde_json::Value, context: &ToolPermissionContext) -> ToolPermissionDecision; }`
    - `ToolPermissionContext { session_id: String, user_id: String, turn: usize, remaining_budget: Option<u64> }`
    - `ToolPermissionDecision { Allow, Deny { reason: String }, AllowWithModification { modified_args: serde_json::Value } }`
  - Provide `DefaultToolPermissionHandler` that always returns Allow
  - Tests: default handler returns Allow, deny decision propagates reason

  **Must NOT do**: No integration with tool dispatch yet (Task 13)

  **Recommended Agent Profile**: `quick` — Trait definition + default impl
  **Skills**: []

  **Parallelization**: YES — Wave 1 | Blocks: 13 | Blocked By: 5

  **References**:
  - `crates/types/src/tool.rs:122-135` — Tool trait for async_trait pattern
  - `crates/types/src/delegation.rs:48-56` — DelegationExecutor trait pattern with CancellationToken

  **Acceptance Criteria**:
  - [ ] Trait and default impl exist
  - [ ] `cargo test -p types -- policy::permission` passes

  **QA Scenarios**:

  ```
  Scenario: Default handler allows all
    Tool: Bash
    Steps: 1. `cargo test -p types -- policy::permission::tests --nocapture`
    Expected Result: DefaultToolPermissionHandler returns Allow for any input
    Evidence: .sisyphus/evidence/task-8-permission-trait.txt
  ```

  **Commit**: YES — `feat(types): add ToolPermissionHandler callback trait`

---

- [x] 9. **Backward Compat Test Baseline**

  **What to do**:
  - Run `cargo test --workspace` and capture full output as baseline
  - Record: total tests, passed, failed, ignored per crate
  - Save to `.sisyphus/evidence/task-9-baseline.txt`
  - This baseline will be compared after all changes to verify zero regressions

  **Must NOT do**: Do NOT modify any existing test or production code

  **Recommended Agent Profile**: `quick` — Single command + capture
  **Skills**: []

  **Parallelization**: YES — Wave 1 | Blocks: 23 | Blocked By: 1-3

  **References**:
  - `docs/guidebook/10-testing-and-quality.md` — Test strategy documentation

  **Acceptance Criteria**:
  - [ ] Baseline file exists at `.sisyphus/evidence/task-9-baseline.txt`
  - [ ] Contains per-crate test pass/fail/ignore counts

  **QA Scenarios**:

  ```
  Scenario: Baseline captured
    Tool: Bash
    Steps:
      1. `cargo test --workspace 2>&1 | tee .sisyphus/evidence/task-9-baseline.txt`
      2. Verify file contains test results for all crates
    Expected Result: Baseline file with per-crate counts
    Evidence: .sisyphus/evidence/task-9-baseline.txt
  ```

  **Commit**: NO (evidence only)


### Wave 2 — Enforcement Pipeline (parallel after Wave 1)

- [x] 10. **Admission Resolution** — `deep`

  **What to do**: Create `crates/runtime/src/policy_guard.rs` with `resolve_policy(global_config: &AgentConfig, agent_def: &AgentDefinition, per_run: Option<&RunPolicyInput>, available_tools: &[String]) -> Result<EffectiveRunPolicy, PolicyValidationError>`. Validate inputs (reject zero budget, negatives). Call merge logic (Task 7). Set `started_at`, compute `deadline`, resolve `toolset` against registry. Add `PolicyValidationError` enum. Wire into gateway admission path. Persist EffectiveRunPolicy in session state for resume.
  **Must NOT do**: No runtime enforcement (Task 11), no tool filtering (Task 12)
  **Parallelization**: Wave 2 | Blocks: 11-17 | Blocked By: 6, 7
  **References**: `runtime/src/budget.rs:565-577` (validation pattern), `types/src/policy_merge.rs` (merge logic), `gateway/src/lib.rs:213-303` (admission entry), `gateway/src/turn_runner.rs:8-24` (boundary types)
  **Acceptance**: resolve_policy() handles all combos, zero budget rejected, `None` = defaults, `cargo test -p runtime -- policy_guard` passes
  **QA**: `cargo test -p runtime -- policy_guard::tests --nocapture` — valid resolves, None defaults, zero rejected. Evidence: `.sisyphus/evidence/task-10-admission.txt`
  **Commit**: `feat(runtime): add policy admission resolution and validation`

---

- [x] 11. **Runtime Turn-Loop Enforcement** — `deep`

  **What to do**: Modify `run_session_internal()` in `runtime/src/lib.rs` to accept `EffectiveRunPolicy`. Add deadline check before each provider call (`Instant::now() < deadline` → `StopReason::MaxRuntimeExceeded`). Add budget check after cost settlement (`remaining < 0` → `StopReason::MaxBudgetExceeded`, bounded overrun). Replace scattered RuntimeLimits checks with unified policy checks. Ensure `policy: None` path hits existing code. TDD: write failing deadline/budget tests first.
  **Must NOT do**: No tool filtering (12-13), no delegation (16)
  **Parallelization**: Wave 2 | Blocks: 15, 18 | Blocked By: 6, 10
  **References**: `runtime/src/lib.rs:431-619` (turn loop), `runtime/src/budget.rs:579-596` (enforce_cost_budget), `runtime/src/provider_response.rs:4-51` (timeout pattern), `runtime/src/lib.rs:100-108` (RuntimeLimits)
  **Acceptance**: Deadline fires MaxRuntimeExceeded, budget fires MaxBudgetExceeded (bounded overrun), None = current behavior, `cargo test -p runtime -- policy` passes
  **QA**: `cargo test -p runtime -- policy::tests::deadline_exceeded` + `policy::tests::budget_overrun`. Evidence: `.sisyphus/evidence/task-11-deadline.txt`, `task-11-budget.txt`
  **Commit**: `feat(runtime): add policy-aware deadline and budget enforcement in turn loop`

---

- [x] 12. **Tool Schema Filtering** — `unspecified-high`

  **What to do**: Add `filtered_schemas(effective_toolset: &BTreeSet<String>, disallowed: &BTreeSet<String>) -> Vec<FunctionDecl>` to `ToolRegistry` in `tools/src/registry.rs`. Return schemas where `name in toolset AND name NOT in disallowed`. Modify `run_session_internal()` to call `filtered_schemas()` when policy present. Keep `schemas()` unchanged (backward compat). TDD: subset, empty set, disallowed overriding allowed.
  **Must NOT do**: No dispatch blocking (13), no delegation (16)
  **Parallelization**: Wave 2 | Blocks: 16, 17 | Blocked By: 5, 10
  **References**: `tools/src/registry.rs:59-61` (schemas()), `runtime/src/lib.rs:371-373` (context.tools population), `tools/src/lib.rs:92-124` (canonical_tool_names), Task 3 evidence
  **Acceptance**: filtered_schemas() exists, schemas() unchanged, disallowed excluded even if in toolset, `cargo test -p tools -- registry::filtered` passes
  **QA**: `cargo test -p tools -- registry::tests::filtered --nocapture`. Evidence: `.sisyphus/evidence/task-12-schema-filter.txt`
  **Commit**: `feat(tools): add filtered_schemas() to ToolRegistry`

---

- [x] 13. **Tool Dispatch Blocking** — `unspecified-high`

  **What to do**: Modify `execute_with_policy_and_context()` in `tools/src/registry.rs` to check disallowed_tools before execution (→ `StopReason::ToolDisallowed`). Add ToolPermissionHandler callback check for non-auto-approved tools (→ `StopReason::ToolPermissionDenied`). Defense-in-depth: catches tools that bypass schema filtering. New check goes BEFORE existing SafetyTier/SecurityPolicy checks.
  **Must NOT do**: No delegation (16)
  **Parallelization**: Wave 2 | Blocks: 16 | Blocked By: 8, 10
  **References**: `tools/src/registry.rs:105-141` (dispatch entry), `runtime/src/tool_execution.rs:48-129` (caller context), `types/src/policy.rs` (ToolPermissionHandler from Task 8)
  **Acceptance**: Disallowed blocked at dispatch, handler deny propagates reason, `cargo test -p tools -- dispatch_policy` passes
  **QA**: `cargo test -p tools -- dispatch_policy::tests::disallowed_blocked` + `handler_deny`. Evidence: `.sisyphus/evidence/task-13-dispatch-block.txt`
  **Commit**: `feat(tools): add policy-based dispatch blocking with permission handler`

---

- [x] 14. **SDK Builder API** — `deep`

  **What to do**: Implement `crates/sdk/src/client.rs` with `OxydraClient::builder().config(cfg).build()`. Add `client.one_shot(prompt, policy) -> RunResult` (single turn, returns response + StopReason). Add `client.stream(prompt, policy) -> impl Stream<Item=RunEvent>` (streaming multi-turn). Define `RunResult { response, stop_reason, usage, tool_calls }` and `RunEvent` enum (Text, ToolCall, ToolResult, BudgetUpdate, PolicyStop, Completed). Wire through AgentRuntime and GatewayServer internals. One-shot: ephemeral session. Streaming: subscribe to broadcast events, translate to RunEvent.
  **Must NOT do**: No control plane (19), no rollout flags (20)
  **Parallelization**: Wave 2 | Blocks: 19, 22 | Blocked By: 4, 10
  **References**: `gateway/src/lib.rs:213-303` (session creation), `gateway/src/turn_runner.rs:8-24` (TurnRunner types), `types/src/model.rs:160-175` (StreamItem), `types/src/channel.rs:403-427` (GatewayServerFrame), `runner/src/bootstrap.rs:250-283` (config loading)
  **Acceptance**: Builder compiles, one_shot returns RunResult, stream returns Stream<RunEvent>, `cargo test -p sdk -- client` passes
  **QA**: `cargo test -p sdk -- client::tests::one_shot_no_policy` + `stream_events`. Evidence: `.sisyphus/evidence/task-14-one-shot.txt`, `task-14-stream.txt`
  **Commit**: `feat(sdk): add OxydraClient with one_shot and stream builder API`


### Wave 3 — Delegation + Accounting (parallel after Wave 2)

- [ ] 15. **Session Budget Ledger** — `deep`

  **What to do**: Create `runtime/src/budget_ledger.rs` with `BudgetLedger` (atomic remaining counters). Methods: `reserve(estimated_cost) -> Result<Reservation, BudgetExhausted>`, `settle(actual_cost)`, `remaining() -> BudgetSnapshot`. Reserve before provider call, settle after. Thread-safe: parent ledger via Arc, children get sub-ledgers deducting from parent. Integrate with turn loop (Task 11).
  **Parallelization**: Wave 3 | Blocks: 16, 20 | Blocked By: 10, 11
  **References**: `runtime/src/budget.rs:579-596` (current cost enforcement), `runtime/src/lib.rs:431-619` (turn loop), `plna_v1.md:113-114` (invariant: monotonic non-increasing)
  **Acceptance**: Reserve/settle works, concurrent safe, parent budget decreases on child settlement, `cargo test -p runtime -- budget_ledger` passes
  **QA**: `cargo test -p runtime -- budget_ledger::tests --nocapture`. Evidence: `.sisyphus/evidence/task-15-ledger.txt`
  **Commit**: `feat(runtime): add atomic session budget ledger with reservation/settlement`

---

- [ ] 16. **Delegation Policy Inheritance** — `deep`

  **What to do**: Modify `RuntimeDelegationExecutor::delegate()` to: (1) resolve child EffectiveRunPolicy by narrowing parent remaining (budget = min(parent_remaining, child_requested), tools = intersection), (2) enforce max delegation depth = 5, (3) filter child tools via `filtered_schemas()`, (4) pass child policy to child runtime. Remove "allowlists not enforced" comment (delegation.rs:22-23). Child cancellation inherits parent remaining deadline.
  **Parallelization**: Wave 3 | Blocks: 21 | Blocked By: 10, 12, 13, 15
  **References**: `runtime/src/delegation.rs:22-23` (gap comment), `runtime/src/delegation.rs:62-178` (delegate()), `types/src/delegation.rs:15-56` (types), `types/src/config.rs:577-592` (AgentDefinition.tools)
  **Acceptance**: Child cannot escalate, depth > 5 rejected, allowlist enforced, `cargo test -p runtime -- delegation::policy` passes
  **QA**: Test: parent allows [A,B], child requests [A,B,C] → gets [A,B]. Evidence: `.sisyphus/evidence/task-16-delegation.txt`
  **Commit**: `feat(runtime): enforce delegation policy narrowing and tool allowlists`

---

- [ ] 17. **Scheduler Policy Integration** — `unspecified-high`

  **What to do**: Add `policy: Option<RunPolicyInput>` to `ScheduleDefinition`. On schedule_create, capture creating user's policy context. On execute_schedule(), resolve via admission (Task 10). Merge with global SchedulerConfig (strictest-wins). Ensure same admission pipeline as interactive runs.
  **Parallelization**: Wave 3 | Blocks: 21 | Blocked By: 10, 12
  **References**: `runtime/src/scheduler_executor.rs:23-201`, `types/src/scheduler.rs`, `tools/src/scheduler_tools.rs`, `types/src/config.rs:886-922` (SchedulerConfig)
  **Acceptance**: Scheduled runs use admission pipeline, stored policy honored, `cargo test -p runtime -- scheduler::policy` passes
  **QA**: `cargo test -p runtime -- scheduler::policy::tests --nocapture`. Evidence: `.sisyphus/evidence/task-17-scheduler.txt`
  **Commit**: `feat(runtime): add scheduler policy integration via admission pipeline`

---

- [ ] 18. **Streaming Policy Events** — `unspecified-high`

  **What to do**: Extend `StreamItem` with `PolicyEvent(PolicyStreamEvent)`. Define `PolicyStreamEvent { BudgetWarning { remaining, threshold_pct }, PolicyStop { reason: StopReason }, BudgetUpdate { remaining } }`. Extend `GatewayServerFrame` with `PolicyNotification`. Emit BudgetUpdate after settlement, BudgetWarning at 80%/95%, PolicyStop on termination. Translate in SDK RunEvent.
  **Parallelization**: Wave 3 | Blocks: 22 | Blocked By: 6, 11
  **References**: `types/src/model.rs:160-175` (StreamItem), `types/src/channel.rs:403-427` (GatewayServerFrame), `runtime/src/lib.rs:460` (event sending)
  **Acceptance**: Events emitted at correct thresholds, `cargo test -p types -p runtime -- policy_event` passes
  **QA**: `cargo test -p runtime -- policy_event::tests --nocapture`. Evidence: `.sisyphus/evidence/task-18-events.txt`
  **Commit**: `feat(types,runtime): add streaming policy events`

---

- [ ] 19. **Control Plane Basics** — `unspecified-high`

  **What to do**: Add to SDK: `client.cancel(session_id)` (triggers CancellationToken), `client.get_status(session_id) -> SessionStatus` (turn, budget_remaining, is_active, stop_reason). Wire cancel through existing CancellationToken. Status reads session state + budget ledger.
  **Parallelization**: Wave 3 | Blocks: 22 | Blocked By: 14
  **References**: `runtime/src/lib.rs:432,583` (cancellation), `runtime/src/scheduler_executor.rs:106` (child_token), `gateway/src/session.rs:87-100` (SessionState)
  **Acceptance**: cancel() triggers token, get_status() returns state, `cargo test -p sdk -- control` passes
  **QA**: `cargo test -p sdk -- control::tests --nocapture`. Evidence: `.sisyphus/evidence/task-19-control.txt`
  **Commit**: `feat(sdk): add cancel and get_status control plane API`


### Wave 4 — Hardening (parallel after Wave 3)

- [ ] 20. **Rollout Mode Flags** — `unspecified-high`

  **What to do**: Implement RolloutMode behavior across all enforcement points. `Enforce`: block/stop. `SoftFail`: log but continue (emit PolicyEvent). `ObserveOnly`: log evaluations, never block. Each enforcement point checks mode before acting. Default `Enforce` for new policies, backward compat path skips rollout.
  **Parallelization**: Wave 4 | Blocks: 23 | Blocked By: 15
  **References**: `runtime/src/policy_guard.rs` (admission), `runtime/src/lib.rs` (turn loop), `tools/src/registry.rs` (dispatch), `plna_v1.md:253-260` (rollout phase)
  **Acceptance**: SoftFail logs+continues, ObserveOnly never blocks, Enforce blocks, `cargo test -p runtime -- rollout` passes
  **QA**: `cargo test -p runtime -- rollout::tests --nocapture`. Evidence: `.sisyphus/evidence/task-20-rollout.txt`
  **Commit**: `feat(runtime): add rollout mode flags (enforce, soft-fail, observe-only)`

---

- [ ] 21. **Edge Case + Property-Based Tests** — `deep`

  **What to do**: Tests for all edge cases: (1) zero budget rejected, (2) bounded overrun completes call, (3) depth > 5 rejected, (4) parent denies + child allows = denied, (5) cancellation during policy check, (6) empty toolset = all, (7) disallowed always wins. Property-based tests (proptest): delegation chain preserves narrowing-only across random policies.
  **Parallelization**: Wave 4 | Blocks: F1-F4 | Blocked By: 16, 17
  **References**: All evidence files Tasks 10-17, `plna_v1.md:110-118` (invariants), `plna_v1.md:264-288` (test plan)
  **Acceptance**: All 7 edge cases tested, property tests pass 1000+ inputs, `cargo test --workspace -- edge_case` passes
  **QA**: `cargo test --workspace -- edge_case --nocapture`. Evidence: `.sisyphus/evidence/task-21-edge-cases.txt`
  **Commit**: `test: add edge case and property-based tests for policy enforcement`

---

- [ ] 22. **SDK Examples** — `quick`

  **What to do**: Create `crates/sdk/examples/`: (1) `one_shot.rs` — no policy, (2) `policy_enforcement.rs` — budget + tool policy, (3) `streaming.rs` — streaming with BudgetUpdate, (4) `delegation_policy.rs` — parent→child narrowing. All compile, inline doc comments.
  **Parallelization**: Wave 4 | Blocks: F1-F4 | Blocked By: 14, 18, 19
  **References**: `crates/sdk/src/client.rs` (API from Task 14), `crates/types/src/policy.rs` (types)
  **Acceptance**: All 4 compile, `cargo build --examples -p sdk` succeeds
  **QA**: `cargo build --examples -p sdk`. Evidence: `.sisyphus/evidence/task-22-examples.txt`
  **Commit**: `docs(sdk): add one-shot, policy, streaming, and delegation examples`

---

- [ ] 23. **Backward Compat Verification Suite** — `deep`

  **What to do**: Compare `cargo test --workspace` to Task 9 baseline. Verify: zero regressions, no existing tests modified, `policy: None` identical behavior. Dedicated compat tests: old-format requests (no policy), full SDK run with None matches pre-SDK.
  **Parallelization**: Wave 4 | Blocks: F1-F4 | Blocked By: 9, 20
  **References**: `.sisyphus/evidence/task-9-baseline.txt` (baseline), `gateway/src/tests.rs` (existing tests)
  **Acceptance**: Zero regressions, None identical, old format accepted, `cargo test --workspace` all pass
  **QA**: Compare test counts to baseline. Evidence: `.sisyphus/evidence/task-23-compat.txt`
  **Commit**: `test: add backward compatibility verification suite`

---

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Rejection → fix → re-run.

- [ ] F1. **Plan Compliance Audit** — `oracle`
  Read the plan end-to-end. For each "Must Have": verify implementation exists (read file, run command). For each "Must NOT Have": search codebase for forbidden patterns — reject with file:line if found. Check evidence files in .sisyphus/evidence/. Compare deliverables against plan. Verify all 6 policy invariants hold via test output.
  Output: `Must Have [N/N] | Must NOT Have [N/N] | Invariants [6/6] | VERDICT: APPROVE/REJECT`

- [ ] F2. **Code Quality Review** — `unspecified-high`
  Run `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo test --workspace`. Review all changed files for: `as any`/`#[allow(clippy::*)]`, empty catches, `println!`/`dbg!` in prod, commented-out code, unused imports. Check AI slop: excessive comments, over-abstraction, generic names.
  Output: `Build [PASS/FAIL] | Clippy [PASS/FAIL] | Tests [N pass/N fail] | Files [N clean/N issues] | VERDICT`

- [ ] F3. **Integration QA** — `unspecified-high`
  Start from clean state (`cargo clean && cargo build --workspace`). Run every QA scenario from every task — follow exact steps. Test cross-task integration: SDK→admission→runtime→tools→delegation full pipeline. Test edge cases: zero budget, empty toolset, deep delegation. Save evidence to `.sisyphus/evidence/final-qa/`.
  Output: `Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [ ] F4. **Scope Fidelity Check** — `deep`
  For each task: read "What to do", read actual diff. Verify 1:1 — everything in spec built, nothing beyond spec built. Check "Must NOT Have" compliance. Detect cross-task contamination. Verify `policy: None` backward compat by running the full existing test suite and comparing to baseline.
  Output: `Tasks [N/N compliant] | Contamination [CLEAN/N issues] | Backward Compat [PASS/FAIL] | VERDICT`

---

## Commit Strategy

| PR | Tasks | Commit Message | Pre-commit |
|----|-------|---------------|------------|
| PR-1: `sdk-types` | 5, 6, 7, 8 | `feat(types): add RunPolicy, EffectiveRunPolicy, StopReason, and policy merge logic` | `cargo test -p types` |
| PR-2: `sdk-foundation` | 4, 9, 14 | `feat(sdk): add crate scaffolding with one-shot and streaming builder API` | `cargo test -p sdk` |
| PR-3: `sdk-enforcement` | 10, 11, 12, 13 | `feat(runtime,tools): add policy enforcement pipeline at admission, runtime, and tool layers` | `cargo test -p runtime -p tools` |
| PR-4: `sdk-session-ledger` | 15 | `feat(runtime): add atomic session budget ledger with reservation/settlement` | `cargo test -p runtime -- budget` |
| PR-5: `sdk-delegation-policy` | 16, 17 | `feat(runtime): enforce delegation narrowing and scheduler policy inheritance` | `cargo test -p runtime -- delegation` |
| PR-6: `sdk-events-control` | 18, 19 | `feat(sdk,types): add streaming policy events and control plane API` | `cargo test -p sdk -p types` |
| PR-7: `sdk-hardening` | 20, 21, 22, 23 | `feat(sdk): add rollout flags, edge case tests, examples, and backward compat suite` | `cargo test --workspace` |

Each PR independently reviewable and releasable. Ship in order but each is self-contained.

---

## Validation & Unit Tests Plan

### Overview
This section defines the comprehensive testing strategy aligned with the Oxydra codebase patterns. Tests follow the existing TDD approach with property-based testing where applicable.

### Test Organization

#### 1. Unit Tests (In-Module)
Location: `src/<module>.rs` with `#[cfg(test)] mod tests`
Pattern: Each public function has corresponding test function

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_function_name_scenario() {
        // Arrange
        let input = ...;
        
        // Act
        let result = function_name(input);
        
        // Assert
        assert_eq!(result, expected);
    }
}
```

#### 2. Integration Tests (`tests/` directory)
Location: `crates/<crate>/tests/<feature>_contracts.rs`
Pattern: Cross-module integration and contract validation

Existing examples:
- `crates/types/tests/core_types_contracts.rs`
- `crates/types/tests/runner_contracts.rs`
- `crates/types/tests/tool_contracts.rs`

#### 3. Contract Tests
Pattern: Verify type serialization/deserialization contracts

```rust
#[test]
fn type_roundtrip_preserves_semantics() {
    let original = create_test_instance();
    let json = serde_json::to_string(&original).unwrap();
    let restored: Type = serde_json::from_str(&json).unwrap();
    assert_eq!(original, restored);
}
```

### Policy System Test Matrix

| Component | Test File | Test Count | Coverage |
|-----------|-----------|------------|----------|
| Policy Types | `types/src/policy.rs` | 28+ | Construction, serde, defaults |
| Policy Merge | `types/src/policy_merge.rs` | 28 | All merge rules, edge cases |
| Permission Handler | `types/src/policy.rs` | 7 | Callback, deny, allow |
| Admission Resolution | `runtime/src/policy_guard.rs` | 11 | Validation, merging |
| Runtime Enforcement | `runtime/src/tests.rs` | 13 | Deadline, budget, bounded overrun |
| Tool Filtering | `tools/src/registry.rs` | 6 | Subset, empty, disallowed |
| Tool Dispatch | `tools/src/registry.rs` | 4 | Blocking, permission checks |
| SDK Client | `sdk/src/client.rs` | 8 | Builder, one_shot, stream |

### Test Commands by Component

```bash
# Policy Types & Merge
cargo test -p types -- policy
cargo test -p types -- policy_merge

# Runtime Enforcement
cargo test -p runtime -- policy
cargo test -p runtime -- policy_guard

# Tool Registry
cargo test -p tools -- registry::filtered
cargo test -p tools -- dispatch_policy

# SDK
cargo test -p sdk -- client
cargo test -p sdk -- events

# Full Suite
cargo test --workspace
```

### Property-Based Testing (Future)

For Wave 4 hardening, add property-based tests:

```rust
// Using proptest or similar
proptest! {
    #[test]
    fn merge_is_idempotent(
        global in arbitrary_runtime_config(),
        agent in arbitrary_agent_definition(),
        per_run in arbitrary_run_policy_input(),
    ) {
        let merged1 = merge_policy(&global, &agent, &per_run, &[]);
        let merged2 = merge_policy(&global, &agent, &per_run, &[]);
        assert_eq!(merged1, merged2);
    }
}
```

### Edge Case Test Categories

1. **Boundary Values**: Zero, max u64, empty strings, empty vectors
2. **None Handling**: All Option fields tested with None
3. **Error Paths**: All error variants triggered and verified
4. **Concurrency**: Thread-safe operations tested with multiple threads
5. **Serialization**: JSON roundtrip for all public types
6. **Default Behavior**: Default impls produce valid minimal instances

### Mock Patterns

Following existing codebase patterns:

```rust
// FakeProvider for deterministic testing
let provider = FakeProvider::new(vec![
    ProviderStep::Complete(Response { ... }),
]);

// Recording wrappers for verification
let recorder = RecordingDelegationExecutor::new(inner);
```

### Test Evidence Collection

Each test run generates evidence:

```bash
# Capture test output
cargo test -p runtime -- policy --nocapture 2>&1 | tee .sisyphus/evidence/task-11-enforcement.txt

# Verify coverage
cargo tarpaulin -p runtime --out Html
```

### Continuous Validation

Pre-commit hooks (recommended):
```bash
#!/bin/sh
# .git/hooks/pre-commit
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test --workspace
```

---

## Success Criteria

### Verification Commands
```bash
cargo fmt --all -- --check                                    # Expected: no diff
cargo clippy --workspace --all-targets -- -D warnings         # Expected: 0 warnings
cargo test --workspace                                        # Expected: all pass, 0 failures
cargo test -p sdk                                             # Expected: SDK tests pass
cargo test -p types -- policy                                 # Expected: policy type tests pass
cargo test -p runtime -- policy                               # Expected: enforcement tests pass
cargo test -p runtime -- delegation::policy                   # Expected: delegation inheritance tests pass
```

### Final Checklist
- [ ] All "Must Have" requirements present and tested
- [ ] All "Must NOT Have" items absent from codebase
- [ ] All 6 policy invariants verified by dedicated tests
- [ ] All 5 risks from Risk Register have mitigation tests
- [ ] All 7 edge case decisions implemented and tested
- [ ] Backward compat: `cargo test --workspace` output identical to baseline (minus new tests)
- [ ] No clippy warnings, no `as any`, no `#[allow(...)]`
- [ ] SDK examples compile and run
