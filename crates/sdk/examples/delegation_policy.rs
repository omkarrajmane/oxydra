//! Example: Parent-to-Child Policy Delegation
//!
//! This example demonstrates how policies are inherited and narrowed
//! when agents delegate tasks to subagents. Shows strictest-wins semantics.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example delegation_policy -p sdk
//! ```

use std::collections::HashSet;
use std::time::Duration;

use chrono::Utc;
use types::{EffectiveRunPolicy, FunctionDecl, RolloutMode, RunPolicyInput, ToolPolicyInput};

/// Demonstrates policy inheritance in delegation chains.
///
/// When an agent delegates to a subagent, the child inherits the parent's
/// policy constraints with strictest-wins semantics:
/// - Budget: min(parent_remaining, child_requested)
/// - Tools: intersection(parent_allowed, child_requested)
/// - Max turns: min(parent_max, child_max)
/// - Deadline: inherits parent's remaining deadline
/// - Disallowed tools: union of both lists
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Policy Delegation Example ===\n");

    // Example 1: Parent policy setup
    println!("Example 1: Parent Agent Policy");
    println!("--------------------------------");

    let parent_policy = create_parent_policy();

    println!(
        "Parent budget: ${:.2}",
        parent_policy.initial_budget_microusd as f64 / 1_000_000.0
    );
    println!("Parent max turns: {:?}", parent_policy.max_turns);
    println!("Parent allowed tools: {:?}", parent_policy.toolset.len());
    println!(
        "Parent disallowed tools: {:?}",
        parent_policy.disallowed_tools
    );

    // Example 2: Child policy request
    println!("\nExample 2: Child Agent Policy Request");
    println!("---------------------------------------");

    let child_request = RunPolicyInput {
        max_budget_microusd: Some(750_000), // Child requests $0.75
        max_turns: Some(15),                // Child requests 15 turns
        max_runtime: Some(Duration::from_secs(400)),
        tool_policy: Some(ToolPolicyInput {
            toolset: Some(vec![
                "web_search".to_string(),
                "file_read".to_string(),
                "file_write".to_string(),
            ]),
            auto_approve_tools: Some(vec!["web_search".to_string()]),
            disallowed_tools: Some(vec!["network_scan".to_string()]),
        }),
    };

    println!(
        "Child requests budget: ${:.2}",
        child_request.max_budget_microusd.unwrap_or(0) as f64 / 1_000_000.0
    );
    println!("Child requests max turns: {:?}", child_request.max_turns);
    println!(
        "Child requests tools: {:?}",
        child_request.tool_policy.as_ref().unwrap().toolset
    );

    // Example 3: Policy narrowing (strictest-wins)
    println!("\nExample 3: Policy Narrowing (Strictest-Wins)");
    println!("---------------------------------------------");

    println!("When parent delegates to child, constraints are narrowed:\n");

    // Budget: min(parent, child)
    let effective_budget = std::cmp::min(
        parent_policy.remaining_budget_microusd,
        child_request.max_budget_microusd.unwrap_or(u64::MAX),
    );
    println!(
        "Budget: min($1.00, $0.75) = ${:.2}",
        effective_budget as f64 / 1_000_000.0
    );

    // Max turns: min(parent, child)
    let effective_turns = match (parent_policy.max_turns, child_request.max_turns) {
        (Some(p), Some(c)) => Some(std::cmp::min(p, c)),
        (Some(p), None) => Some(p),
        (None, Some(c)) => Some(c),
        (None, None) => None,
    };
    println!("Max turns: min(20, 15) = {:?}", effective_turns);

    // Tools: intersection(parent, child)
    let parent_tools: HashSet<String> = parent_policy
        .toolset
        .iter()
        .map(|t| t.name.clone())
        .collect();
    let child_tools: HashSet<String> = child_request
        .tool_policy
        .as_ref()
        .unwrap()
        .toolset
        .as_ref()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let effective_tools: Vec<String> = parent_tools.intersection(&child_tools).cloned().collect();
    println!(
        "Tools: intersection of parent and child = {:?}",
        effective_tools
    );

    // Disallowed tools: union of both
    let child_disallowed: HashSet<String> = child_request
        .tool_policy
        .as_ref()
        .unwrap()
        .disallowed_tools
        .as_ref()
        .unwrap()
        .iter()
        .cloned()
        .collect();
    let effective_disallowed: Vec<String> = parent_policy
        .disallowed_tools
        .union(&child_disallowed)
        .cloned()
        .collect();
    println!(
        "Disallowed tools: union of both = {:?}",
        effective_disallowed
    );

    // Example 4: Delegation depth tracking
    println!("\nExample 4: Delegation Depth Tracking");
    println!("-------------------------------------");

    println!("Session IDs track delegation depth:\n");

    let root_session = "root_session_123";
    let child_session = format!("subagent:{}:uuid-1", root_session);
    let grandchild_session = format!("subagent:{}:uuid-2", child_session);

    println!("Root:       {}", root_session);
    println!("Child:      {}", child_session);
    println!("Grandchild: {}", grandchild_session);

    let root_depth = count_delegation_depth(root_session);
    let child_depth = count_delegation_depth(&child_session);
    let grandchild_depth = count_delegation_depth(&grandchild_session);

    println!("\nDepth calculation:");
    println!("  Root depth:       {}", root_depth);
    println!("  Child depth:      {}", child_depth);
    println!("  Grandchild depth: {}", grandchild_depth);
    println!("\nMax delegation depth: 5 levels");

    // Example 5: Creating effective policy for child
    println!("\nExample 5: Child's Effective Policy");
    println!("-----------------------------------");

    let child_effective_policy = EffectiveRunPolicy {
        started_at: parent_policy.started_at,
        deadline: parent_policy.deadline, // Inherits parent's deadline
        initial_budget_microusd: effective_budget,
        remaining_budget_microusd: effective_budget,
        toolset: vec![], // Would be populated with FunctionDecl objects
        auto_approve_tools: parent_policy.auto_approve_tools.clone(),
        disallowed_tools: effective_disallowed.iter().cloned().collect(),
        parent_run_id: Some("parent-run-123".to_string()),
        max_turns: effective_turns,
        rollout_mode: RolloutMode::Enforce,
    };

    println!("Child effective policy:");
    println!(
        "  Initial budget: ${:.2}",
        child_effective_policy.initial_budget_microusd as f64 / 1_000_000.0
    );
    println!("  Max turns: {:?}", child_effective_policy.max_turns);
    println!(
        "  Parent run ID: {:?}",
        child_effective_policy.parent_run_id
    );
    println!("  Rollout mode: {:?}", child_effective_policy.rollout_mode);

    // Example 6: Budget consumption across delegation chain
    println!("\nExample 6: Budget Consumption in Delegation Chain");
    println!("-------------------------------------------------");

    println!("Budget flows down the delegation chain:\n");

    let parent_budget = 1_000_000_u64;
    let child_budget = 750_000_u64;
    let grandchild_budget = 500_000_u64;

    println!("Initial allocation:");
    println!(
        "  Parent:      ${:.2} (full budget)",
        parent_budget as f64 / 1_000_000.0
    );
    println!(
        "  Child:       ${:.2} (narrowed)",
        child_budget as f64 / 1_000_000.0
    );
    println!(
        "  Grandchild:  ${:.2} (further narrowed)",
        grandchild_budget as f64 / 1_000_000.0
    );

    println!("\nAfter grandchild spends $0.10:");
    let grandchild_spend = 100_000_u64;
    let grandchild_remaining = grandchild_budget - grandchild_spend;
    println!(
        "  Grandchild spent:   ${:.2}",
        grandchild_spend as f64 / 1_000_000.0
    );
    println!(
        "  Grandchild remaining: ${:.2}",
        grandchild_remaining as f64 / 1_000_000.0
    );

    println!("\nBudget propagates up on completion:");
    println!(
        "  Child sees:     ${:.2} remaining",
        (child_budget - grandchild_spend) as f64 / 1_000_000.0
    );
    println!(
        "  Parent sees:    ${:.2} remaining",
        (parent_budget - grandchild_spend) as f64 / 1_000_000.0
    );

    // Example 7: Policy enforcement at each level
    println!("\nExample 7: Multi-Level Policy Enforcement");
    println!("-------------------------------------------");

    println!("Policy violations are checked at each level:\n");

    println!("1. Parent level:");
    println!("   - Checks parent's max_turns, budget, deadline");
    println!("   - Enforces parent's disallowed_tools\n");

    println!("2. Child level:");
    println!("   - Checks narrowed max_turns (min of parent/child)");
    println!("   - Checks narrowed budget (min of parent/child)");
    println!("   - Enforces intersection of disallowed tools\n");

    println!("3. Grandchild level:");
    println!("   - Checks further narrowed constraints");
    println!("   - Cannot exceed any ancestor's limits\n");

    println!("Security principle: Child cannot escalate beyond parent constraints");

    // Example 8: Using delegation with the SDK
    println!("\nExample 8: SDK Usage with Delegation");
    println!("--------------------------------------");

    demonstrate_delegation_sdk_usage();

    println!("\n✅ Delegation policy example completed!");
    println!("\nKey takeaways:");
    println!("  - Policies are narrowed using strictest-wins semantics");
    println!("  - Budget: minimum of parent remaining and child request");
    println!("  - Tools: intersection of parent and child allowlists");
    println!("  - Disallowed tools: union of both lists (always wins)");
    println!("  - Session IDs track delegation depth with 'subagent:' prefix");
    println!("  - Child cannot exceed parent constraints (security boundary)");
    println!("  - Budget consumption propagates up the delegation chain");

    Ok(())
}

/// Creates a sample parent policy for demonstration
fn create_parent_policy() -> EffectiveRunPolicy {
    let mut auto_approve = HashSet::new();
    auto_approve.insert("file_read".to_string());
    auto_approve.insert("web_search".to_string());

    let mut disallowed = HashSet::new();
    disallowed.insert("shell".to_string());
    disallowed.insert("execute_code".to_string());

    EffectiveRunPolicy {
        started_at: Utc::now(),
        deadline: None,
        initial_budget_microusd: 1_000_000,
        remaining_budget_microusd: 1_000_000,
        toolset: vec![
            FunctionDecl {
                name: "file_read".to_string(),
                description: Some("Read file contents".to_string()),
                parameters: serde_json::json!({}),
            },
            FunctionDecl {
                name: "file_write".to_string(),
                description: Some("Write to file".to_string()),
                parameters: serde_json::json!({}),
            },
            FunctionDecl {
                name: "web_search".to_string(),
                description: Some("Search the web".to_string()),
                parameters: serde_json::json!({}),
            },
        ],
        auto_approve_tools: auto_approve,
        disallowed_tools: disallowed,
        parent_run_id: None,
        max_turns: Some(20),
        rollout_mode: RolloutMode::Enforce,
    }
}

/// Counts delegation depth from session ID
fn count_delegation_depth(session_id: &str) -> usize {
    session_id.matches("subagent:").count()
}

/// Demonstrates SDK usage patterns for delegation
#[allow(dead_code)]
fn demonstrate_delegation_sdk_usage() {
    println!();
    println!("// Parent agent configuration");
    println!("let parent_config = ClientConfig::new(\"parent_user\")");
    println!("    .with_agent_name(\"orchestrator_agent\")");
    println!("    .with_policy(RunPolicyInput {{");
    println!("        max_budget_microusd: Some(2_000_000), // $2.00");
    println!("        max_turns: Some(30),");
    println!("        tool_policy: Some(ToolPolicyInput {{");
    println!(
        "            toolset: Some(vec![\"delegate\".to_string(), \"web_search\".to_string()]),"
    );
    println!("            disallowed_tools: Some(vec![\"shell\".to_string()]),");
    println!("            ..Default::default()");
    println!("        }}),");
    println!("        ..Default::default()");
    println!("    }});");
    println!();
    println!("// When parent delegates to child, the child's policy is automatically");
    println!("// narrowed based on the parent's effective policy.");
    println!();
    println!("// Child agent inherits narrowed constraints:");
    println!("// - Budget: min(parent_remaining, child_requested)");
    println!("// - Tools: intersection(parent_allowed, child_requested)");
    println!("// - Max turns: min(parent_max, child_max)");
    println!("// - Deadline: parent's remaining deadline");
    println!();
    println!("// The SDK handles policy inheritance automatically through the");
    println!("// DelegationRequest which includes the parent_policy field.");
}

/// Shows different policy narrowing scenarios
#[allow(dead_code)]
fn demonstrate_narrowing_scenarios() {
    println!("\nScenario 1: Child requests more than parent allows");
    println!("  Parent budget: $1.00, Child requests: $2.00");
    println!("  Result: Child gets $1.00 (parent wins)\n");

    println!("Scenario 2: Child requests less than parent allows");
    println!("  Parent budget: $1.00, Child requests: $0.50");
    println!("  Result: Child gets $0.50 (child request wins, more restrictive)\n");

    println!("Scenario 3: Tool intersection");
    println!("  Parent tools: [read, write, search], Child tools: [read, search, delete]");
    println!("  Result: Child gets [read, search] (intersection)\n");

    println!("Scenario 4: Disallowed tool union");
    println!("  Parent blocks: [shell], Child blocks: [network_scan]");
    println!("  Result: Child cannot use [shell, network_scan] (union)");
}
