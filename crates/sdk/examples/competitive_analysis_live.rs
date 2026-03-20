//! Real-World Example: AI Coding Assistants Competitive Analysis
//!
//! This example demonstrates a real agentic workflow that would run WITHIN the Oxydra runtime.
//! It shows how to build a complete analysis pipeline that:
//! - Uses web_search to find comparison articles
//! - Uses web_fetch to extract detailed content
//! - Uses LLM to synthesize structured competitive analysis
//! - Generates actionable recommendations
//!
//! # Architecture Note
//!
//! This example uses the SDK's embedded client pattern (runs within the Oxydra runtime).
//! For a standalone client that connects to a remote daemon via WebSocket, you would use
//! the gateway WebSocket protocol directly.
//!
//! # To Run This Example
//!
//! ```bash
//! # This compiles the example (it's a demonstration of the SDK API)
//! cargo build --example competitive_analysis_live -p sdk
//!
//! # To actually execute with real searches, you'd need:
//! # 1. Oxydra daemon running with Exa configured
//! # 2. A program that uses this SDK code within the runtime context
//! # 3. Or use the WebSocket gateway to send turns to the daemon
//! ```
//!
//! # Expected Runtime (when executed)
//! - 30-60 seconds (depends on API latency)
//! - Cost: ~$0.05-0.15 (budget capped at $0.50)

#![allow(dead_code, unused_imports)]


use std::collections::HashMap;
use std::time::Duration;

use sdk::ClientConfig;
use types::{RunPolicyInput, ToolPolicyInput};

/// Hardcoded search topic - AI coding assistants competitive analysis
const SEARCH_TOPIC: &str = "AI coding assistants comparison 2024 Cursor GitHub Copilot CodeWhisperer features pricing";

/// Maximum sources to analyze
const MAX_SOURCES: usize = 5;

/// Budget limit in micro-USD ($0.50)
const BUDGET_LIMIT: u64 = 500_000;

/// Competitive analysis result
#[derive(Debug, Clone)]
struct CompetitiveAnalysis {
    market_category: String,
    analysis_date: String,
    sources_analyzed: usize,
    tools: Vec<ToolAnalysis>,
    comparison_matrix: ComparisonMatrix,
    market_insights: Vec<String>,
    recommendations: Vec<Recommendation>,
    total_cost_usd: f64,
}

/// Analysis of a single tool
#[derive(Debug, Clone)]
struct ToolAnalysis {
    name: String,
    vendor: String,
    pricing: PricingInfo,
    key_features: Vec<String>,
    supported_languages: Vec<String>,
    supported_ides: Vec<String>,
    user_rating: Option<f32>,
    strengths: Vec<String>,
    weaknesses: Vec<String>,
    best_for: Vec<String>,
}

/// Pricing information
#[derive(Debug, Clone)]
struct PricingInfo {
    free_tier: bool,
    paid_tier_start: Option<String>,
    enterprise_pricing: Option<String>,
    notes: String,
}

/// Comparison matrix across all tools
#[derive(Debug, Clone)]
struct ComparisonMatrix {
    features: HashMap<String, Vec<bool>>,
    pricing_tiers: Vec<(String, Vec<String>)>,
    language_support: HashMap<String, Vec<bool>>,
    ide_support: HashMap<String, Vec<bool>>,
}

/// Actionable recommendation
#[derive(Debug, Clone)]
struct Recommendation {
    category: String,
    target_audience: String,
    recommendation: String,
    rationale: String,
}

/// Source article metadata
#[derive(Debug, Clone)]
struct SourceArticle {
    title: String,
    url: String,
    source: String,
    published_date: Option<String>,
    content_summary: String,
}

/// Demonstrates the competitive analysis workflow
/// 
/// NOTE: This is a demonstration of the SDK API patterns. In a real implementation,
/// you would either:
/// 1. Run this code within the Oxydra runtime (embedded client)
/// 2. Connect to the daemon via WebSocket gateway protocol
/// 3. Use the runner CLI to execute agent configurations
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 AI Coding Assistants Competitive Analysis");
    println!("============================================\n");
    
    println!("Search Topic: {}", SEARCH_TOPIC);
    println!("Max Sources: {}", MAX_SOURCES);
    println!("Budget Limit: ${:.2}\n", BUDGET_LIMIT as f64 / 1_000_000.0);
    
    println!("📋 This example demonstrates the SDK API for building agentic workflows.");
    println!("   To execute with real searches, run within Oxydra runtime or use WebSocket gateway.\n");
    
    // Demonstrate the configuration
    demonstrate_configuration();
    
    // Demonstrate the workflow phases
    demonstrate_workflow();
    
    // Show sample output
    demonstrate_sample_output();
    
    println!("\n✅ Example workflow demonstration complete!");
    println!("\nTo run this for real:");
    println!("  1. Start Oxydra daemon with Exa configured");
    println!("  2. Use this SDK code within the runtime context");
    println!("  3. Or connect via WebSocket gateway protocol");
    
    Ok(())
}

/// Demonstrates client configuration
fn demonstrate_configuration() {
    println!("⚙️  Client Configuration");
    println!("------------------------");
    
    let config = ClientConfig::new("alice")
        .with_agent_name("competitive_analyst")
        .with_policy(RunPolicyInput {
            max_budget_microusd: Some(BUDGET_LIMIT),
            max_turns: Some(50),
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
    println!("Budget: ${:.2}", BUDGET_LIMIT as f64 / 1_000_000.0);
    println!("Tools: web_search, web_fetch, file_write\n");
}

/// Demonstrates the analysis workflow
fn demonstrate_workflow() {
    println!("📊 Analysis Workflow");
    println!("--------------------");
    
    println!("\nPhase 1: Search for Comparison Articles");
    println!("  Prompt: Search for '{}'", SEARCH_TOPIC);
    println!("  Tool: web_search (via Exa API)");
    println!("  Expected: 5 comparison articles/reviews");
    println!("  Cost: ~$0.01-0.02");
    
    println!("\nPhase 2: Fetch and Analyze Each Source");
    println!("  For each of the 5 sources:");
    println!("    - Fetch full article content");
    println!("    - Extract tool information (pricing, features, ratings)");
    println!("    - Parse structured data from unstructured text");
    println!("  Tool: web_fetch");
    println!("  Cost: ~$0.02-0.05 per source");
    
    println!("\nPhase 3: Consolidate and Deduplicate");
    println!("  - Merge duplicate tool entries from multiple sources");
    println!("  - Combine feature lists");
    println!("  - Aggregate ratings");
    println!("  - Build unified tool profiles");
    
    println!("\nPhase 4: Synthesize Competitive Matrix");
    println!("  Prompt: Create comparison matrix from analyzed tools");
    println!("  LLM generates:");
    println!("    - Feature comparison table");
    println!("    - Pricing tier comparison");
    println!("    - Language/IDE support matrix");
    println!("    - Market insights");
    println!("    - Recommendations by user type");
    println!("  Cost: ~$0.02-0.04");
    
    println!("\nPhase 5: Generate Report");
    println!("  - Compile structured analysis");
    println!("  - Save to /shared/ai_coding_assistants_analysis.json");
    println!("  Tool: file_write");
}

/// Demonstrates sample output
fn demonstrate_sample_output() {
    println!("\n📈 Sample Output (Simulated)");
    println!("------------------------------");
    
    let sample_analysis = create_sample_analysis();
    display_analysis(&sample_analysis);
}

/// Creates a sample analysis for demonstration
fn create_sample_analysis() -> CompetitiveAnalysis {
    let tools = vec![
        ToolAnalysis {
            name: "GitHub Copilot".to_string(),
            vendor: "GitHub/Microsoft".to_string(),
            pricing: PricingInfo {
                free_tier: true,
                paid_tier_start: Some("$10/month (Individual)".to_string()),
                enterprise_pricing: Some("$19-39/user/month".to_string()),
                notes: "Free for verified students and maintainers".to_string(),
            },
            key_features: vec![
                "Code completion".to_string(),
                "Chat interface".to_string(),
                "Pull request summaries".to_string(),
                "Test generation".to_string(),
            ],
            supported_languages: vec![
                "Python".to_string(),
                "JavaScript".to_string(),
                "TypeScript".to_string(),
                "Go".to_string(),
                "Rust".to_string(),
            ],
            supported_ides: vec![
                "VS Code".to_string(),
                "JetBrains IDEs".to_string(),
                "Visual Studio".to_string(),
                "Neovim".to_string(),
            ],
            user_rating: Some(4.5),
            strengths: vec![
                "Excellent IDE integration".to_string(),
                "Large training dataset".to_string(),
                "Fast suggestions".to_string(),
            ],
            weaknesses: vec![
                "Limited context window".to_string(),
                "Occasional irrelevant suggestions".to_string(),
            ],
            best_for: vec!["Individual developers".to_string(), "Teams".to_string()],
        },
        ToolAnalysis {
            name: "Cursor".to_string(),
            vendor: "Anysphere".to_string(),
            pricing: PricingInfo {
                free_tier: true,
                paid_tier_start: Some("$20/month (Pro)".to_string()),
                enterprise_pricing: Some("Custom pricing".to_string()),
                notes: "Free tier limited to 2000 completions/month".to_string(),
            },
            key_features: vec![
                "AI-powered IDE".to_string(),
                "Code generation".to_string(),
                "Natural language editing".to_string(),
                "Terminal integration".to_string(),
            ],
            supported_languages: vec![
                "All major languages".to_string(),
            ],
            supported_ides: vec![
                "Cursor IDE (VS Code fork)".to_string(),
            ],
            user_rating: Some(4.7),
            strengths: vec![
                "Full IDE replacement".to_string(),
                "Powerful chat interface".to_string(),
                "Context-aware suggestions".to_string(),
            ],
            weaknesses: vec![
                "Requires switching editors".to_string(),
                "Higher price point".to_string(),
            ],
            best_for: vec!["Power users".to_string(), "AI-first developers".to_string()],
        },
        ToolAnalysis {
            name: "Amazon CodeWhisperer".to_string(),
            vendor: "AWS".to_string(),
            pricing: PricingInfo {
                free_tier: true,
                paid_tier_start: Some("$19/month (Professional)".to_string()),
                enterprise_pricing: Some("Custom pricing".to_string()),
                notes: "Free for individual use".to_string(),
            },
            key_features: vec![
                "Code suggestions".to_string(),
                "Security scanning".to_string(),
                "Reference tracking".to_string(),
                "AWS integration".to_string(),
            ],
            supported_languages: vec![
                "Python".to_string(),
                "JavaScript".to_string(),
                "Java".to_string(),
                "TypeScript".to_string(),
            ],
            supported_ides: vec![
                "VS Code".to_string(),
                "JetBrains IDEs".to_string(),
                "AWS Cloud9".to_string(),
            ],
            user_rating: Some(4.0),
            strengths: vec![
                "Strong security focus".to_string(),
                "AWS service integration".to_string(),
                "Reference attribution".to_string(),
            ],
            weaknesses: vec![
                "Fewer languages than competitors".to_string(),
                "Less mature chat features".to_string(),
            ],
            best_for: vec!["AWS developers".to_string(), "Enterprise teams".to_string()],
        },
    ];
    
    CompetitiveAnalysis {
        market_category: "AI Coding Assistants".to_string(),
        analysis_date: "2024-03-12T15:30:00Z".to_string(),
        sources_analyzed: 5,
        tools,
        comparison_matrix: ComparisonMatrix {
            features: HashMap::new(),
            pricing_tiers: vec![],
            language_support: HashMap::new(),
            ide_support: HashMap::new(),
        },
        market_insights: vec![
            "Market is consolidating around 3-4 major players".to_string(),
            "Pricing ranges from free to $40/user/month".to_string(),
            "IDE integration quality is the key differentiator".to_string(),
            "Security and compliance features increasingly important".to_string(),
        ],
        recommendations: vec![
            Recommendation {
                category: "Individual Developers".to_string(),
                target_audience: "Solo developers, freelancers, students".to_string(),
                recommendation: "GitHub Copilot Individual ($10/month) or Cursor Free".to_string(),
                rationale: "Best balance of features, IDE support, and pricing".to_string(),
            },
            Recommendation {
                category: "Small Teams".to_string(),
                target_audience: "Startups, small dev teams (2-20 people)".to_string(),
                recommendation: "GitHub Copilot Business ($19/user/month)".to_string(),
                rationale: "Team collaboration features, centralized billing, good value".to_string(),
            },
            Recommendation {
                category: "Enterprises".to_string(),
                target_audience: "Large organizations with compliance needs".to_string(),
                recommendation: "GitHub Copilot Enterprise or Amazon CodeWhisperer Professional".to_string(),
                rationale: "Enterprise security, SSO, audit logs, compliance certifications".to_string(),
            },
            Recommendation {
                category: "AI-First Developers".to_string(),
                target_audience: "Developers who want maximum AI assistance".to_string(),
                recommendation: "Cursor Pro ($20/month)".to_string(),
                rationale: "Full AI-native IDE experience, most powerful chat interface".to_string(),
            },
        ],
        total_cost_usd: 0.0847,
    }
}

/// Displays the competitive analysis
fn display_analysis(analysis: &CompetitiveAnalysis) {
    println!("\n╔════════════════════════════════════════════════════════════════╗");
    println!("║     AI CODING ASSISTANTS - COMPETITIVE ANALYSIS REPORT         ║");
    println!("╚════════════════════════════════════════════════════════════════╝");
    println!();
    println!("Market Category: {}", analysis.market_category);
    println!("Analysis Date: {}", analysis.analysis_date);
    println!("Sources Analyzed: {}", analysis.sources_analyzed);
    println!("Total Cost: ${:.4}", analysis.total_cost_usd);
    println!();
    
    println!("┌────────────────────────────────────────────────────────────────┐");
    println!("│ TOOLS ANALYZED                                                 │");
    println!("└────────────────────────────────────────────────────────────────┘");
    
    for (i, tool) in analysis.tools.iter().enumerate() {
        println!("\n{}. {} by {}", i + 1, tool.name, tool.vendor);
        println!("   Pricing: {}", 
            if tool.pricing.free_tier { 
                format!("Free tier + {}", tool.pricing.paid_tier_start.as_deref().unwrap_or("Paid plans"))
            } else { 
                tool.pricing.paid_tier_start.as_deref().unwrap_or("Paid").to_string() 
            }
        );
        if let Some(rating) = tool.user_rating {
            println!("   User Rating: {:.1}/5.0", rating);
        }
        println!("   Features: {}", tool.key_features.join(", "));
        println!("   Languages: {}", tool.supported_languages.join(", "));
        
        if !tool.strengths.is_empty() {
            println!("   ✅ Strengths:");
            for strength in &tool.strengths[..std::cmp::min(3, tool.strengths.len())] {
                println!("      • {}", strength);
            }
        }
        
        if !tool.weaknesses.is_empty() {
            println!("   ⚠️  Weaknesses:");
            for weakness in &tool.weaknesses[..std::cmp::min(2, tool.weaknesses.len())] {
                println!("      • {}", weakness);
            }
        }
    }
    
    println!("\n┌────────────────────────────────────────────────────────────────┐");
    println!("│ MARKET INSIGHTS                                                │");
    println!("└────────────────────────────────────────────────────────────────┘");
    for insight in &analysis.market_insights {
        println!("  • {}", insight);
    }
    
    println!("\n┌────────────────────────────────────────────────────────────────┐");
    println!("│ RECOMMENDATIONS                                                │");
    println!("└────────────────────────────────────────────────────────────────┘");
    for rec in &analysis.recommendations {
        println!("\n  📌 {}", rec.category);
        println!("     For: {}", rec.target_audience);
        println!("     → {}", rec.recommendation);
        println!("     Why: {}", rec.rationale);
    }
    
    println!();
}
