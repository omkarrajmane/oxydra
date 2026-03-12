# Oxydra AI SDK Migration Plan (v2)

## Goal

Convert Oxydra into a first-class AI SDK that wraps the Rust-native agent runtime and exposes strict per-run controls for:

- budget
- max runtime / timeout
- allowed/disallowed tools
- delegation policy inheritance

This plan is additive and avoids large rewrites.

## Repository Strategy

## Decision

Implement SDK inside the existing Oxydra codebase (do not create a separate repo).

Rationale:

- Reuses existing layered crates (`types`, `runtime`, `tools`, `provider`, `memory`) without duplication.
- Keeps policy enforcement in one place.
- Reduces drift between app layer (`runner/gateway/tui`) and SDK behavior.

## Branching and Sync Model

Use fork + upstream rebase workflow:

1. Keep `main` clean.
2. Create feature branch(es), starting with `feat/aisdk-core`.
3. Add canonical Oxydra remote as `upstream`.
4. Rebase feature branches onto `upstream/main` regularly.
5. Ship in small PRs, not one monolithic PR.

Suggested initial command sequence:

```bash
git remote add upstream https://github.com/shantanugoel/oxydra.git
git fetch upstream
git checkout -b feat/aisdk-core upstream/main
```

## Architecture Direction

## New Crate

Add `crates/sdk` as an app-facing facade (orchestration API), depending on existing core crates.

No rewrite of:

- `crates/runtime`
- `crates/tools`
- `crates/provider`
- `crates/memory`

## SDK Surface (v1)

- simple path (one-shot): `query`-style API
- advanced path (stateful): client/session API with streaming + control plane

Control plane capabilities (v1/v1.1):

- interrupt/cancel
- stop background task
- permission mode updates
- model updates (optional phase)
- MCP/tool server enable/disable (optional phase)

## Policy Model (Core Contract)

Add a canonical per-run policy contract in `types` used by SDK + runtime:

- `max_turns`
- `max_cost`
- `turn_timeout`
- `max_wall_time` (new)
- `allowed_tools`
- `disallowed_tools`
- `permission_mode`

Merge rule:

- strictest-wins merge across global config, agent defaults, and per-run overrides.

Enforcement rule:

- enforce at runtime/tool execution choke points (not just schema exposure).

## Delegation and Scheduler Rules

Child/delegated executions must inherit parent policy and may only become stricter.

Required behavior:

- child cannot expand tool scope beyond parent allowed set
- child cannot exceed parent remaining budget/time
- parent cancellation propagates to child tasks

## Phased Delivery Plan

## Phase 0 - Contract Freeze and Baseline

- Finalize API names and policy semantics.
- Define migration notes for existing runner/gateway users.
- Baseline current tests in touched crates.

Exit criteria:

- agreed API contract doc
- no behavior drift in existing runtime paths

## Phase 1 - SDK Facade Skeleton

- Add `crates/sdk`.
- Implement entrypoints for one-shot and streaming runs.
- Wire to existing runtime constructors.

Exit criteria:

- SDK can run a basic turn with current behavior parity
- compile + tests pass

## Phase 2 - Policy Enforcement Pipeline

- Introduce canonical `RunPolicy` + merge layer.
- Enforce tool allow/deny in execution path.
- Add wall-clock session timeout (`max_wall_time`).

Exit criteria:

- forbidden tools blocked in direct and delegated paths
- timeout and budget stop conditions enforced deterministically

## Phase 3 - Session Accounting

- Add per-session ledger for cost/turn/time accounting.
- Ensure scheduler and runtime read from same counters.

Exit criteria:

- multi-turn sessions stop at expected limits
- no double-counting or bypass

## Phase 4 - Delegation Propagation Hardening

- Remove/avoid process-global delegation state for SDK embeddings.
- Ensure runtime-scoped delegation wiring.
- Clamp child constraints to parent remaining limits.

Exit criteria:

- child runs cannot escalate permissions or budgets
- cancellation propagation verified

## Phase 5 - Hooks / Extensions / MCP UX

- Add hook lifecycle points (`PreToolUse`, `PostToolUse`, etc.) where feasible.
- Add in-process extension/tool registration path.
- Keep external MCP compatibility.

Exit criteria:

- hook behavior tested for allow/deny/augment decisions
- extension registration docs and examples complete

## Verification Gates (Every Phase)

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- focused tests for touched modules

Additional SDK-specific tests:

- policy merge matrix tests
- direct vs delegated tool enforcement equivalence
- budget/time exhaustion behavior tests
- control-plane interruption/cancellation tests

## PR Breakdown (Recommended)

1. `sdk-foundation`: crate scaffolding + minimal API + docs
2. `sdk-run-policy`: policy types + merge logic + runtime wiring
3. `sdk-tool-policy-enforcement`: execution-path allow/deny enforcement
4. `sdk-session-ledger`: budget/time/turn accounting
5. `sdk-delegation-policy`: inheritance + cancellation propagation
6. `sdk-hooks-and-extensions`: hooks + in-process extensions + examples

Each PR should be independently reviewable and releasable.

## External Pattern Cues to Adopt

From nanoclaw and Claude Agent SDK style:

- small orchestrator facade over stable core
- explicit policy inputs per run
- clear streaming event framing
- layered tool permission model
- optional advanced control plane for long-running sessions

## Patterns to Avoid

- broad rewrite of existing core crates
- permissive defaults that silently bypass policy
- splitting policy checks across many ad-hoc call sites
- long-lived branch with one giant integration PR

## Immediate Next Step

Start with `Phase 0` by writing a short SDK contract RFC (types + lifecycle + merge semantics), then open PR-1 (`sdk-foundation`).
