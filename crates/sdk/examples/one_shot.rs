//! Example: Simple One-Shot Execution
//!
//! This example demonstrates the simplest usage of the Oxydra SDK:
//! - Creating a client with configuration
//! - Executing a single-turn prompt
//! - Handling the result
//!
//! # Usage
//!
//! ```bash
//! cargo run --example one_shot -p sdk
//! ```

use sdk::ClientConfig;
use types::RunPolicyInput;

/// This example shows how to perform a simple one-shot execution.
///
/// A one-shot execution is a single-turn prompt that returns a complete
/// response without streaming. This is useful for simple queries that
/// don't require multi-turn conversations.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: Configure the client
    // The ClientConfig specifies the user ID, agent name, and optional settings
    let config = ClientConfig::new("example_user").with_agent_name("default");

    println!("Client configured for user: {}", config.user_id);
    println!("Using agent: {}", config.agent_name);

    // Step 2: Build the client
    // In a real application, you would provide the runtime, gateway, and turn_runner
    // For this example, we show the API structure
    println!("\nBuilding OxydraClient with config...");

    // Note: In a real application, you would create these components:
    // let runtime = Arc::new(AgentRuntime::new(...));
    // let gateway = Arc::new(GatewayServer::new(...));
    // let turn_runner = Arc::new(MyTurnRunner::new(...));
    //
    // let client = OxydraClient::builder()
    //     .config(config)
    //     .runtime(runtime)
    //     .gateway(gateway)
    //     .turn_runner(turn_runner)
    //     .build()?;

    // Step 3: Execute a one-shot prompt
    // The one_shot method takes a prompt and optional policy, then returns a RunResult
    println!("\nExample: Executing one-shot prompt");
    println!("Prompt: 'What is the capital of France?'");

    // Without policy - uses default runtime limits
    let _policy: Option<RunPolicyInput> = None;
    println!("Policy: None (using default runtime limits)");

    // In a real application:
    // let result: RunResult = client.one_shot("What is the capital of France?", None).await?;

    // Step 4: Handle the result
    // The RunResult contains the response text, stop reason, usage info, and tool calls
    println!("\nExpected RunResult structure:");
    println!("  - response: String (the assistant's reply)");
    println!("  - stop_reason: StopReason (why the run stopped)");
    println!("  - usage: Option<UsageUpdate> (token usage stats)");
    println!("  - tool_calls: Vec<ToolCall> (any tool calls made)");

    // Example result handling:
    // println!("Response: {}", result.response);
    // println!("Stop reason: {:?}", result.stop_reason);
    // if let Some(usage) = result.usage {
    //     println!("Tokens used: {}", usage.total_tokens.unwrap_or(0));
    // }

    println!("\n✅ One-shot execution example completed successfully!");
    println!("\nKey takeaways:");
    println!("  - Use ClientConfig to specify user and agent");
    println!("  - Use OxydraClient::builder() to construct the client");
    println!("  - Use one_shot() for single-turn executions");
    println!("  - RunResult contains the complete response and metadata");

    Ok(())
}

/// Demonstrates the RunResult API
#[allow(dead_code)]
fn demonstrate_run_result_api() {
    // Create a simple RunResult
    let result = sdk::RunResult {
        response: "The capital of France is Paris.".to_string(),
        stop_reason: types::StopReason::Completed,
        usage: None,
        tool_calls: vec![],
    };

    println!("\nRunResult example:");
    println!("  Response: {}", result.response);
    println!("  Stop reason: {:?}", result.stop_reason);
    println!("  Tool calls: {:?}", result.tool_calls);
}
