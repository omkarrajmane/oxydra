//! Example: Policy Enforcement Demo
//!
//! This example demonstrates how to use runtime policies to enforce:
//! - Budget limits (max cost in micro-USD)
//! - Tool restrictions (allowed/disallowed tools)
//! - Maximum turns and runtime
//!
//! # Usage
//!
//! ```bash
//! cargo run --example policy_enforcement -p sdk
//! ```

use std::time::Duration;

use sdk::ClientConfig;
use types::{RunPolicyInput, ToolPolicyInput};

/// Demonstrates policy configuration for budget and tool enforcement.
///
/// Policies allow you to set constraints on agent execution:
/// - Budget limits prevent runaway costs
/// - Tool policies restrict which tools can be used
/// - Turn/runtime limits prevent infinite loops
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Policy Enforcement Example ===\n");

    // Example 1: Budget-constrained policy
    // This policy limits the run to $0.50 (500,000 micro-USD)
    println!("Example 1: Budget Policy");
    println!("--------------------------");

    let budget_policy = RunPolicyInput {
        max_budget_microusd: Some(500_000), // $0.50
        max_turns: Some(10),
        max_runtime: Some(Duration::from_secs(300)), // 5 minutes
        tool_policy: None,
    };

    println!(
        "Max budget: ${:.2}",
        budget_policy.max_budget_microusd.unwrap_or(0) as f64 / 1_000_000.0
    );
    println!("Max turns: {:?}", budget_policy.max_turns);
    println!("Max runtime: {:?}", budget_policy.max_runtime);

    // Example 2: Tool-restricted policy
    // This policy only allows specific tools and blocks dangerous ones
    println!("\nExample 2: Tool Policy");
    println!("----------------------");

    let tool_policy = ToolPolicyInput {
        // Only allow these specific tools
        toolset: Some(vec![
            "file_read".to_string(),
            "file_write".to_string(),
            "web_search".to_string(),
        ]),
        // Auto-approve safe read-only tools
        auto_approve_tools: Some(vec!["file_read".to_string(), "web_search".to_string()]),
        // Explicitly block dangerous tools
        disallowed_tools: Some(vec!["shell".to_string(), "execute_code".to_string()]),
    };

    let restricted_policy = RunPolicyInput {
        max_budget_microusd: Some(1_000_000), // $1.00
        max_turns: Some(20),
        max_runtime: Some(Duration::from_secs(600)),
        tool_policy: Some(tool_policy),
    };

    println!(
        "Allowed tools: {:?}",
        restricted_policy.tool_policy.as_ref().unwrap().toolset
    );
    println!(
        "Auto-approve: {:?}",
        restricted_policy
            .tool_policy
            .as_ref()
            .unwrap()
            .auto_approve_tools
    );
    println!(
        "Disallowed tools: {:?}",
        restricted_policy
            .tool_policy
            .as_ref()
            .unwrap()
            .disallowed_tools
    );

    // Example 3: Combining policies with client config
    println!("\nExample 3: Client Configuration with Policy");
    println!("---------------------------------------------");

    let config = ClientConfig::new("budget_constrained_user")
        .with_agent_name("safe_agent")
        .with_policy(RunPolicyInput {
            max_budget_microusd: Some(250_000), // $0.25
            max_turns: Some(5),
            max_runtime: Some(Duration::from_secs(120)),
            tool_policy: Some(ToolPolicyInput {
                toolset: Some(vec!["web_search".to_string()]),
                auto_approve_tools: Some(vec!["web_search".to_string()]),
                disallowed_tools: Some(vec!["shell".to_string(), "file_write".to_string()]),
            }),
        });

    println!("User: {}", config.user_id);
    println!("Agent: {}", config.agent_name);

    if let Some(policy) = &config.policy {
        println!(
            "Budget: ${:.2}",
            policy.max_budget_microusd.unwrap_or(0) as f64 / 1_000_000.0
        );
        println!("Max turns: {:?}", policy.max_turns);

        if let Some(tool_policy) = &policy.tool_policy {
            println!("Tool allowlist: {:?}", tool_policy.toolset);
            println!("Tool blocklist: {:?}", tool_policy.disallowed_tools);
        }
    }

    // Example 4: Policy enforcement behavior
    println!("\nExample 4: Policy Enforcement Behavior");
    println!("----------------------------------------");

    println!("When a policy constraint is exceeded:");
    println!("  - Budget exceeded: Run stops with StopReason::MaxBudgetExceeded");
    println!("  - Turns exceeded: Run stops with StopReason::MaxTurns");
    println!("  - Runtime exceeded: Run stops with StopReason::MaxRuntimeExceeded");
    println!("  - Disallowed tool: Tool call blocked with ToolError::PolicyViolation");

    println!("\nRollout modes control enforcement behavior:");
    println!("  - Enforce (default): Strictly block violations");
    println!("  - SoftFail: Allow but emit PolicyStop events");
    println!("  - ObserveOnly: Log only, never block");

    // Example 5: Using policy with one_shot execution
    println!("\nExample 5: Executing with Policy");
    println!("---------------------------------");

    println!("To execute with a policy:");
    println!("  let policy = RunPolicyInput {{");
    println!("      max_budget_microusd: Some(100_000),");
    println!("      max_turns: Some(3),");
    println!("      ..Default::default()");
    println!("  }};");
    println!();
    println!("  let result = client.one_shot(\"Your prompt\", Some(policy)).await?;");
    println!();
    println!("  match result.stop_reason {{");
    println!("      StopReason::Completed => println!(\"Success!\"),");
    println!("      StopReason::MaxBudgetExceeded => println!(\"Budget limit reached\"),");
    println!("      StopReason::MaxTurns => println!(\"Turn limit reached\"),");
    println!("      _ => println!(\"Stopped: {{:?}}\", result.stop_reason),");
    println!("  }}");

    println!("\n✅ Policy enforcement example completed!");
    println!("\nKey takeaways:");
    println!("  - Use RunPolicyInput to set budget, turn, and runtime limits");
    println!("  - Use ToolPolicyInput to control tool availability");
    println!("  - Attach policies to ClientConfig for persistent constraints");
    println!("  - Pass policies to one_shot() or stream() for per-run constraints");
    println!("  - Check stop_reason to understand why a run ended");

    Ok(())
}

/// Demonstrates creating different policy configurations
#[allow(dead_code)]
fn demonstrate_policy_patterns() {
    // Strict policy - minimal budget, limited tools
    let _strict = RunPolicyInput {
        max_budget_microusd: Some(50_000), // $0.05
        max_turns: Some(3),
        max_runtime: Some(Duration::from_secs(60)),
        tool_policy: Some(ToolPolicyInput {
            toolset: Some(vec!["web_search".to_string()]),
            auto_approve_tools: None,
            disallowed_tools: Some(vec!["shell".to_string(), "file_write".to_string()]),
        }),
    };

    // Permissive policy - higher limits, more tools
    let _permissive = RunPolicyInput {
        max_budget_microusd: Some(5_000_000), // $5.00
        max_turns: Some(50),
        max_runtime: Some(Duration::from_secs(1800)), // 30 minutes
        tool_policy: Some(ToolPolicyInput {
            toolset: None, // All available tools
            auto_approve_tools: Some(vec!["file_read".to_string(), "web_search".to_string()]),
            disallowed_tools: Some(vec!["shell".to_string()]),
        }),
    };

    // No policy - use runtime defaults
    let _default: Option<RunPolicyInput> = None;
}
