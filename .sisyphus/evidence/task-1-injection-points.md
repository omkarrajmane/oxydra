# Runtime Injection Points Analysis

**Task:** Verify runtime injection points at all 8 enforcement locations  
**Date:** 2026-03-12  
**Scope:** Read-only analysis of policy check insertion seams

---

## Executive Summary

All 8 enforcement locations have viable injection seams. Risk assessment:
- **Low Risk:** 5 points (simple inline additions or existing parameter patterns)
- **Medium Risk:** 2 points (signature changes needed but localized)
- **High Risk:** 1 point (requires trait modification)

---

## 1. ADMISSION CONTROL (Session Creation)

**Location:** `crates/gateway/src/lib.rs`

### 1.1 Current Code Seam

**File:** `crates/gateway/src/lib.rs:254`  
**Function:** `create_or_get_session()`

```rust
self.ensure_session_capacity(user_id, &user).await?;

// Create a new session.
let new_session_id = uuid::Uuid::now_v7().to_string();
```

**File:** `crates/gateway/src/lib.rs:266-271`

```rust
if sessions.len() >= self.max_sessions_per_user {
    return Err(format!(
        "maximum sessions reached (limit: {})",
        self.max_sessions_per_user
    ));
}
```

### 1.2 Injection Method

**Method:** Inline check with parameter addition

The `ensure_session_capacity()` call at line 254 is the ideal insertion point. The function already exists and performs capacity checks.

**Recommended approach:**
1. Add `policy_checker: Arc<dyn PolicyChecker>` parameter to `GatewayServer` struct
2. Insert policy validation call after line 254:
   ```rust
   self.ensure_session_capacity(user_id, &user).await?;
   self.policy_checker.validate_session_creation(user_id, agent_name).await?;
   ```

### 1.3 Risk Assessment

**Risk Level:** LOW

- Simple inline addition after existing capacity check
- No signature changes required for public APIs
- Policy checker can be added as a field to `GatewayServer`

---

## 2. TURN-LOOP (Runtime Loop)

**Location:** `crates/runtime/src/lib.rs`

### 2.1 Current Code Seam

**File:** `crates/runtime/src/lib.rs:363-620`  
**Function:** `run_session_internal()`

**Entry point (line 427):**
```rust
self.validate_guard_preconditions()?;
let mut turn = 0usize;
let mut accumulated_cost = 0.0;
```

**Turn iteration check (lines 431-440):**
```rust
loop {
    if cancellation.is_cancelled() {
        let state = TurnState::Cancelled;
        tracing::debug!(?state, "run_session cancelled before provider call");
        return Err(RuntimeError::Cancelled);
    }
    if turn >= self.limits.max_turns {
        return Err(RuntimeError::BudgetExceeded);
    }
    turn += 1;
    // ... turn execution
}
```

### 2.2 Injection Method

**Method:** Parameter addition to `AgentRuntime` + inline checks

The `AgentRuntime` struct (line 134-145) already holds `limits: RuntimeLimits`. A policy checker can be added as an optional field:

```rust
pub struct AgentRuntime {
    // ... existing fields
    limits: RuntimeLimits,
    policy_checker: Option<Arc<dyn PolicyChecker>>,  // ADDED
}
```

**Injection points:**
1. **Line 427** (after `validate_guard_preconditions`): Add pre-session policy check
2. **Line 440** (after `turn += 1`): Add per-turn policy check

### 2.3 Risk Assessment

**Risk Level:** LOW

- `AgentRuntime` is constructed via `AgentRuntime::new()` (line 148-166)
- Adding an optional field with a builder-style method (`with_policy_checker`) is non-breaking
- All injection points are internal to the runtime loop

---

## 3. PROVIDER-CALL (Timeout Enforcement)

**Location:** `crates/runtime/src/provider_response.rs`

### 3.1 Current Code Seam

**File:** `crates/runtime/src/provider_response.rs:4-51`  
**Function:** `request_provider_response()`

```rust
pub(crate) async fn request_provider_response(
    &self,
    context: &Context,
    cancellation: &CancellationToken,
    stream_events: Option<RuntimeStreamEventSender>,
) -> Result<Response, RuntimeError> {
    let provider = self.provider_for_context(context)?;
    let caps = provider.capabilities(&context.model)?;
    // ... streaming or complete response logic
}
```

**Timeout enforcement location (lines 26-47):**
```rust
if caps.supports_streaming && caps.supports_tools {
    match self
        .stream_response(
            provider,
            &request_context,
            cancellation,
            stream_events.clone(),
        )
        .await
    {
        Ok(response) => return Ok(response),
        Err(StreamCollectError::Cancelled) => return Err(RuntimeError::Cancelled),
        Err(StreamCollectError::TurnTimedOut) => {
            return Err(Self::provider_timeout_error(
                request_context.provider.clone(),
                self.limits.turn_timeout,
            ));
        }
        // ...
    }
}
```

### 3.2 Injection Method

**Method:** Inline check via `AgentRuntime` field

The function is `pub(crate)` and called from `run_session_internal` (line 459-461). The `self` reference provides access to any policy checker stored in `AgentRuntime`.

**Recommended injection at line 10** (after provider resolution):
```rust
let provider = self.provider_for_context(context)?;
let caps = provider.capabilities(&context.model)?;

// Policy check insertion
if let Some(ref policy) = self.policy_checker {
    policy.validate_provider_call(context, &caps).await?;
}
```

### 3.3 Risk Assessment

**Risk Level:** LOW

- Function is `pub(crate)` - internal visibility
- No signature changes needed
- Access to `self` provides policy checker reference

---

## 4. TOOL-SCHEMA (Schema Validation)

**Location:** `crates/tools/src/registry.rs` + `crates/runtime/src/tool_execution.rs`

### 4.1 Current Code Seam

**File:** `crates/tools/src/registry.rs:105-141`  
**Function:** `execute_with_policy_and_context()`

```rust
pub async fn execute_with_policy_and_context<F>(
    &self,
    name: &str,
    args: &str,
    mut safety_gate: F,
    context: &ToolExecutionContext,
) -> Result<String, ToolError>
where
    F: FnMut(SafetyTier) -> Result<(), ToolError>,
{
    let tool = self
        .get(name)
        .ok_or_else(|| execution_failed(name, format!("unknown tool `{name}`")))?;

    safety_gate(tool.safety_tier())?;
    if let Some(policy) = &self.security_policy {
        let arguments = parse_policy_args(name, args)?;
        policy
            .enforce(name, tool.safety_tier(), &arguments)
            .map_err(|violation| { ... })?;
    }
    // ... execution
}
```

**File:** `crates/runtime/src/tool_execution.rs:58-129`  
**Function:** `execute_tool_call()`

```rust
pub(crate) async fn execute_tool_call(
    &self,
    name: &str,
    arguments: &serde_json::Value,
    cancellation: &CancellationToken,
    tool_context: &ToolExecutionContext,
) -> Result<String, RuntimeError> {
    let tool = self.tool_registry.get(name).ok_or_else(|| { ... })?;
    
    // Schema validation (lines 79-102)
    let schema_decl = tool.schema();
    let mut schema_json = schema_decl.parameters.clone();
    strip_additional_properties(&mut schema_json);
    // ... validation logic
}
```

### 4.2 Injection Method

**Method:** Wrapper function + trait extension

The `ToolRegistry` already has a `security_policy: Option<Arc<dyn SecurityPolicy>>` field (line 9). This pattern can be extended:

1. **Option A (Recommended):** Extend existing `SecurityPolicy` trait with policy check method
2. **Option B:** Add separate `policy_checker` field to `ToolRegistry`

**Injection point in `execute_tool_call` (line 65-70):**
```rust
let tool = self.tool_registry.get(name).ok_or_else(|| { ... })?;

// Policy check insertion
if let Some(ref policy) = self.policy_checker {
    policy.validate_tool_schema(name, &tool.schema()).await?;
}
```

### 4.3 Risk Assessment

**Risk Level:** MEDIUM

- `ToolRegistry` already has policy infrastructure (`security_policy`)
- However, `execute_tool_call` is in `AgentRuntime` impl, not `ToolRegistry`
- Requires adding policy checker to `AgentRuntime` (same as turn-loop)

---

## 5. TOOL-DISPATCH (Tool Execution)

**Location:** `crates/runtime/src/tool_execution.rs`

### 5.1 Current Code Seam

**File:** `crates/runtime/src/tool_execution.rs:111-128`  
**Function:** `execute_tool_call()` - execution section

```rust
tokio::select! {
    _ = cancellation.cancelled() => Err(RuntimeError::Cancelled),
    timed = tokio::time::timeout(
        self.limits.turn_timeout, 
        self.tool_registry.execute_with_context(name, &arg_str, tool_context)
    ) => match timed {
        Ok(output) => output.map_err(RuntimeError::from),
        Err(_) => {
            let timeout = self.limits.turn_timeout;
            tracing::warn!(...);
            Err(RuntimeError::Tool(ToolError::ExecutionFailed { ... }))
        }
    }
}
```

**Batch dispatch in `run_session_internal` (lines 556-613):**
```rust
for tool_call in &tool_calls {
    let tier = self
        .tool_registry
        .get(&tool_call.name)
        .map(|t| t.safety_tier());

    if tier == Some(SafetyTier::ReadOnly) {
        current_batch.push(tool_call);
    } else {
        // Sequential execution for non-readonly
        let result = self
            .execute_tool_and_format(tool_call, cancellation, &tool_context)
            .await?;
        // ...
    }
}
```

### 5.2 Injection Method

**Method:** Inline check in dispatch loop

**Injection point at line 557-561** (before tier check):
```rust
for tool_call in &tool_calls {
    // Policy check insertion
    if let Some(ref policy) = self.policy_checker {
        policy.validate_tool_dispatch(&tool_call.name, &tool_call.arguments).await?;
    }
    
    let tier = self
        .tool_registry
        .get(&tool_call.name)
        .map(|t| t.safety_tier());
    // ...
}
```

### 5.3 Risk Assessment

**Risk Level:** LOW

- Simple inline addition in existing loop
- Access to `self` provides policy checker reference
- No signature changes needed

---

## 6. DELEGATION (Subagent Dispatch)

**Location:** `crates/runtime/src/delegation.rs`

### 6.1 Current Code Seam

**File:** `crates/runtime/src/delegation.rs:62-178`  
**Function:** `delegate()` (impl of `DelegationExecutor` trait)

```rust
#[async_trait]
impl types::DelegationExecutor for RuntimeDelegationExecutor {
    async fn delegate(
        &self,
        request: DelegationRequest,
        parent_cancellation: &CancellationToken,
        _progress_sender: Option<DelegationProgressSender>,
    ) -> Result<DelegationResult, RuntimeError> {
        // Lookup the agent definition
        let agent_def = match self.agents.get(&request.agent_name) {
            Some(def) => def,
            None => { ... }
        };

        // Build a fresh context for the subagent run
        let effective_selection = resolve_delegation_selection(...);
        let mut ctx = Context { ... };
        
        // ... context setup
        
        // Run the session (lines 157-165)
        let response = self
            .runtime
            .run_session_for_session_with_tool_context(
                &subagent_session_id,
                &mut ctx,
                parent_cancellation,
                &tool_context,
            )
            .await;
        // ...
    }
}
```

### 6.2 Injection Method

**Method:** Parameter addition via `DelegationRequest` or wrapper

**Option A (Recommended):** Add policy check before subagent execution (line 157):
```rust
// Policy check insertion
if let Some(ref policy) = self.policy_checker {
    policy.validate_delegation(&request).await?;
}

let response = self
    .runtime
    .run_session_for_session_with_tool_context(...)
    .await;
```

**Option B:** Extend `DelegationRequest` struct to include policy context

### 6.3 Risk Assessment

**Risk Level:** MEDIUM

- `RuntimeDelegationExecutor` would need a `policy_checker` field added
- Constructor `new()` (line 32-42) would need an additional parameter
- This is a breaking change for `RuntimeDelegationExecutor::new()`

---

## 7. SCHEDULER (Scheduled Task Execution)

**Location:** `crates/runtime/src/scheduler_executor.rs`

### 7.1 Current Code Seam

**File:** `crates/runtime/src/scheduler_executor.rs:90-201`  
**Function:** `execute_schedule()`

```rust
async fn execute_schedule(&self, schedule: ScheduleDefinition) {
    let run_id = uuid::Uuid::new_v4().to_string();
    let session_id = format!("scheduled:{}", schedule.schedule_id);
    let started_at = Utc::now().to_rfc3339();

    let prompt = if schedule.notification_policy == NotificationPolicy::Conditional {
        format!("{}\n\n---\nYou are executing a scheduled task...", schedule.goal)
    } else {
        schedule.goal.clone()
    };

    let child_cancellation = self.cancellation.child_token();

    let result = self
        .turn_runner
        .run_scheduled_turn(&schedule.user_id, &session_id, prompt, child_cancellation)
        .await;
    // ... result handling
}
```

**File:** `crates/runtime/src/scheduler_executor.rs:66-88`  
**Function:** `tick()`

```rust
pub(crate) async fn tick(&self) {
    let now = Utc::now().to_rfc3339();
    let due = match self
        .store
        .due_schedules(&now, self.config.max_concurrent)
        .await
    {
        Ok(due) => due,
        Err(e) => { ... }
    };

    if due.is_empty() {
        return;
    }

    tracing::debug!("scheduler: {} due schedule(s)", due.len());

    let futs: Vec<_> = due.into_iter().map(|s| self.execute_schedule(s)).collect();
    futures::future::join_all(futs).await;
}
```

### 7.2 Injection Method

**Method:** Parameter addition to `SchedulerExecutor`

The `SchedulerExecutor` struct (lines 23-29):
```rust
pub struct SchedulerExecutor {
    store: Arc<dyn SchedulerStore>,
    turn_runner: Arc<dyn ScheduledTurnRunner>,
    notifier: Arc<dyn SchedulerNotifier>,
    config: SchedulerConfig,
    cancellation: CancellationToken,
    policy_checker: Option<Arc<dyn PolicyChecker>>,  // ADDED
}
```

**Injection point at line 108-111** (before `run_scheduled_turn`):
```rust
// Policy check insertion
if let Some(ref policy) = self.policy_checker {
    policy.validate_scheduled_execution(&schedule).await?;
}

let result = self
    .turn_runner
    .run_scheduled_turn(&schedule.user_id, &session_id, prompt, child_cancellation)
    .await;
```

### 7.3 Risk Assessment

**Risk Level:** MEDIUM

- `SchedulerExecutor::new()` (lines 32-46) would need an additional parameter
- This is a breaking change for the constructor
- However, the scheduler is typically constructed in a single location (bootstrap)

---

## 8. RESUME (Session Restoration)

**Location:** `crates/gateway/src/lib.rs` + `crates/runtime/src/lib.rs`

### 8.1 Current Code Seam

**File:** `crates/gateway/src/lib.rs:230-248`  
**Function:** `create_or_get_session()` - resume path

```rust
// Try resuming from the session store.
if let Some(store) = &self.session_store
    && let Ok(Some(record)) = store.get_session(id).await
{
    let session = Arc::new(SessionState::new(
        record.session_id.clone(),
        record.user_id.clone(),
        record.agent_name.clone(),
        record.parent_session_id.clone(),
        record.channel_origin.clone(),
    ));
    let mut sessions = user.sessions.write().await;
    sessions.insert(record.session_id.clone(), Arc::clone(&session));
    tracing::info!(...);
    return Ok(session);
}
```

**File:** `crates/runtime/src/lib.rs:331-361`  
**Function:** `restore_session()`

```rust
pub async fn restore_session(
    &self,
    session_id: &str,
    context: &mut Context,
    limit: Option<u64>,
) -> Result<(), RuntimeError> {
    let Some(memory) = &self.memory else {
        return Ok(());
    };

    let restored = match memory
        .recall(MemoryRecallRequest {
            session_id: session_id.to_owned(),
            limit,
        })
        .await
    {
        Ok(records) => records,
        Err(MemoryError::NotFound { .. }) => return Ok(()),
        Err(error) => return Err(RuntimeError::from(error)),
    };

    let restored_messages = restored
        .into_iter()
        .map(|record| serde_json::from_value::<Message>(record.payload))
        .collect::<Result<Vec<_>, _>>()
        .map_err(...)?;
    context.messages = restored_messages;
    Ok(())
}
```

### 8.2 Injection Method

**Method:** Inline check in gateway + trait extension for runtime

**Gateway injection at line 232** (after store lookup, before session creation):
```rust
if let Some(store) = &self.session_store
    && let Ok(Some(record)) = store.get_session(id).await
{
    // Policy check insertion
    if let Some(ref policy) = self.policy_checker {
        policy.validate_session_resume(&record).await?;
    }
    
    let session = Arc::new(SessionState::new(...));
    // ...
}
```

**Runtime injection at line 337** (after memory check, before recall):
```rust
pub async fn restore_session(
    &self,
    session_id: &str,
    context: &mut Context,
    limit: Option<u64>,
) -> Result<(), RuntimeError> {
    // Policy check insertion
    if let Some(ref policy) = self.policy_checker {
        policy.validate_session_restore(session_id).await?;
    }
    
    let Some(memory) = &self.memory else {
        return Ok(());
    };
    // ...
}
```

### 8.3 Risk Assessment

**Risk Level:** LOW

- Gateway resume: Simple inline check, `GatewayServer` already holds policy checker
- Runtime restore: Requires `policy_checker` field in `AgentRuntime` (same as turn-loop)
- No signature changes needed for public APIs

---

## Summary Table

| # | Enforcement Point | File:Line | Function | Injection Method | Risk Level |
|---|-------------------|-----------|----------|------------------|------------|
| 1 | Admission | `gateway/src/lib.rs:254` | `create_or_get_session()` | Inline + field | LOW |
| 2 | Turn-Loop | `runtime/src/lib.rs:427,440` | `run_session_internal()` | Inline + field | LOW |
| 3 | Provider-Call | `runtime/src/provider_response.rs:10` | `request_provider_response()` | Inline via self | LOW |
| 4 | Tool-Schema | `runtime/src/tool_execution.rs:65` | `execute_tool_call()` | Inline + field | MEDIUM |
| 5 | Tool-Dispatch | `runtime/src/lib.rs:557` | `run_session_internal()` loop | Inline via self | LOW |
| 6 | Delegation | `runtime/src/delegation.rs:157` | `delegate()` | Inline + ctor param | MEDIUM |
| 7 | Scheduler | `runtime/src/scheduler_executor.rs:108` | `execute_schedule()` | Inline + ctor param | MEDIUM |
| 8 | Resume | `gateway/src/lib.rs:232`, `runtime/src/lib.rs:337` | `create_or_get_session()`, `restore_session()` | Inline + field | LOW |

---

## High-Risk Points (None)

No high-risk points identified. All injection seams are viable with straightforward approaches.

---

## Recommended Implementation Order

1. **Phase 1 (Foundation):** Add `policy_checker: Option<Arc<dyn PolicyChecker>>` to `AgentRuntime`
2. **Phase 2 (Core Runtime):** Implement turn-loop, provider-call, tool-dispatch, tool-schema checks
3. **Phase 3 (Gateway):** Implement admission and resume checks
4. **Phase 4 (Advanced):** Implement delegation and scheduler checks (require constructor changes)

---

## PolicyChecker Trait Sketch

```rust
#[async_trait]
pub trait PolicyChecker: Send + Sync {
    async fn validate_session_creation(&self, user_id: &str, agent_name: &str) -> Result<(), PolicyError>;
    async fn validate_session_resume(&self, record: &SessionRecord) -> Result<(), PolicyError>;
    async fn validate_session_restore(&self, session_id: &str) -> Result<(), PolicyError>;
    async fn validate_turn_start(&self, turn: usize, context: &Context) -> Result<(), PolicyError>;
    async fn validate_provider_call(&self, context: &Context, caps: &ProviderCaps) -> Result<(), PolicyError>;
    async fn validate_tool_schema(&self, tool_name: &str, schema: &FunctionDecl) -> Result<(), PolicyError>;
    async fn validate_tool_dispatch(&self, tool_name: &str, arguments: &Value) -> Result<(), PolicyError>;
    async fn validate_delegation(&self, request: &DelegationRequest) -> Result<(), PolicyError>;
    async fn validate_scheduled_execution(&self, schedule: &ScheduleDefinition) -> Result<(), PolicyError>;
}
```

---

## Notes

- All enforcement points have access to `self` (either `AgentRuntime`, `GatewayServer`, or `SchedulerExecutor`)
- The pattern of adding an `Option<Arc<dyn PolicyChecker>>` field is consistent across all structs
- No points require complex refactoring or function rewrites
- The `SecurityPolicy` trait already exists in the tools crate and could be extended or used as a model
