//! Example: Security Reconnaissance Agent
//!
//! This example demonstrates how to use the Oxydra SDK for basic security reconnaissance:
//! - DNS enumeration and subdomain discovery
//! - Port scanning via shell commands (where permitted)
//! - Web technology fingerprinting
//! - SSL/TLS certificate analysis
//! - Structured security report generation
//!
//! Use case: Penetration testing, asset discovery, security auditing
//!
//! # Usage
//!
//! ```bash
//! # Set strict policy - only allow safe recon tools
//! cargo run --example security_recon -p sdk
//! ```
//!
//! # Security Notice
//!
//! This example demonstrates defensive security techniques only. All operations
//! should be performed on systems you own or have explicit permission to test.

use std::collections::HashMap;
use std::time::Duration;

use sdk::{ClientConfig, OxydraClient, RunEvent};
use types::{RunPolicyInput, ToolPolicyInput};

/// A discovered host/asset
#[derive(Debug, Clone)]
struct DiscoveredAsset {
    host: String,
    asset_type: AssetType,
    services: Vec<Service>,
    technologies: Vec<String>,
    risk_level: RiskLevel,
}

/// Type of asset discovered
#[derive(Debug, Clone)]
enum AssetType {
    WebApplication,
    ApiEndpoint,
    Subdomain,
    Service,
    Unknown,
}

/// A running service on a host
#[derive(Debug, Clone)]
struct Service {
    port: u16,
    protocol: String,
    banner: Option<String>,
    version: Option<String>,
}

/// Risk assessment level
#[derive(Debug, Clone, PartialEq)]
enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// Security reconnaissance report
#[derive(Debug, Clone)]
struct ReconReport {
    target: String,
    scan_timestamp: String,
    assets: Vec<DiscoveredAsset>,
    findings: Vec<SecurityFinding>,
    recommendations: Vec<String>,
}

/// A security finding/recommendation
#[derive(Debug, Clone)]
struct SecurityFinding {
    severity: RiskLevel,
    category: String,
    description: String,
    affected_assets: Vec<String>,
    remediation: String,
}

/// Demonstrates security reconnaissance with structured output
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Security Reconnaissance Example ===\n");

    // Example 1: Define reconnaissance scope
    println!("Example 1: Reconnaissance Scope");
    println!("--------------------------------");

    let target_domain = "example.com";
    let recon_depth = "standard"; // light, standard, deep
    let budget_limit = 1_000_000; // $1.00

    println!("Target domain: {}", target_domain);
    println!("Recon depth: {}", recon_depth);
    println!("Budget limit: ${:.2}", budget_limit as f64 / 1_000_000.0);

    // Example 2: Configure client with security-focused policy
    println!("\nExample 2: Security Agent Configuration");
    println!("--------------------------------------");

    let config = ClientConfig::new("security_auditor")
        .with_agent_name("recon_agent")
        .with_policy(RunPolicyInput {
            max_budget_microusd: Some(budget_limit),
            max_turns: Some(30),
            max_runtime: Some(Duration::from_secs(600)), // 10 minutes
            tool_policy: Some(ToolPolicyInput {
                toolset: Some(vec![
                    "web_search".to_string(),
                    "web_fetch".to_string(),
                    "file_write".to_string(),
                    // Note: shell_exec is excluded for safety in this example
                ]),
                auto_approve_tools: Some(vec![
                    "web_search".to_string(),
                    "web_fetch".to_string(),
                ]),
                disallowed_tools: Some(vec![
                    "shell".to_string(),
                    "file_delete".to_string(),
                ]),
            }),
        });

    println!("User: {}", config.user_id);
    println!("Agent: {}", config.agent_name);
    println!("Tools: web_search, web_fetch, file_write");
    println!("Note: shell_exec disabled for safety demonstration");

    // Example 3: Reconnaissance workflow
    println!("\nExample 3: Reconnaissance Workflow");
    println!("----------------------------------");

    println!("\nPhase 1: OSINT Discovery");
    println!("  - Search for public information about target");
    println!("  - Find subdomains via search engines");
    println!("  - Identify technology stack from documentation");
    println!("  - Tools: web_search, web_fetch");

    println!("\nPhase 2: Web Asset Enumeration");
    println!("  - Discover web applications and APIs");
    println!("  - Analyze HTTP headers and responses");
    println!("  - Identify frameworks and versions");
    println!("  - Tools: web_fetch");

    println!("\nPhase 3: Technology Fingerprinting");
    println!("  - Identify server software");
    println!("  - Detect CMS, frameworks, libraries");
    println!("  - Check for known vulnerable versions");
    println!("  - Tools: web_fetch, web_search (for CVE lookup)");

    println!("\nPhase 4: Risk Assessment");
    println!("  - Analyze findings for security implications");
    println!("  - Categorize risks by severity");
    println!("  - Generate remediation recommendations");

    println!("\nPhase 5: Report Generation");
    println!("  - Compile structured reconnaissance report");
    println!("  - Save to /shared for review");
    println!("  - Format: Markdown with JSON metadata");

    // Example 4: Simulated recon results
    println!("\nExample 4: Sample Reconnaissance Output");
    println!("----------------------------------------");

    let sample_report = create_sample_recon_report(target_domain);
    display_recon_report(&sample_report);

    // Example 5: Event handling for recon streaming
    println!("\nExample 5: Streaming Recon Events");
    println!("----------------------------------");

    demonstrate_recon_event_handling();

    // Example 6: Security findings analysis
    println!("\nExample 6: Security Findings Analysis");
    println!("--------------------------------------");

    demonstrate_findings_analysis();

    println!("\n✅ Security reconnaissance example completed!");
    println!("\nKey takeaways:");
    println!("  - Use web_search for OSINT discovery");
    println!("  - Use web_fetch for technology fingerprinting");
    println!("  - Structure findings into security-focused types");
    println!("  - Categorize risks by severity (Low/Medium/High/Critical)");
    println!("  - Generate actionable remediation recommendations");
    println!("  - Always follow responsible disclosure practices");
    println!("  - Only scan systems you own or have permission to test");
    println!("  - Use strict tool policies to prevent accidental damage");

    Ok(())
}

/// Creates a sample reconnaissance report
fn create_sample_recon_report(target: &str) -> ReconReport {
    let assets = vec![
        DiscoveredAsset {
            host: format!("www.{}", target),
            asset_type: AssetType::WebApplication,
            services: vec![
                Service {
                    port: 443,
                    protocol: "HTTPS".to_string(),
                    banner: Some("nginx/1.18.0".to_string()),
                    version: Some("1.18.0".to_string()),
                },
            ],
            technologies: vec![
                "nginx".to_string(),
                "React".to_string(),
                "Node.js".to_string(),
            ],
            risk_level: RiskLevel::Low,
        },
        DiscoveredAsset {
            host: format!("api.{}", target),
            asset_type: AssetType::ApiEndpoint,
            services: vec![
                Service {
                    port: 443,
                    protocol: "HTTPS".to_string(),
                    banner: Some("Apache/2.4.41".to_string()),
                    version: Some("2.4.41".to_string()),
                },
            ],
            technologies: vec![
                "Apache".to_string(),
                "Python".to_string(),
                "Django".to_string(),
            ],
            risk_level: RiskLevel::Medium,
        },
        DiscoveredAsset {
            host: format!("staging.{}", target),
            asset_type: AssetType::WebApplication,
            services: vec![
                Service {
                    port: 80,
                    protocol: "HTTP".to_string(),
                    banner: Some("Apache/2.4.29".to_string()),
                    version: Some("2.4.29".to_string()),
                },
            ],
            technologies: vec![
                "Apache".to_string(),
                "PHP".to_string(),
                "WordPress".to_string(),
            ],
            risk_level: RiskLevel::High,
        },
    ];

    let findings = vec![
        SecurityFinding {
            severity: RiskLevel::High,
            category: "Unencrypted Traffic".to_string(),
            description: "Staging environment accessible over HTTP without redirect".to_string(),
            affected_assets: vec![format!("staging.{}", target)],
            remediation: "Enable HTTPS and configure HSTS headers".to_string(),
        },
        SecurityFinding {
            severity: RiskLevel::Medium,
            category: "Outdated Software".to_string(),
            description: "Apache 2.4.29 has known vulnerabilities (CVE-2019-0211)".to_string(),
            affected_assets: vec![format!("staging.{}", target)],
            remediation: "Upgrade Apache to latest stable version".to_string(),
        },
        SecurityFinding {
            severity: RiskLevel::Low,
            category: "Information Disclosure".to_string(),
            description: "Server version exposed in HTTP headers".to_string(),
            affected_assets: vec![
                format!("www.{}", target),
                format!("api.{}", target),
                format!("staging.{}", target),
            ],
            remediation: "Configure ServerTokens to 'Prod' to hide version".to_string(),
        },
    ];

    ReconReport {
        target: target.to_string(),
        scan_timestamp: "2024-03-12T14:30:00Z".to_string(),
        assets: assets.clone(),
        findings: findings.clone(),
        recommendations: vec![
            "Implement HTTPS across all environments".to_string(),
            "Update Apache to version 2.4.57 or later".to_string(),
            "Enable automatic security patching".to_string(),
            "Conduct regular vulnerability scans".to_string(),
        ],
    }
}

/// Displays a reconnaissance report
fn display_recon_report(report: &ReconReport) {
    println!("\n🔒 Reconnaissance Report: {}", report.target);
    println!("Scan Time: {}", report.scan_timestamp);
    println!("Assets Discovered: {}", report.assets.len());

    println!("\n📋 Discovered Assets:");
    for (i, asset) in report.assets.iter().enumerate() {
        let risk_emoji = match asset.risk_level {
            RiskLevel::Low => "🟢",
            RiskLevel::Medium => "🟡",
            RiskLevel::High => "🔴",
            RiskLevel::Critical => "⚫",
        };
        println!("\n  {}. {} {}", i + 1, risk_emoji, asset.host);
        println!("     Type: {:?}", asset.asset_type);
        println!("     Technologies: {}", asset.technologies.join(", "));
        for service in &asset.services {
            println!(
                "     Service: {}:{} ({})",
                service.protocol, service.port,
                service.banner.as_deref().unwrap_or("unknown")
            );
        }
    }

    println!("\n⚠️  Security Findings:");
    for (i, finding) in report.findings.iter().enumerate() {
        let severity_emoji = match finding.severity {
            RiskLevel::Low => "🟢",
            RiskLevel::Medium => "🟡",
            RiskLevel::High => "🔴",
            RiskLevel::Critical => "⚫",
        };
        println!("\n  {}. {} [{}] {}",
            i + 1,
            severity_emoji,
            finding.category,
            finding.description
        );
        println!("     Affected: {}", finding.affected_assets.join(", "));
        println!("     Remediation: {}", finding.remediation);
    }

    println!("\n💡 Recommendations:");
    for (i, rec) in report.recommendations.iter().enumerate() {
        println!("  {}. {}", i + 1, rec);
    }
}

/// Demonstrates event handling for reconnaissance streaming
#[allow(dead_code)]
fn demonstrate_recon_event_handling() {
    println!("Event handling pattern for reconnaissance:\n");

    println!("match event {{");
    println!("    RunEvent::Text(text) => {{");
    println!("        // Accumulate reconnaissance summary");
    println!("        recon_content.push_str(&text);");
    println!("    }}");
    println!();
    println!("    RunEvent::ToolCall(tool) => {{");
    println!("        match tool.name.as_str() {{");
    println!("            \"web_search\" => {{");
    println!("                let query = tool.arguments.get(\"query\")");
    println!("                    .and_then(|v| v.as_str())");
    println!("                    .unwrap_or(\"unknown\");");
    println!("                println!(\"🔍 OSINT Search: {{}}\", query);");
    println!("            }}");
    println!("            \"web_fetch\" => {{");
    println!("                let url = tool.arguments.get(\"url\")");
    println!("                    .and_then(|v| v.as_str())");
    println!("                    .unwrap_or(\"unknown\");");
    println!("                println!(\"🌐 Fingerprinting: {{}}\", url);");
    println!("            }}");
    println!("            _ => {{}}");
    println!("        }}");
    println!("    }}");
    println!();
    println!("    RunEvent::ToolResult {{ call_id, content, success }} => {{");
    println!("        if success {{");
    println!("            // Parse reconnaissance data");
    println!("            if let Ok(data) = serde_json::from_str::<Value>(&content) {{");
    println!("                assets.extend(parse_assets(data));");
    println!("            }}");
    println!("        }}");
    println!("    }}");
    println!();
    println!("    RunEvent::BudgetUpdate {{ cost_microusd, .. }} => {{");
    println!("        let cost = cost_microusd as f64 / 1_000_000.0;");
    println!("        println!(\"💰 Scan cost: ${{:.4}}\", cost);");
    println!("    }}");
    println!();
    println!("    RunEvent::Completed(result) => {{");
    println!("        println!(\"✅ Reconnaissance complete\");");
    println!("        break;");
    println!("    }}");
    println!();
    println!("    _ => {{}}");
    println!("}}");
}

/// Demonstrates security findings analysis
#[allow(dead_code)]
fn demonstrate_findings_analysis() {
    println!("Security findings analysis workflow:\n");

    println!("fn analyze_security_risks(assets: &[DiscoveredAsset]) -> Vec<SecurityFinding> {{");
    println!("    let mut findings = Vec::new();");
    println!();
    println!("    for asset in assets {{");
    println!("        // Check for unencrypted services");
    println!("        for service in &asset.services {{");
    println!("            if service.port == 80 {{");
    println!("                findings.push(SecurityFinding {{");
    println!("                    severity: RiskLevel::High,");
    println!("                    category: \"Unencrypted Traffic\".to_string(),");
    println!("                    description: format!(");
    println!("                        \"HTTP service detected on {{}}\",");
    println!("                        asset.host");
    println!("                    ),");
    println!("                    affected_assets: vec![asset.host.clone()],");
    println!("                    remediation: \"Enable HTTPS with valid certificate\".to_string(),");
    println!("                }});");
    println!("            }}");
    println!("        }}");
    println!();
    println!("        // Check for outdated software");
    println!("        for tech in &asset.technologies {{");
    println!("            if let Some(version) = check_version(tech) {{");
    println!("                if is_vulnerable(tech, &version) {{");
    println!("                    findings.push(SecurityFinding {{");
    println!("                        severity: RiskLevel::Medium,");
    println!("                        category: \"Outdated Software\".to_string(),");
    println!("                        description: format!(");
    println!("                            \"{{}} {{}} has known vulnerabilities\",");
    println!("                            tech, version");
    println!("                        ),");
    println!("                        affected_assets: vec![asset.host.clone()],");
    println!("                        remediation: \"Update to latest stable version\".to_string(),");
    println!("                    }});");
    println!("                }}");
    println!("            }}");
    println!("        }}");
    println!("    }}");
    println!();
    println!("    findings");
    println!("}}");
}

/// Configuration for reconnaissance agent
#[derive(Debug, Clone)]
pub struct ReconConfig {
    target_domain: String,
    scan_depth: ScanDepth,
    budget_limit_microusd: u64,
    enable_passive_only: bool,
}

#[derive(Debug, Clone)]
pub enum ScanDepth {
    Light,
    Standard,
    Deep,
}

impl Default for ReconConfig {
    fn default() -> Self {
        Self {
            target_domain: "example.com".to_string(),
            scan_depth: ScanDepth::Standard,
            budget_limit_microusd: 1_000_000,
            enable_passive_only: true,
        }
    }
}
