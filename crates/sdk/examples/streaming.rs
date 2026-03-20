//! Example: Streaming with BudgetUpdate Events
//!
//! This example demonstrates how to use the streaming API to receive
//! real-time events including text deltas, tool calls, and budget updates.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example streaming -p sdk
//! ```

use std::time::Duration;

use sdk::ClientConfig;
use types::{RunPolicyInput, ToolPolicyInput};

/// Demonstrates the streaming API with real-time event handling.
///
/// The stream() method returns a RunEventStream that yields events as they
/// occur during agent execution. This allows for real-time display of
/// progress, incremental text output, and budget tracking.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Streaming API Example ===\n");

    // Example 1: Understanding RunEvent types
    println!("Example 1: RunEvent Types");
    println!("---------------------------");

    println!("The streaming API yields these event types:\n");

    println!("RunEvent::Text(String)");
    println!("  - Text delta from the assistant");
    println!("  - Accumulate these to build the full response\n");

    println!("RunEvent::ToolCall(ToolCall)");
    println!("  - A tool call was initiated by the agent");
    println!("  - Contains tool name, arguments, and call ID\n");

    println!("RunEvent::ToolResult {{ call_id, content, success }}");
    println!("  - Result from a tool execution");
    println!("  - Links back to the ToolCall via call_id\n");

    println!("RunEvent::BudgetUpdate {{ tokens_used, cost_microusd, remaining_budget }}");
    println!("  - Budget tracking during execution");
    println!("  - Helps monitor costs in real-time\n");

    println!("RunEvent::BudgetWarning {{ remaining, threshold_pct }}");
    println!("  - Emitted when budget crosses thresholds (e.g., 80%, 95%)\n");

    println!("RunEvent::PolicyStop {{ reason, stop_reason }}");
    println!("  - Policy enforcement stopped the run");
    println!("  - Includes the reason (e.g., MaxBudgetExceeded)\n");

    println!("RunEvent::Completed(RunResult)");
    println!("  - Run finished successfully");
    println!("  - Contains final response, usage, and tool calls\n");

    // Example 2: Setting up a streaming client with budget tracking
    println!("Example 2: Streaming with Budget Policy");
    println!("--------------------------------------");

    let _config = ClientConfig::new("streaming_user")
        .with_agent_name("streaming_agent")
        .with_policy(RunPolicyInput {
            max_budget_microusd: Some(1_000_000), // $1.00 budget
            max_turns: Some(10),
            max_runtime: Some(Duration::from_secs(300)),
            tool_policy: Some(ToolPolicyInput {
                toolset: Some(vec!["web_search".to_string(), "file_read".to_string()]),
                auto_approve_tools: Some(vec!["web_search".to_string()]),
                disallowed_tools: Some(vec!["shell".to_string()]),
            }),
        });

    println!("Client configured with $1.00 budget limit");
    println!("Budget updates will be emitted during streaming\n");

    // Example 3: Event handling pattern
    println!("Example 3: Event Handling Pattern");
    println!("---------------------------------");

    println!("Typical streaming event loop:\n");
    println!("  let mut stream = client.stream(\"Your prompt\", Some(policy)).await?;");
    println!("  let mut response_text = String::new();");
    println!("  let mut total_cost: u64 = 0;");
    println!();
    println!("  while let Some(event) = stream.next().await {{");
    println!("      match event {{");
    println!("          RunEvent::Text(text) => {{");
    println!("              print!(\"{{}}\", text);");
    println!("              response_text.push_str(&text);");
    println!("          }}");
    println!("          RunEvent::BudgetUpdate {{ cost_microusd, remaining_budget, .. }} => {{");
    println!("              total_cost = cost_microusd;");
    println!("              println!(\"\\n[Cost: ${{:.4}}, Remaining: ${{:.4}}]\",");
    println!("                  cost_microusd as f64 / 1_000_000.0,");
    println!("                  remaining_budget.unwrap_or(0) as f64 / 1_000_000.0);");
    println!("          }}");
    println!("          RunEvent::ToolCall(tool_call) => {{");
    println!("              println!(\"\\n[Tool: {{}}]\", tool_call.name);");
    println!("          }}");
    println!("          RunEvent::Completed(result) => {{");
    println!("              println!(\"\\n\\nCompleted: {{}}\", result.response);");
    println!("              break;");
    println!("          }}");
    println!("          RunEvent::PolicyStop {{ reason, .. }} => {{");
    println!("              println!(\"\\nPolicy stop: {{}}\", reason);");
    println!("              break;");
    println!("          }}");
    println!("          _ => {{}}");
    println!("      }}");
    println!("  }}");

    // Example 4: Budget tracking strategies
    println!("\nExample 4: Budget Tracking Strategies");
    println!("-------------------------------------");

    println!("Strategy 1: Real-time cost display");
    println!("  - Show running cost as it accumulates");
    println!("  - Useful for interactive applications\n");

    println!("Strategy 2: Budget threshold alerts");
    println!("  - Listen for BudgetWarning events");
    println!("  - Alert user at 50%, 80%, 95% thresholds\n");

    println!("Strategy 3: Cost accumulation");
    println!("  - Sum all BudgetUpdate events");
    println!("  - Report total cost at completion\n");

    // Example 5: Complete streaming example
    println!("Example 5: Complete Streaming Implementation");
    println!("--------------------------------------------");

    demonstrate_streaming_structure();

    println!("\n✅ Streaming example completed!");
    println!("\nKey takeaways:");
    println!("  - Use client.stream() for real-time event streaming");
    println!("  - Handle RunEvent::Text for incremental output");
    println!("  - Handle RunEvent::BudgetUpdate for cost tracking");
    println!("  - Handle RunEvent::Completed for final result");
    println!("  - Handle RunEvent::PolicyStop for policy violations");
    println!("  - Use futures_util::StreamExt for stream iteration");

    Ok(())
}

/// Demonstrates the complete streaming structure
#[allow(dead_code)]
fn demonstrate_streaming_structure() {
    println!();
    println!("async fn stream_with_budget_tracking(");
    println!("    client: &sdk::OxydraClient,");
    println!("    prompt: &str,");
    println!("    policy: Option<RunPolicyInput>,");
    println!(") -> Result<(String, u64), Box<dyn std::error::Error>> {{");
    println!();
    println!("    let mut stream = client.stream(prompt, policy).await?;");
    println!("    let mut full_response = String::new();");
    println!("    let mut total_tokens = 0_u64;");
    println!("    let mut total_cost = 0_u64;");
    println!();
    println!("    while let Some(event) = stream.next().await {{");
    println!("        match event {{");
    println!("            RunEvent::Text(text) => {{");
    println!("                print!(\"{{}}\", text);");
    println!("                full_response.push_str(&text);");
    println!("            }}");
    println!();
    println!("            RunEvent::BudgetUpdate {{");
    println!("                tokens_used,");
    println!("                cost_microusd,");
    println!("                remaining_budget,");
    println!("            }} => {{");
    println!("                total_tokens = tokens_used;");
    println!("                total_cost = cost_microusd;");
    println!();
    println!("                if let Some(remaining) = remaining_budget {{");
    println!("                    let remaining_usd = remaining as f64 / 1_000_000.0;");
    println!("                    if remaining_usd < 0.10 {{");
    println!(
        "                        eprintln!(\"\\n[WARNING: Low budget: ${{:.2}}]\", remaining_usd);"
    );
    println!("                    }}");
    println!("                }}");
    println!("            }}");
    println!();
    println!("            RunEvent::BudgetWarning {{ remaining, threshold_pct }} => {{");
    println!("                eprintln!(");
    println!("                    \"\\n[BUDGET WARNING: {{}}% used, ${{:.2}} remaining]\",");
    println!("                    threshold_pct,");
    println!("                    remaining as f64 / 1_000_000.0");
    println!("                );");
    println!("            }}");
    println!();
    println!("            RunEvent::ToolCall(tool_call) => {{");
    println!("                println!(");
    println!("                    \"\\n[TOOL CALL: {{}} ({{}})]\",");
    println!("                    tool_call.name,");
    println!("                    tool_call.id");
    println!("                );");
    println!("            }}");
    println!();
    println!("            RunEvent::ToolResult {{ call_id, content, success }} => {{");
    println!("                println!(");
    println!("                    \"\\n[TOOL RESULT: {{}} - {{}}]\",");
    println!("                    call_id,");
    println!("                    if success {{ \"success\" }} else {{ \"failed\" }}");
    println!("                );");
    println!("                if !success {{");
    println!("                    eprintln!(\"  Error: {{}}\", content);");
    println!("                }}");
    println!("            }}");
    println!();
    println!("            RunEvent::Completed(result) => {{");
    println!("                println!(");
    println!("                    \"\\n\\n[COMPLETED] Stop reason: {{:?}}\",");
    println!("                    result.stop_reason");
    println!("                );");
    println!("                if let Some(usage) = result.usage {{");
    println!("                    println!(");
    println!("                        \"Tokens: {{}} prompt + {{}} completion = {{}} total\",");
    println!("                        usage.prompt_tokens.unwrap_or(0),");
    println!("                        usage.completion_tokens.unwrap_or(0),");
    println!("                        usage.total_tokens.unwrap_or(0)");
    println!("                    );");
    println!("                }}");
    println!("                return Ok((full_response, total_cost));");
    println!("            }}");
    println!();
    println!("            RunEvent::PolicyStop {{ reason, stop_reason }} => {{");
    println!("                eprintln!(");
    println!("                    \"\\n[POLICY STOP: {{}} ({{:?}})]\",");
    println!("                    reason, stop_reason");
    println!("                );");
    println!("                return Ok((full_response, total_cost));");
    println!("            }}");
    println!();
    println!("            _ => {{}}");
    println!("        }}");
    println!("    }}");
    println!();
    println!("    Ok((full_response, total_cost))");
    println!("}}");
}

/// Shows different event handling patterns
#[allow(dead_code)]
fn demonstrate_event_patterns() {
    // Pattern 1: Minimal - just get the text
    println!("\nPattern 1: Minimal text extraction");
    println!("  while let Some(RunEvent::Text(text)) = stream.next().await {{");
    println!("      print!(\"{{}}\", text);");
    println!("  }}");

    // Pattern 2: Cost monitoring only
    println!("\nPattern 2: Cost monitoring");
    println!("  while let Some(event) = stream.next().await {{");
    println!("      if let RunEvent::BudgetUpdate {{ cost_microusd, .. }} = event {{");
    println!("          println!(\"Cost so far: ${{:.4}}\", cost_microusd as f64 / 1_000_000.0);");
    println!("      }}");
    println!("  }}");

    // Pattern 3: Full event logging
    println!("\nPattern 3: Full event logging");
    println!("  while let Some(event) = stream.next().await {{");
    println!("      println!(\"[EVENT] {{:?}}\", event);");
    println!("  }}");
}
