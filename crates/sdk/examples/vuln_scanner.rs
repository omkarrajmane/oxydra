//! Example: Dependency Vulnerability Scanner
//!
//! This example demonstrates how to use the Oxydra SDK to scan project dependencies
//! for known security vulnerabilities:
//! - Parse dependency files (Cargo.toml, package.json, requirements.txt)
//! - Query vulnerability databases via web search
//! - Check for CVEs and security advisories
//! - Generate structured vulnerability reports
//! - Provide remediation recommendations
//!
//! Use case: CI/CD security scanning, dependency auditing, compliance checks
//!
//! # Usage
//!
//! ```bash
//! # Place a dependency file in /shared/dependencies/
//! cargo run --example vuln_scanner -p sdk
//! ```

use std::collections::HashMap;
use std::time::Duration;

use sdk::{ClientConfig, OxydraClient, RunEvent};
use types::{RunPolicyInput, ToolPolicyInput};

/// A project dependency
#[derive(Debug, Clone)]
struct Dependency {
    name: String,
    version: String,
    ecosystem: Ecosystem,
    direct: bool, // true = direct dependency, false = transitive
}

/// Package ecosystem type
#[derive(Debug, Clone)]
enum Ecosystem {
    Rust,
    Node,
    Python,
    Go,
    Java,
    Unknown,
}

/// A vulnerability finding
#[derive(Debug, Clone)]
struct Vulnerability {
    cve_id: Option<String>,
    severity: Severity,
    title: String,
    description: String,
    affected_versions: String,
    fixed_versions: Option<String>,
    references: Vec<String>,
}

/// Severity levels
#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// Scan result for a single dependency
#[derive(Debug, Clone)]
struct DependencyScanResult {
    dependency: Dependency,
    vulnerabilities: Vec<Vulnerability>,
    scan_status: ScanStatus,
}

/// Status of the vulnerability scan
#[derive(Debug, Clone)]
enum ScanStatus {
    Clean,
    VulnerabilitiesFound(usize),
    ScanFailed(String),
}

/// Complete vulnerability scan report
#[derive(Debug, Clone)]
struct VulnScanReport {
    project_name: String,
    scan_timestamp: String,
    ecosystem: Ecosystem,
    total_dependencies: usize,
    results: Vec<DependencyScanResult>,
    summary: ScanSummary,
}

/// Summary statistics
#[derive(Debug, Clone)]
struct ScanSummary {
    critical_count: usize,
    high_count: usize,
    medium_count: usize,
    low_count: usize,
    clean_count: usize,
    failed_count: usize,
}

/// Demonstrates dependency vulnerability scanning
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Dependency Vulnerability Scanner ===\n");

    // Example 1: Define scan parameters
    println!("Example 1: Scan Configuration");
    println!("----------------------------");

    let project_path = "/shared/my-project/Cargo.toml";
    let ecosystem = Ecosystem::Rust;
    let budget_limit = 750_000; // $0.75

    println!("Project: {}", project_path);
    println!("Ecosystem: {:?}", ecosystem);
    println!("Budget limit: ${:.2}", budget_limit as f64 / 1_000_000.0);

    // Example 2: Configure client
    println!("\nExample 2: Scanner Configuration");
    println!("---------------------------------");

    let config = ClientConfig::new("security_scanner")
        .with_agent_name("vuln_scanner")
        .with_policy(RunPolicyInput {
            max_budget_microusd: Some(budget_limit),
            max_turns: Some(50),
            max_runtime: Some(Duration::from_secs(300)),
            tool_policy: Some(ToolPolicyInput {
                toolset: Some(vec![
                    "file_read".to_string(),
                    "web_search".to_string(),
                    "web_fetch".to_string(),
                    "file_write".to_string(),
                ]),
                auto_approve_tools: Some(vec![
                    "file_read".to_string(),
                    "web_search".to_string(),
                    "web_fetch".to_string(),
                ]),
                disallowed_tools: Some(vec!["shell".to_string()]),
            }),
        });

    println!("User: {}", config.user_id);
    println!("Agent: {}", config.agent_name);
    println!("Tools: file_read, web_search, web_fetch, file_write");

    // Example 3: Scanning workflow
    println!("\nExample 3: Vulnerability Scanning Workflow");
    println!("-------------------------------------------");

    println!("\nPhase 1: Dependency Discovery");
    println!("  - Read dependency manifest file");
    println!("  - Parse dependency names and versions");
    println!("  - Identify direct vs transitive dependencies");
    println!("  - Tool: file_read");

    println!("\nPhase 2: Vulnerability Lookup");
    println!("  - Search for CVEs affecting each dependency");
    println!("  - Query security advisory databases");
    println!("  - Check GitHub Security Advisories");
    println!("  - Tools: web_search, web_fetch");

    println!("\nPhase 3: Severity Assessment");
    println!("  - Parse CVSS scores from CVE data");
    println!("  - Categorize by severity (Critical/High/Medium/Low)");
    println!("  - Check for available patches");

    println!("\nPhase 4: Report Generation");
    println!("  - Compile vulnerability findings");
    println!("  - Generate remediation recommendations");
    println!("  - Export SARIF or Markdown report");
    println!("  - Tool: file_write");

    // Example 4: Simulated scan results
    println!("\nExample 4: Sample Vulnerability Scan Output");
    println!("------------------------------------------");

    let sample_report = create_sample_vuln_report();
    display_vuln_report(&sample_report);

    // Example 5: Event handling
    println!("\nExample 5: Streaming Scan Events");
    println!("--------------------------------");

    demonstrate_scan_event_handling();

    // Example 6: Remediation workflow
    println!("\nExample 6: Automated Remediation");
    println!("--------------------------------");

    demonstrate_remediation_workflow();

    println!("\n✅ Vulnerability scanner example completed!");
    println!("\nKey takeaways:");
    println!("  - Use file_read to parse dependency manifests");
    println!("  - Use web_search to query CVE databases");
    println!("  - Structure findings with severity levels");
    println!("  - Generate actionable remediation advice");
    println!("  - Export reports in standard formats (SARIF)");
    println!("  - Track scan costs with BudgetUpdate events");
    println!("  - Handle large dependency trees efficiently");

    Ok(())
}

/// Creates a sample vulnerability scan report
fn create_sample_vuln_report() -> VulnScanReport {
    let results = vec![
        DependencyScanResult {
            dependency: Dependency {
                name: "tokio".to_string(),
                version: "1.25.0".to_string(),
                ecosystem: Ecosystem::Rust,
                direct: true,
            },
            vulnerabilities: vec![
                Vulnerability {
                    cve_id: Some("CVE-2023-22466".to_string()),
                    severity: Severity::High,
                    title: "Resource exhaustion in tokio runtime".to_string(),
                    description: "A vulnerability in tokio's scheduler could lead to resource exhaustion".to_string(),
                    affected_versions: ">=1.0.0, <1.25.1".to_string(),
                    fixed_versions: Some("1.25.1".to_string()),
                    references: vec![
                        "https://cve.mitre.org/cgi-bin/cvename.cgi?name=CVE-2023-22466".to_string(),
                        "https://github.com/tokio-rs/tokio/security/advisories".to_string(),
                    ],
                },
            ],
            scan_status: ScanStatus::VulnerabilitiesFound(1),
        },
        DependencyScanResult {
            dependency: Dependency {
                name: "openssl".to_string(),
                version: "0.10.45".to_string(),
                ecosystem: Ecosystem::Rust,
                direct: false,
            },
            vulnerabilities: vec![
                Vulnerability {
                    cve_id: Some("CVE-2023-0464".to_string()),
                    severity: Severity::Critical,
                    title: "X.509 policy validation bypass".to_string(),
                    description: "OpenSSL's X.509 verification could be bypassed in certain conditions".to_string(),
                    affected_versions: ">=3.0.0, <3.0.8".to_string(),
                    fixed_versions: Some("3.0.8".to_string()),
                    references: vec![
                        "https://www.openssl.org/news/secadv/20230322.txt".to_string(),
                    ],
                },
                Vulnerability {
                    cve_id: Some("CVE-2023-0465".to_string()),
                    severity: Severity::Medium,
                    title: "Invalid certificate chain validation".to_string(),
                    description: "Certificate chain validation could be bypassed".to_string(),
                    affected_versions: ">=3.0.0, <3.0.8".to_string(),
                    fixed_versions: Some("3.0.8".to_string()),
                    references: vec![],
                },
            ],
            scan_status: ScanStatus::VulnerabilitiesFound(2),
        },
        DependencyScanResult {
            dependency: Dependency {
                name: "serde".to_string(),
                version: "1.0.152".to_string(),
                ecosystem: Ecosystem::Rust,
                direct: true,
            },
            vulnerabilities: vec![],
            scan_status: ScanStatus::Clean,
        },
    ];

    let summary = ScanSummary {
        critical_count: 1,
        high_count: 1,
        medium_count: 1,
        low_count: 0,
        clean_count: 1,
        failed_count: 0,
    };

    VulnScanReport {
        project_name: "my-rust-project".to_string(),
        scan_timestamp: "2024-03-12T15:45:00Z".to_string(),
        ecosystem: Ecosystem::Rust,
        total_dependencies: 3,
        results,
        summary,
    }
}

/// Displays a vulnerability scan report
fn display_vuln_report(report: &VulnScanReport) {
    println!("\n🔒 Vulnerability Scan Report: {}", report.project_name);
    println!("Ecosystem: {:?}", report.ecosystem);
    println!("Scan Time: {}", report.scan_timestamp);
    println!("Dependencies Scanned: {}", report.total_dependencies);

    // Summary
    println!("\n📊 Summary:");
    if report.summary.critical_count > 0 {
        println!("  ⚫ Critical: {}", report.summary.critical_count);
    }
    if report.summary.high_count > 0 {
        println!("  🔴 High: {}", report.summary.high_count);
    }
    if report.summary.medium_count > 0 {
        println!("  🟡 Medium: {}", report.summary.medium_count);
    }
    if report.summary.low_count > 0 {
        println!("  🟢 Low: {}", report.summary.low_count);
    }
    println!("  ✅ Clean: {}", report.summary.clean_count);
    if report.summary.failed_count > 0 {
        println!("  ❌ Failed: {}", report.summary.failed_count);
    }

    // Detailed findings
    println!("\n🔍 Detailed Findings:");
    for result in &report.results {
        match &result.scan_status {
            ScanStatus::Clean => {
                println!("\n  ✅ {} {}", result.dependency.name, result.dependency.version);
                println!("     Status: No vulnerabilities found");
            }
            ScanStatus::VulnerabilitiesFound(count) => {
                let severity_emoji = if result.vulnerabilities.iter().any(|v| v.severity == Severity::Critical) {
                    "⚫"
                } else if result.vulnerabilities.iter().any(|v| v.severity == Severity::High) {
                    "🔴"
                } else if result.vulnerabilities.iter().any(|v| v.severity == Severity::Medium) {
                    "🟡"
                } else {
                    "🟢"
                };
                println!("\n  {} {} {} ({} vulnerabilities)",
                    severity_emoji,
                    result.dependency.name,
                    result.dependency.version,
                    count
                );

                for vuln in &result.vulnerabilities {
                    let emoji = match vuln.severity {
                        Severity::Critical => "⚫",
                        Severity::High => "🔴",
                        Severity::Medium => "🟡",
                        Severity::Low => "🟢",
                    };
                    println!("\n     {} [{}] {}",
                        emoji,
                        vuln.cve_id.as_deref().unwrap_or("N/A"),
                        vuln.title
                    );
                    println!("        Severity: {:?}", vuln.severity);
                    println!("        Affected: {}", vuln.affected_versions);
                    if let Some(fixed) = &vuln.fixed_versions {
                        println!("        Fixed in: {}", fixed);
                    }
                }
            }
            ScanStatus::ScanFailed(reason) => {
                println!("\n  ❌ {} {}", result.dependency.name, result.dependency.version);
                println!("     Error: {}", reason);
            }
        }
    }

    // Recommendations
    println!("\n💡 Remediation Recommendations:");
    println!("  1. Update tokio to version 1.25.1 or later");
    println!("  2. Update openssl to version 3.0.8 or later");
    println!("  3. Run 'cargo update' to update transitive dependencies");
    println!("  4. Enable Dependabot or similar automated scanning");
}

/// Demonstrates event handling for scanning
#[allow(dead_code)]
fn demonstrate_scan_event_handling() {
    println!("Event handling pattern for vulnerability scanning:\n");

    println!("match event {{");
    println!("    RunEvent::Text(text) => {{");
    println!("        // Accumulate scan progress");
    println!("        scan_log.push_str(&text);");
    println!("    }}");
    println!();
    println!("    RunEvent::ToolCall(tool) => {{");
    println!("        match tool.name.as_str() {{");
    println!("            \"file_read\" => {{");
    println!("                println!(\"📁 Reading dependency file...\");");
    println!("            }}");
    println!("            \"web_search\" => {{");
    println!("                let query = tool.arguments.get(\"query\")");
    println!("                    .and_then(|v| v.as_str())");
    println!("                    .unwrap_or(\"unknown\");");
    println!("                println!(\"🔍 Searching CVE database: {{}}\", query);");
    println!("            }}");
    println!("            _ => {{}}");
    println!("        }}");
    println!("    }}");
    println!();
    println!("    RunEvent::ToolResult {{ call_id, content, success }} => {{");
    println!("        if success {{");
    println!("            // Parse vulnerability data");
    println!("            if let Ok(data) = serde_json::from_str::<Value>(&content) {{");
    println!("                vulnerabilities.extend(parse_cve_data(data));");
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
    println!("        println!(\"✅ Vulnerability scan complete\");");
    println!("        break;");
    println!("    }}");
    println!();
    println!("    _ => {{}}");
    println!("}}");
}

/// Demonstrates automated remediation workflow
#[allow(dead_code)]
fn demonstrate_remediation_workflow() {
    println!("Automated remediation workflow:\n");

    println!("fn generate_remediation_plan(report: &VulnScanReport) -> RemediationPlan {{");
    println!("    let mut updates = Vec::new();");
    println!("    let mut breaking_changes = Vec::new();");
    println!();
    println!("    for result in &report.results {{");
    println!("        if let ScanStatus::VulnerabilitiesFound(_) = result.scan_status {{");
    println!("            // Check if patch version available");
    println!("            for vuln in &result.vulnerabilities {{");
    println!("                if let Some(fixed) = &vuln.fixed_versions {{");
    println!("                    updates.push(DependencyUpdate {{");
    println!("                        name: result.dependency.name.clone(),");
    println!("                        current: result.dependency.version.clone(),");
    println!("                        target: fixed.clone(),");
    println!("                        severity: vuln.severity.clone(),");
    println!("                    }});");
    println!("                }}");
    println!("            }}");
    println!("        }}");
    println!("    }}");
    println!();
    println!("    RemediationPlan {{ updates, breaking_changes }}");
    println!("}}");
}

/// Parse dependencies from Cargo.toml
#[allow(dead_code)]
fn parse_cargo_toml(content: &str) -> Vec<Dependency> {
    let mut deps = Vec::new();
    // Simplified parsing - real implementation would use toml crate
    for line in content.lines() {
        if line.contains('=') && !line.starts_with('[') {
            let parts: Vec<&str> = line.split('=').collect();
            if parts.len() >= 2 {
                let name = parts[0].trim().trim_matches('"').to_string();
                let version = parts[1].trim().trim_matches('"').to_string();
                deps.push(Dependency {
                    name,
                    version,
                    ecosystem: Ecosystem::Rust,
                    direct: true,
                });
            }
        }
    }
    deps
}

/// Configuration for vulnerability scanner
#[derive(Debug, Clone)]
pub struct ScannerConfig {
    fail_on_severity: Severity,
    include_transitive: bool,
    budget_limit_microusd: u64,
    output_format: OutputFormat,
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Markdown,
    Json,
    Sarif,
}

impl Default for ScannerConfig {
    fn default() -> Self {
        Self {
            fail_on_severity: Severity::High,
            include_transitive: true,
            budget_limit_microusd: 750_000,
            output_format: OutputFormat::Markdown,
        }
    }
}

/// A planned dependency update
#[derive(Debug, Clone)]
struct DependencyUpdate {
    name: String,
    current: String,
    target: String,
    severity: Severity,
}

/// Complete remediation plan
#[derive(Debug, Clone)]
struct RemediationPlan {
    updates: Vec<DependencyUpdate>,
    breaking_changes: Vec<String>,
}
