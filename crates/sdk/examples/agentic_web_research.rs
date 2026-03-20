//! Example: Agentic Web Research with Structured Output
//!
//! This example demonstrates how to use the Oxydra SDK for agentic web research:
//! - Search for information using web_search tool
//! - Fetch detailed content from relevant sources
//! - Structure the findings into a research report
//! - Save results to a file for further processing
//!
//! Use case: Research a technology topic, competitor analysis, or academic research
//!
//! # Usage
//!
//! ```bash
//! # Set your web search provider (duckduckgo, google, or searxng)
//! export OXYDRA_WEB_SEARCH_PROVIDER=duckduckgo
//!
//! # For Google search, also set:
//! # export OXYDRA_WEB_SEARCH_GOOGLE_API_KEY=your_key
//! # export OXYDRA_WEB_SEARCH_GOOGLE_CX=your_engine_id
//!
//! cargo run --example agentic_web_research -p sdk
//! ```

#![allow(dead_code, unused_imports)]


use std::collections::HashMap;
use std::time::Duration;

use sdk::{ClientConfig, OxydraClient, RunEvent};
use types::{RunPolicyInput, ToolPolicyInput};

/// A structured research finding
#[derive(Debug, Clone)]
struct ResearchFinding {
    title: String,
    url: String,
    source: String,
    summary: String,
    relevance_score: f32,
}

/// A complete research report
#[derive(Debug, Clone)]
struct ResearchReport {
    query: String,
    findings: Vec<ResearchFinding>,
    total_sources: usize,
    key_insights: Vec<String>,
    generated_at: String,
}

/// Demonstrates agentic web research with structured output
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Agentic Web Research Example ===\n");

    // Example 1: Define research parameters
    println!("Example 1: Research Configuration");
    println!("--------------------------------");

    let research_topic = "Rust async runtime performance comparison 2024";
    let max_sources = 5;
    let budget_limit = 500_000; // $0.50

    println!("Research topic: {}", research_topic);
    println!("Max sources: {}", max_sources);
    println!("Budget limit: ${:.2}", budget_limit as f64 / 1_000_000.0);

    // Example 2: Configure client with research-focused policy
    println!("\nExample 2: Client Configuration");
    println!("--------------------------------");

    let config = ClientConfig::new("researcher")
        .with_agent_name("web_researcher")
        .with_policy(RunPolicyInput {
            max_budget_microusd: Some(budget_limit),
            max_turns: Some(20),
            max_runtime: Some(Duration::from_secs(300)),
            tool_policy: Some(ToolPolicyInput {
                toolset: Some(vec![
                    "web_search".to_string(),
                    "web_fetch".to_string(),
                    "file_write".to_string(),
                ]),
                auto_approve_tools: Some(vec![
                    "web_search".to_string(),
                    "web_fetch".to_string(),
                ]),
                disallowed_tools: Some(vec!["shell".to_string()]),
            }),
        });

    println!("User: {}", config.user_id);
    println!("Agent: {}", config.agent_name);
    println!("Tools: web_search, web_fetch, file_write");

    // Example 3: Research workflow structure
    println!("\nExample 3: Agentic Research Workflow");
    println!("--------------------------------------");

    println!("\nStep 1: Initial Search");
    println!("  - Query: '{}'", research_topic);
    println!("  - Tool: web_search");
    println!("  - Expected: List of relevant URLs with snippets");

    println!("\nStep 2: Source Evaluation");
    println!("  - Filter by relevance (title, snippet quality)");
    println!("  - Prioritize authoritative sources");
    println!("  - Limit to top N sources");

    println!("\nStep 3: Deep Content Extraction");
    println!("  - Fetch full content from selected URLs");
    println!("  - Tool: web_fetch");
    println!("  - Extract key information");

    println!("\nStep 4: Synthesis & Structuring");
    println!("  - Summarize findings");
    println!("  - Generate insights");
    println!("  - Create structured report");

    println!("\nStep 5: Output Generation");
    println!("  - Save report to file");
    println!("  - Tool: file_write");
    println!("  - Format: Markdown or JSON");

    // Example 4: Simulated research results
    println!("\nExample 4: Sample Research Output");
    println!("---------------------------------");

    let sample_report = create_sample_report(research_topic);
    display_report(&sample_report);

    // Example 5: Event handling for research streaming
    println!("\nExample 5: Streaming Research Events");
    println!("-------------------------------------");

    demonstrate_research_event_handling();

    // Example 6: Complete implementation template
    println!("\nExample 6: Complete Implementation");
    println!("------------------------------------");

    demonstrate_complete_implementation();

    println!("\n✅ Agentic web research example completed!");
    println!("\nKey takeaways:");
    println!("  - Use web_search for initial discovery");
    println!("  - Use web_fetch for deep content extraction");
    println!("  - Structure findings into typed objects (ResearchFinding)");
    println!("  - Generate reports with metadata (timestamps, sources)");
    println!("  - Save results to /shared for persistence");
    println!("  - Handle BudgetUpdate events for cost tracking");
    println!("  - Use ToolCall/ToolResult events to track progress");

    Ok(())
}

/// Creates a sample research report for demonstration
fn create_sample_report(topic: &str) -> ResearchReport {
    let findings = vec![
        ResearchFinding {
            title: "Tokio vs async-std: Performance Benchmarks 2024".to_string(),
            url: "https://example.com/tokio-benchmarks".to_string(),
            source: "Rust Performance Blog".to_string(),
            summary: "Tokio shows 15% better throughput in I/O-bound workloads".to_string(),
            relevance_score: 0.95,
        },
        ResearchFinding {
            title: "smol: A small and fast async runtime".to_string(),
            url: "https://example.com/smol-runtime".to_string(),
            source: "smol Documentation".to_string(),
            summary: "Minimal runtime with competitive performance for embedded use".to_string(),
            relevance_score: 0.88,
        },
        ResearchFinding {
            title: "Comparing Rust Async Runtimes: A Deep Dive".to_string(),
            url: "https://example.com/async-comparison".to_string(),
            source: "Rust Weekly".to_string(),
            summary: "Analysis of scheduling algorithms and memory usage patterns".to_string(),
            relevance_score: 0.92,
        },
    ];

    ResearchReport {
        query: topic.to_string(),
        findings: findings.clone(),
        total_sources: findings.len(),
        key_insights: vec![
            "Tokio remains the most popular choice for production".to_string(),
            "smol offers compelling minimal footprint for constrained environments".to_string(),
            "Performance differences are workload-dependent".to_string(),
        ],
        generated_at: "2024-03-12T10:30:00Z".to_string(),
    }
}

/// Displays a research report
fn display_report(report: &ResearchReport) {
    println!("\n📊 Research Report: {}", report.query);
    println!("Generated: {}", report.generated_at);
    println!("Sources analyzed: {}", report.total_sources);

    println!("\n🔍 Key Findings:");
    for (i, finding) in report.findings.iter().enumerate() {
        println!("\n  {}. {}", i + 1, finding.title);
        println!("     Source: {}", finding.source);
        println!("     URL: {}", finding.url);
        println!("     Relevance: {:.0}%", finding.relevance_score * 100.0);
        println!("     Summary: {}", finding.summary);
    }

    println!("\n💡 Key Insights:");
    for (i, insight) in report.key_insights.iter().enumerate() {
        println!("  {}. {}", i + 1, insight);
    }
}

/// Demonstrates event handling for research streaming
#[allow(dead_code)]
fn demonstrate_research_event_handling() {
    println!("Event handling pattern for research:\n");

    println!("match event {{");
    println!("    RunEvent::Text(text) => {{");
    println!("        // Accumulate research summary text");
    println!("        report_content.push_str(&text);");
    println!("    }}");
    println!();
    println!("    RunEvent::ToolCall(tool) => {{");
    println!("        match tool.name.as_str() {{");
    println!("            \"web_search\" => {{");
    println!("                println!(\"🔍 Searching...\");");
    println!("            }}");
    println!("            \"web_fetch\" => {{");
    println!("                let url = tool.arguments.get(\"url\")");
    println!("                    .and_then(|v| v.as_str())");
    println!("                    .unwrap_or(\"unknown\");");
    println!("                println!(\"📄 Fetching: {{}}\", url);");
    println!("            }}");
    println!("            \"file_write\" => {{");
    println!("                println!(\"💾 Saving report...\");");
    println!("            }}");
    println!("            _ => {{}}");
    println!("        }}");
    println!("    }}");
    println!();
    println!("    RunEvent::ToolResult {{ call_id, content, success }} => {{");
    println!("        if success {{");
    println!("            // Parse tool result and extract data");
    println!("            if let Ok(data) = serde_json::from_str::<Value>(&content) {{");
    println!("                findings.push(parse_finding(data));");
    println!("            }}");
    println!("        }}");
    println!("    }}");
    println!();
    println!("    RunEvent::BudgetUpdate {{ cost_microusd, .. }} => {{");
    println!("        let cost = cost_microusd as f64 / 1_000_000.0;");
    println!("        println!(\"💰 Research cost so far: ${{:.4}}\", cost);");
    println!("    }}");
    println!();
    println!("    RunEvent::Completed(result) => {{");
    println!("        println!(\"✅ Research complete: {{}}\", result.response);");
    println!("        break;");
    println!("    }}");
    println!();
    println!("    _ => {{}}");
    println!("}}");
}

/// Demonstrates complete implementation template
#[allow(dead_code)]
fn demonstrate_complete_implementation() {
    println!("Complete research agent implementation:\n");

    println!("pub struct ResearchAgent {{");
    println!("    client: OxydraClient,");
    println!("    config: ResearchConfig,");
    println!("}}");
    println!();
    println!("impl ResearchAgent {{");
    println!("    pub async fn research(&self, topic: &str) -> Result<ResearchReport, Error> {{");
    println!();
    println!("        // Step 1: Search for sources");
    println!("        let search_results = self.search_sources(topic).await?;");
    println!();
    println!("        // Step 2: Fetch and analyze top sources");
    println!("        let mut findings = Vec::new();");
    println!("        for source in search_results.iter().take(self.config.max_sources) {{");
    println!("            let content = self.fetch_content(&source.url).await?;");
    println!("            let finding = self.analyze_content(source, &content).await?;");
    println!("            findings.push(finding);");
    println!("        }}");
    println!();
    println!("        // Step 3: Generate insights");
    println!("        let insights = self.generate_insights(&findings).await?;");
    println!();
    println!("        // Step 4: Create and save report");
    println!("        let report = ResearchReport {{");
    println!("            query: topic.to_string(),");
    println!("            findings,");
    println!("            total_sources: findings.len(),");
    println!("            key_insights: insights,");
    println!("            generated_at: Utc::now().to_rfc3339(),");
    println!("        }};");
    println!();
    println!("        self.save_report(&report).await?;");
    println!();
    println!("        Ok(report)");
    println!("    }}");
    println!();
    println!("    async fn search_sources(&self, query: &str) -> Result<Vec<SearchResult>, Error> {{");
    println!("        let prompt = format!(");
    println!("            \"Search for: {{}}. Return the top 10 most relevant results.\",");
    println!("            query");
    println!("        );");
    println!("        let result = self.client.one_shot(prompt, None).await?;");
    println!("        parse_search_results(&result.response)");
    println!("    }}");
    println!();
    println!("    async fn fetch_content(&self, url: &str) -> Result<String, Error> {{");
    println!("        let prompt = format!(");
    println!("            \"Fetch and summarize the content from: {{}}\",");
    println!("            url");
    println!("        );");
    println!("        let result = self.client.one_shot(prompt, None).await?;");
    println!("        Ok(result.response)");
    println!("    }}");
    println!("}}");
}

/// Configuration for research agent
#[derive(Debug, Clone)]
pub struct ResearchConfig {
    max_sources: usize,
    budget_limit_microusd: u64,
    relevance_threshold: f32,
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            max_sources: 5,
            budget_limit_microusd: 500_000,
            relevance_threshold: 0.7,
        }
    }
}

/// Parse search results from web_search tool output
#[allow(dead_code)]
fn parse_search_results(output: &str) -> Result<Vec<SearchResult>, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(output)?;
    let results = value
        .get("results")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    Some(SearchResult {
                        title: item.get("title")?.as_str()?.to_string(),
                        url: item.get("url")?.as_str()?.to_string(),
                        snippet: item.get("snippet")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(results)
}

/// A search result from web_search
#[derive(Debug, Clone)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}
