# Oxydra SDK Implementation Guide

> **Version**: aisdk-v3  
> **Status**: Production Ready  
> **Last Updated**: 2026-03-12

The Oxydra SDK provides a high-level, type-safe interface for executing AI agent runs with comprehensive policy enforcement.

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Core Concepts](#core-concepts)
3. [Usage Patterns](#usage-patterns)
4. [Policy Enforcement](#policy-enforcement)
5. [Event Streaming](#event-streaming)
6. [Advanced Topics](#advanced-topics)
7. [Examples](#examples)
8. [API Reference](#api-reference)

---

## Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
sdk = { path = "../crates/sdk" }
types = { path = "../crates/types" }
tokio = { version = "1", features = ["full"] }
```

### Basic Usage

```rust
use sdk::{ClientConfig, OxydraClient};
use types::RunPolicyInput;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Configure the client
    let config = ClientConfig::new("user-123")
        .with_agent_name("assistant");
    
    // 2. Build the client (requires runtime components)
    let client = OxydraClient::builder()
        .config(config)
        .runtime(runtime)
        .gateway(gateway)
        .turn_runner(turn_runner)
        .build()?;
    
    // 3. Execute a simple one-shot run
    let result = client
        .one_shot("What is the capital of France?", None)
        .await?;
    
    // 4. Use the result
    println!("Response: {}", result.response);
    println!("Stop reason: {:?}", result.stop_reason);
    
    Ok(())
}
```

---

## Core Concepts

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Your Application                      │
│  ┌──────────────────────────────────────────────────────┐   │
│  │                  OxydraClient                       │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────┐ │   │
│  │  │   Config     │  │   Runtime    │  │  Gateway   │ │   │
│  │  │  (policy)    │  │  (execution) │  │ (sessions)│ │   │
│  │  └──────────────┘  └──────────────┘  └────────────┘ │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Oxydra Runtime                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Policy    │  │   Budget    │  │     Delegation      │  │
│  │   Guard     │  │   Ledger    │  │      Executor       │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Key Components

| Component | Purpose | Location |
|-----------|---------|----------|
| `OxydraClient` | Main entry point for SDK usage | `sdk::client` |
| `ClientConfig` | User, agent, and policy configuration | `sdk::policy` |
| `RunPolicyInput` | Budget, turn, and runtime constraints | `types::policy` |
| `ToolPolicyInput` | Tool allow/block lists | `types::policy` |
| `RunResult` | Final result of a run | `sdk::events` |
| `RunEvent` | Streaming event enum | `sdk::events` |

---

## Usage Patterns

### Pattern 1: One-Shot Execution

For simple, single-turn interactions without streaming:

```rust
use sdk::{ClientConfig, OxydraClient};

let config = ClientConfig::new("user-123")
    .with_agent_name("assistant");

let client = OxydraClient::builder()
    .config(config)
    .runtime(runtime)
    .gateway(gateway)
    .turn_runner(turn_runner)
    .build()?;

// Execute without policy constraints
let result = client.one_shot("Hello!", None).await?;

// Execute with policy constraints
let policy = RunPolicyInput {
    max_turns: Some(5),
    max_budget_microusd: Some(100_000), // $0.10
    ..Default::default()
};
let result = client.one_shot("Hello!", Some(policy)).await?;
```

### Pattern 2: Streaming Execution

For real-time responses with progress tracking:

```rust
use sdk::{ClientConfig, OxydraClient, RunEvent};
use futures_util::StreamExt;

let client = /* ... build client ... */;

// Start streaming
let mut stream = client.stream("Your prompt", None).await?;
let mut full_response = String::new();

while let Some(event) = stream.next().await {
    match event {
        RunEvent::Text(text) => {
            print!("{}", text);
            full_response.push_str(&text);
        }
        RunEvent::ToolCall(tool) => {
            println!("\n[Tool: {}]", tool.name);
        }
        RunEvent::BudgetUpdate { cost_microusd, .. } => {
            println!("\n[Cost: ${:.4}]", cost_microusd as f64 / 1_000_000.0);
        }
        RunEvent::Completed(result) => {
            println!("\n[Done: {:?}]", result.stop_reason);
            break;
        }
        _ => {}
    }
}
```

### Pattern 3: Session Control

For managing active sessions:

```rust
use sdk::OxydraClient;

let client = /* ... build client ... */;
let session_id = "session-abc-123";

// Cancel an active turn
client.cancel(session_id).await?;

// Get session status
let status = client.get_status(session_id).await?;
println!("Turn: {}", status.turn);
println!("Budget remaining: {:?}", status.budget_remaining);
println!("Active: {}", status.is_active);
```

---

## Policy Enforcement

### Policy Types

#### RunPolicyInput

Controls budget, turns, and runtime:

```rust
use std::time::Duration;
use types::RunPolicyInput;

let policy = RunPolicyInput {
    // Maximum budget in micro-USD ($1.00 = 1_000_000)
    max_budget_microusd: Some(500_000), // $0.50
    
    // Maximum number of turns (back-and-forth exchanges)
    max_turns: Some(10),
    
    // Maximum wall-clock runtime
    max_runtime: Some(Duration::from_secs(300)), // 5 minutes
    
    // Tool-specific policy
    tool_policy: Some(tool_policy),
};
```

#### ToolPolicyInput

Controls tool access:

```rust
use types::ToolPolicyInput;
use std::collections::HashSet;

let tool_policy = ToolPolicyInput {
    // Allowed tools (None = all tools allowed)
    toolset: Some(vec![
        "web_search".to_string(),
        "read_file".to_string(),
    ]),
    
    // Auto-approved tools (no user confirmation needed)
    auto_approve_tools: Some(HashSet::from([
        "web_search".to_string(),
    ])),
    
    // Blocked tools (always denied, takes precedence)
    disallowed_tools: Some(HashSet::from([
        "shell".to_string(),
        "exec".to_string(),
    ])),
};
```

### Policy Resolution

Policies are resolved at **admission time** using **strictest-wins** semantics:

```
Global Config (RuntimeConfig)
        ↓
Agent Definition (AgentDefinition)
        ↓
Per-Run Policy (RunPolicyInput)
        ↓
EffectiveRunPolicy (resolved)
```

**Strictest-Wins Rules**:
- **Budget**: `min(global, agent, per_run)`
- **Max Turns**: `min(global, agent, per_run)`
- **Tools**: Intersection of all allowlists
- **Disallowed**: Union of all blocked tools (always wins)
- **Deadline**: Earliest of all deadlines

### Rollout Modes

Control how policy violations are handled:

```rust
use types::RolloutMode;

let policy = RunPolicyInput {
    rollout_mode: Some(RolloutMode::Enforce),      // Default: block on violation
    // rollout_mode: Some(RolloutMode::SoftFail),  // Log warning, continue
    // rollout_mode: Some(RolloutMode::ObserveOnly), // Log only, never block
    ..Default::default()
};
```

| Mode | Behavior | Use Case |
|------|----------|----------|
| `Enforce` | Block/stop on violation | Production |
| `SoftFail` | Log warning, emit event, continue | Gradual rollout |
| `ObserveOnly` | Log evaluation only | Monitoring |

---

## Event Streaming

### RunEvent Variants

| Event | Description | Fields |
|-------|-------------|--------|
| `Text(String)` | Text delta from assistant | `text` |
| `ToolCall(ToolCall)` | Tool invocation request | `name`, `arguments`, `call_id` |
| `ToolResult { ... }` | Tool execution result | `call_id`, `content`, `success` |
| `UsageUpdate(UsageUpdate)` | Token usage stats | `tokens_used`, `cost_microusd` |
| `BudgetUpdate { ... }` | Budget consumption | `tokens_used`, `cost_microusd`, `remaining_budget` |
| `BudgetWarning { ... }` | Budget threshold alert | `remaining`, `threshold_pct` |
| `PolicyStop { ... }` | Policy termination | `reason`, `stop_reason` |
| `Completed(RunResult)` | Run finished | Full result |

### Streaming Example with Full Handling

```rust
use sdk::{RunEvent, RunResult};
use futures_util::StreamExt;

async fn handle_stream(mut stream: RunEventStream) -> Result<RunResult, ClientError> {
    let mut response = String::new();
    let mut tool_calls = Vec::new();
    
    while let Some(event) = stream.next().await {
        match event {
            RunEvent::Text(text) => {
                response.push_str(&text);
            }
            
            RunEvent::ToolCall(tool) => {
                println!("Tool called: {}", tool.name);
                tool_calls.push(tool);
            }
            
            RunEvent::ToolResult { call_id, content, success } => {
                if success {
                    println!("Tool {} succeeded", call_id);
                } else {
                    println!("Tool {} failed: {}", call_id, content);
                }
            }
            
            RunEvent::BudgetUpdate { cost_microusd, remaining_budget, .. } => {
                let cost_dollars = cost_microusd as f64 / 1_000_000.0;
                let remaining_dollars = remaining_budget as f64 / 1_000_000.0;
                println!("Cost: ${:.4}, Remaining: ${:.4}", cost_dollars, remaining_dollars);
            }
            
            RunEvent::BudgetWarning { remaining, threshold_pct } => {
                println!("WARNING: Budget at {}% ({} remaining)", threshold_pct, remaining);
            }
            
            RunEvent::PolicyStop { reason, stop_reason } => {
                println!("Stopped: {} ({:?})", reason, stop_reason);
                return Err(ClientError::PolicyViolation(reason));
            }
            
            RunEvent::Completed(result) => {
                return Ok(result);
            }
            
            _ => {}
        }
    }
    
    Err(ClientError::Stream("Stream ended unexpectedly".to_string()))
}
```

---

## Advanced Topics

### Delegation and Policy Inheritance

When agents delegate to subagents, policies are **narrowed** (never expanded):

```rust
// Parent session
let parent_policy = RunPolicyInput {
    max_budget_microusd: Some(1_000_000), // $1.00
    max_turns: Some(20),
    tool_policy: Some(ToolPolicyInput {
        toolset: Some(vec!["web_search".to_string(), "read_file".to_string()]),
        disallowed_tools: Some(HashSet::from(["shell".to_string()])),
        ..Default::default()
    }),
    ..Default::default()
};

// Child session (delegated)
let child_policy = RunPolicyInput {
    max_budget_microusd: Some(500_000), // $0.50
    max_turns: Some(10),
    tool_policy: Some(ToolPolicyInput {
        toolset: Some(vec!["web_search".to_string()]), // Subset of parent
        ..Default::default()
    }),
    ..Default::default()
};

// Resolved child policy:
// - Budget: min(1M, 500K) = 500K
// - Turns: min(20, 10) = 10
// - Tools: intersection([web, read], [web]) = [web]
// - Disallowed: union([shell], []) = [shell]
```

**Delegation Depth Limit**: Maximum 5 levels (enforced at admission)

**Session ID Format**:
- Root: `uuid`
- Level 1: `subagent:parent_uuid:uuid`
- Level 2: `subagent:parent_uuid:subagent:grandparent_uuid:uuid`

### Budget Ledger

The budget ledger tracks spending atomically:

```rust
use runtime::BudgetLedger;

// Create ledger with initial budget
let ledger = BudgetLedger::new(1_000_000); // $1.00

// Reserve before provider call
let reservation = ledger.reserve(estimated_cost)?;

// ... make provider call ...

// Settle with actual cost
ledger.settle(reservation, actual_cost);

// Check remaining
let remaining = ledger.remaining();
```

**Features**:
- Thread-safe (AtomicU64)
- Parent/child hierarchy
- Bounded overrun semantics (completes current call, stops before next)
- Budget warnings at 80% and 95%

### Error Handling

```rust
use sdk::ClientError;

match result {
    Err(ClientError::Runtime(e)) => println!("Runtime error: {}", e),
    Err(ClientError::Gateway(e)) => println!("Gateway error: {}", e),
    Err(ClientError::Session(e)) => println!("Session error: {}", e),
    Err(ClientError::Stream(e)) => println!("Stream error: {}", e),
    Err(ClientError::Cancelled) => println!("Cancelled by user"),
    Err(ClientError::PolicyViolation(reason)) => println!("Policy: {}", reason),
    Ok(result) => println!("Success: {}", result.response),
}
```

---

## Examples

### Example 1: Simple Chatbot

```rust
use sdk::{ClientConfig, OxydraClient};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ClientConfig::new("user-123")
        .with_agent_name("chatbot");
    
    let client = OxydraClient::builder()
        .config(config)
        .runtime(runtime)
        .gateway(gateway)
        .turn_runner(turn_runner)
        .build()?;
    
    loop {
        print!("You: ");
        std::io::stdout().flush()?;
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        if input.trim() == "quit" {
            break;
        }
        
        let result = client.one_shot(&input, None).await?;
        println!("Bot: {}", result.response);
    }
    
    Ok(())
}
```

### Example 2: Budget-Constrained Agent

```rust
use sdk::{ClientConfig, OxydraClient};
use types::{RunPolicyInput, ToolPolicyInput};
use std::collections::HashSet;

let policy = RunPolicyInput {
    max_budget_microusd: Some(100_000), // $0.10 per run
    max_turns: Some(5),
    tool_policy: Some(ToolPolicyInput {
        toolset: Some(vec!["web_search".to_string()]),
        disallowed_tools: Some(HashSet::from(["shell".to_string()])),
        ..Default::default()
    }),
    ..Default::default()
};

let config = ClientConfig::new("user-123")
    .with_agent_name("safe-search")
    .with_policy(policy);

let client = /* ... */;
let result = client.one_shot("Search for Rust tutorials", None).await?;
```

### Example 3: Real-Time Monitoring

```rust
use sdk::{ClientConfig, OxydraClient, RunEvent};
use futures_util::StreamExt;

let client = /* ... */;
let mut stream = client.stream("Analyze this data", None).await?;

let (tx, mut rx) = tokio::sync::mpsc::channel(100);

// Spawn monitoring task
let monitor = tokio::spawn(async move {
    while let Some(event) = rx.recv().await {
        match event {
            RunEvent::BudgetUpdate { cost_microusd, .. } => {
                // Send to monitoring dashboard
                metrics::gauge!("run_cost", cost_microusd as f64 / 1_000_000.0);
            }
            RunEvent::PolicyStop { reason, .. } => {
                // Alert on policy violation
                tracing::warn!("Policy stop: {}", reason);
            }
            _ => {}
        }
    }
});

// Process stream
while let Some(event) = stream.next().await {
    let _ = tx.send(event.clone()).await;
    // ... handle event ...
}

drop(tx);
monitor.await?;
```

---

## API Reference

### OxydraClient

```rust
impl OxydraClient {
    /// Create a builder for constructing the client
    pub fn builder() -> ClientBuilder;
    
    /// Execute a single-turn run
    pub async fn one_shot(
        &self,
        prompt: impl Into<String>,
        policy: Option<RunPolicyInput>,
    ) -> Result<RunResult, ClientError>;
    
    /// Execute a streaming run
    pub async fn stream(
        &self,
        prompt: impl Into<String>,
        policy: Option<RunPolicyInput>,
    ) -> Result<RunEventStream, ClientError>;
    
    /// Cancel an active turn
    pub async fn cancel(&self, session_id: &str) -> Result<(), ClientError>;
    
    /// Get session status
    pub async fn get_status(
        &self,
        session_id: &str,
    ) -> Result<SessionStatus, ClientError>;
}
```

### ClientConfig

```rust
impl ClientConfig {
    /// Create new config with required user_id
    pub fn new(user_id: impl Into<String>) -> Self;
    
    /// Set agent name
    pub fn with_agent_name(mut self, name: impl Into<String>) -> Self;
    
    /// Set session ID (auto-generated if not set)
    pub fn with_session_id(mut self, id: impl Into<String>) -> Self;
    
    /// Set policy constraints
    pub fn with_policy(mut self, policy: RunPolicyInput) -> Self;
}
```

### Types

```rust
// Result of a completed run
pub struct RunResult {
    pub response: String,
    pub stop_reason: StopReason,
    pub usage: Option<UsageUpdate>,
    pub tool_calls: Vec<ToolCall>,
}

// Session status
pub struct SessionStatus {
    pub turn: usize,
    pub budget_remaining: Option<u64>,
    pub is_active: bool,
    pub stop_reason: Option<StopReason>,
}

// Stop reasons
#[non_exhaustive]
pub enum StopReason {
    Completed,
    MaxTurnsReached,
    BudgetExceeded,
    DeadlineReached,
    ToolUnavailable,
    Cancelled,
    Error(String),
}
```

---

## Troubleshooting

### Common Issues

**Issue**: `ClientError::Runtime("Policy resolution failed")`

**Solution**: Check that your policy constraints are valid (e.g., max_turns > 0, budget > 0).

---

**Issue**: `ClientError::Stream("Stream ended unexpectedly")`

**Solution**: Ensure you're handling all RunEvent variants, especially `Completed`.

---

**Issue**: Budget warnings not firing

**Solution**: Budget warnings are only emitted during streaming. Use `client.stream()` instead of `one_shot()`.

---

**Issue**: Tools being blocked unexpectedly

**Solution**: Check `disallowed_tools` in your policy - it takes precedence over `toolset`.

---

## Additional Resources

- **Examples**: `crates/sdk/examples/`
- **Integration Tests**: `crates/sdk/src/client.rs` (test module)
- **Policy Types**: `crates/types/src/policy.rs`
- **Known Issues**: `.sisyphus/KNOWN_ISSUES.md`

---

## License

This SDK is part of the Oxydra project. See the main repository for license information.
