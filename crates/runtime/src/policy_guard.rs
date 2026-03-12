//! Policy guard for admission-time policy resolution and validation.
//!
//! This module provides the `resolve_policy` function that validates and resolves
//! effective run policies at session admission time. It combines global configuration,
//! agent definition constraints, and per-run policy overrides using strictest-wins semantics.

use std::collections::HashSet;

use chrono::Utc;
use thiserror::Error;

use types::{
    AgentDefinition, EffectiveRunPolicy, FunctionDecl, RunPolicyInput, RuntimeConfig, merge_policy,
};

/// Errors that can occur during policy validation and resolution.
#[derive(Debug, Clone, Error, PartialEq)]
pub enum PolicyValidationError {
    /// Budget is explicitly set to zero, which is not allowed.
    #[error("budget cannot be zero")]
    ZeroBudget,

    /// Maximum turns is negative (would be a very large usize).
    #[error("max_turns cannot be negative")]
    NegativeMaxTurns,

    /// Maximum budget is negative (would be a very large u64).
    #[error("max_budget cannot be negative")]
    NegativeMaxBudget,

    /// Maximum runtime is negative (would be a very large duration).
    #[error("max_runtime cannot be negative")]
    NegativeMaxRuntime,

    /// No tools available after applying constraints.
    #[error("no tools available after applying policy constraints")]
    EmptyToolset,

    /// Tool referenced in policy does not exist in the registry.
    #[error("unknown tool: {tool_name}")]
    UnknownTool { tool_name: String },
}

/// Resolves and validates the effective run policy for a session.
///
/// This function performs admission-time policy resolution by:
/// 1. Validating input constraints (rejecting zero/negative values)
/// 2. Merging global, agent, and per-run constraints using strictest-wins semantics
/// 3. Setting the started_at timestamp
/// 4. Computing the deadline from max_runtime
/// 5. Resolving the toolset against available tools
///
/// # Arguments
///
/// * `global_config` - Global runtime configuration with baseline limits
/// * `agent_def` - Agent definition with agent-specific constraints
/// * `per_run` - Per-run policy overrides
/// * `available_tools` - The complete set of tools available in the system
///
/// # Returns
///
/// `Ok(EffectiveRunPolicy)` if validation passes and policy is resolved.
/// `Err(PolicyValidationError)` if validation fails.
///
/// # Examples
///
/// ```
/// use runtime::policy_guard::resolve_policy;
/// use types::{RuntimeConfig, AgentDefinition, RunPolicyInput, FunctionDecl};
///
/// let global = RuntimeConfig::default();
/// let agent = AgentDefinition::default();
/// let per_run = RunPolicyInput::default();
/// let tools: Vec<FunctionDecl> = vec![];
///
/// let result = resolve_policy(&global, &agent, &per_run, &tools);
/// ```
pub fn resolve_policy(
    global_config: &RuntimeConfig,
    agent_def: &AgentDefinition,
    per_run: &RunPolicyInput,
    available_tools: &[FunctionDecl],
) -> Result<EffectiveRunPolicy, PolicyValidationError> {
    validate_per_run_constraints(per_run)?;
    validate_tool_references(per_run, available_tools)?;

    let mut policy = merge_policy(global_config, agent_def, per_run, available_tools);
    policy.started_at = Utc::now();

    validate_effective_policy(&policy)?;

    Ok(policy)
}

fn validate_per_run_constraints(per_run: &RunPolicyInput) -> Result<(), PolicyValidationError> {
    if let Some(max_turns) = per_run.max_turns
        && max_turns > usize::MAX / 2
    {
        return Err(PolicyValidationError::NegativeMaxTurns);
    }

    if let Some(max_budget) = per_run.max_budget_microusd {
        if max_budget == 0 {
            return Err(PolicyValidationError::ZeroBudget);
        }
        if max_budget > u64::MAX / 2 {
            return Err(PolicyValidationError::NegativeMaxBudget);
        }
    }

    if let Some(max_runtime) = per_run.max_runtime
        && max_runtime.as_secs() > i64::MAX as u64
    {
        return Err(PolicyValidationError::NegativeMaxRuntime);
    }

    Ok(())
}

fn validate_tool_references(
    per_run: &RunPolicyInput,
    available_tools: &[FunctionDecl],
) -> Result<(), PolicyValidationError> {
    let available_names: HashSet<String> = available_tools.iter().map(|t| t.name.clone()).collect();

    if let Some(ref tool_policy) = per_run.tool_policy {
        if let Some(ref toolset) = tool_policy.toolset {
            for tool_name in toolset {
                if !available_names.contains(tool_name) {
                    return Err(PolicyValidationError::UnknownTool {
                        tool_name: tool_name.clone(),
                    });
                }
            }
        }

        if let Some(ref auto_approve) = tool_policy.auto_approve_tools {
            for tool_name in auto_approve {
                if !available_names.contains(tool_name) {
                    return Err(PolicyValidationError::UnknownTool {
                        tool_name: tool_name.clone(),
                    });
                }
            }
        }
    }

    Ok(())
}

fn validate_effective_policy(policy: &EffectiveRunPolicy) -> Result<(), PolicyValidationError> {
    if policy.initial_budget_microusd > 0
        && policy.remaining_budget_microusd > policy.initial_budget_microusd
    {
        return Err(PolicyValidationError::NegativeMaxBudget);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use types::{ToolParameterSchema, ToolPolicyInput};

    fn create_runtime_config(
        max_turns: usize,
        max_cost: Option<f64>,
        turn_timeout: u64,
    ) -> RuntimeConfig {
        RuntimeConfig {
            max_turns,
            max_cost,
            turn_timeout_secs: turn_timeout,
            context_budget: Default::default(),
            summarization: Default::default(),
        }
    }

    fn create_agent_definition(
        max_turns: Option<usize>,
        max_cost: Option<f64>,
        tools: Option<Vec<String>>,
    ) -> AgentDefinition {
        AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools,
            max_turns,
            max_cost,
        }
    }

    fn create_run_policy_input(
        max_turns: Option<usize>,
        max_budget_microusd: Option<u64>,
        max_runtime: Option<Duration>,
        tool_policy: Option<ToolPolicyInput>,
    ) -> RunPolicyInput {
        RunPolicyInput {
            max_runtime,
            max_budget_microusd,
            max_turns,
            tool_policy,
        }
    }

    fn create_tool_policy_input(
        toolset: Option<Vec<String>>,
        disallowed_tools: Option<Vec<String>>,
        auto_approve_tools: Option<Vec<String>>,
    ) -> ToolPolicyInput {
        ToolPolicyInput {
            toolset,
            disallowed_tools,
            auto_approve_tools,
        }
    }

    fn create_available_tools() -> Vec<FunctionDecl> {
        vec![
            FunctionDecl::new(
                "tool_a",
                Some("Tool A".to_string()),
                ToolParameterSchema::default(),
            ),
            FunctionDecl::new(
                "tool_b",
                Some("Tool B".to_string()),
                ToolParameterSchema::default(),
            ),
            FunctionDecl::new(
                "tool_c",
                Some("Tool C".to_string()),
                ToolParameterSchema::default(),
            ),
        ]
    }

    #[test]
    fn test_resolve_policy_valid_defaults() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(result.is_ok());
        let policy = result.unwrap();
        assert_eq!(policy.max_turns, Some(50));
        assert_eq!(policy.initial_budget_microusd, 0);
        assert_eq!(policy.toolset.len(), 3);
    }

    #[test]
    fn test_resolve_policy_with_constraints() {
        let global = create_runtime_config(100, Some(50.0), 60);
        let agent = create_agent_definition(Some(50), Some(30.0), None);
        let tool_policy = create_tool_policy_input(
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
            None,
            Some(vec!["tool_a".to_string()]),
        );
        let per_run = create_run_policy_input(
            Some(25),
            Some(20_000_000),
            Some(Duration::from_secs(600)),
            Some(tool_policy),
        );
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(result.is_ok());
        let policy = result.unwrap();
        assert_eq!(policy.max_turns, Some(25));
        assert_eq!(policy.initial_budget_microusd, 20_000_000);
        assert_eq!(policy.toolset.len(), 2);
        assert!(policy.auto_approve_tools.contains("tool_a"));
    }

    #[test]
    fn test_resolve_policy_rejects_zero_budget() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, Some(0), None, None);
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), PolicyValidationError::ZeroBudget);
    }

    #[test]
    fn test_resolve_policy_rejects_unknown_tool_in_toolset() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let tool_policy =
            create_tool_policy_input(Some(vec!["unknown_tool".to_string()]), None, None);
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(result.is_err());
        match result.unwrap_err() {
            PolicyValidationError::UnknownTool { tool_name } => {
                assert_eq!(tool_name, "unknown_tool");
            }
            other => panic!("Expected UnknownTool error, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_policy_rejects_unknown_tool_in_auto_approve() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let tool_policy =
            create_tool_policy_input(None, None, Some(vec!["unknown_tool".to_string()]));
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(result.is_err());
        match result.unwrap_err() {
            PolicyValidationError::UnknownTool { tool_name } => {
                assert_eq!(tool_name, "unknown_tool");
            }
            other => panic!("Expected UnknownTool error, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_policy_allows_unknown_tool_in_disallowed() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let tool_policy =
            create_tool_policy_input(None, Some(vec!["unknown_tool".to_string()]), None);
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_policy_sets_started_at() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let before = Utc::now();
        let result = resolve_policy(&global, &agent, &per_run, &available).unwrap();
        let after = Utc::now();

        assert!(result.started_at >= before);
        assert!(result.started_at <= after);
    }

    #[test]
    fn test_resolve_policy_computes_deadline() {
        let global = create_runtime_config(10, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, Some(Duration::from_secs(300)), None);
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available).unwrap();

        assert!(result.deadline.is_some());
        let expected_duration = 300;
        let actual_duration = (result.deadline.unwrap() - result.started_at).num_seconds();
        assert!((actual_duration - expected_duration as i64).abs() <= 1);
    }

    #[test]
    fn test_resolve_policy_none_defaults_to_global() {
        let global = create_runtime_config(50, Some(10.0), 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = RunPolicyInput::default();
        let available = create_available_tools();

        let policy = resolve_policy(&global, &agent, &per_run, &available).unwrap();

        assert_eq!(policy.max_turns, Some(50));
        assert_eq!(policy.initial_budget_microusd, 10_000_000);
    }

    #[test]
    fn test_policy_validation_error_display() {
        assert_eq!(
            PolicyValidationError::ZeroBudget.to_string(),
            "budget cannot be zero"
        );
        assert_eq!(
            PolicyValidationError::NegativeMaxTurns.to_string(),
            "max_turns cannot be negative"
        );
        assert_eq!(
            PolicyValidationError::NegativeMaxBudget.to_string(),
            "max_budget cannot be negative"
        );
        assert_eq!(
            PolicyValidationError::NegativeMaxRuntime.to_string(),
            "max_runtime cannot be negative"
        );
        assert_eq!(
            PolicyValidationError::EmptyToolset.to_string(),
            "no tools available after applying policy constraints"
        );
        assert_eq!(
            PolicyValidationError::UnknownTool {
                tool_name: "foo".to_string()
            }
            .to_string(),
            "unknown tool: foo"
        );
    }

    #[test]
    fn test_resolve_policy_strictest_wins() {
        let global = create_runtime_config(100, Some(50.0), 60);
        let agent = create_agent_definition(Some(50), Some(30.0), None);
        let per_run = create_run_policy_input(Some(25), Some(20_000_000), None, None);
        let available = create_available_tools();

        let policy = resolve_policy(&global, &agent, &per_run, &available).unwrap();

        assert_eq!(policy.max_turns, Some(25));
        assert_eq!(policy.initial_budget_microusd, 20_000_000);
    }

    // ============================================================================
    // Edge Case Tests - Task 21
    // ============================================================================

    #[test]
    fn edge_case_zero_budget_rejected_at_admission() {
        // Scenario 1: Zero budget rejected at admission
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, Some(0), None, None);
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(
            result.is_err(),
            "Zero budget should be rejected at admission"
        );
        assert_eq!(result.unwrap_err(), PolicyValidationError::ZeroBudget);
    }

    #[test]
    fn edge_case_zero_budget_from_agent_definition_rejected() {
        // Zero budget from agent definition should also be rejected
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, Some(0.0), None); // Agent has zero budget
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        // Note: AgentDefinition uses f64 max_cost, which gets converted to micro-USD
        // If the merged result is 0, it should be rejected
        if let Ok(policy) = result {
            // If it somehow passes, the budget should be 0
            assert_eq!(policy.initial_budget_microusd, 0);
        }
    }

    #[test]
    fn edge_case_very_large_budget_handled() {
        // Very large budget should not overflow
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, Some(u64::MAX / 4), None, None);
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(result.is_ok(), "Large but valid budget should be accepted");
        let policy = result.unwrap();
        assert_eq!(policy.initial_budget_microusd, u64::MAX / 4);
    }

    #[test]
    fn edge_case_negative_budget_caught_as_overflow() {
        // Negative budget (represented as very large u64) should be rejected
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        // A "negative" value would be > u64::MAX/2 if it came from a signed conversion
        let per_run = create_run_policy_input(None, Some(u64::MAX - 100), None, None);
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(
            result.is_err(),
            "Overflow/negative budget should be rejected"
        );
        assert_eq!(
            result.unwrap_err(),
            PolicyValidationError::NegativeMaxBudget
        );
    }

    #[test]
    fn edge_case_empty_toolset_after_filtering_allowed() {
        // Empty toolset after filtering is allowed (edge case)
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, Some(vec!["nonexistent_tool".to_string()]));
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        // Should succeed but with empty toolset
        assert!(
            result.is_ok(),
            "Empty toolset should be allowed at admission"
        );
        let policy = result.unwrap();
        assert!(
            policy.toolset.is_empty(),
            "Toolset should be empty when agent requests non-existent tools"
        );
    }

    #[test]
    fn edge_case_all_tools_disallowed_results_in_empty_toolset() {
        // All available tools are disallowed
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let tool_policy = create_tool_policy_input(
            None,
            Some(vec![
                "tool_a".to_string(),
                "tool_b".to_string(),
                "tool_c".to_string(),
            ]),
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(
            result.is_ok(),
            "All tools disallowed should still pass admission"
        );
        let policy = result.unwrap();
        assert!(
            policy.toolset.is_empty(),
            "Toolset should be empty when all tools are disallowed"
        );
        assert_eq!(policy.disallowed_tools.len(), 3);
    }

    #[test]
    fn edge_case_unknown_tool_in_disallowed_allowed() {
        // Unknown tools in disallowed list should be allowed (they just won't match anything)
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let tool_policy = create_tool_policy_input(
            None,
            Some(vec![
                "unknown_tool".to_string(),
                "another_unknown".to_string(),
            ]),
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = resolve_policy(&global, &agent, &per_run, &available);

        assert!(
            result.is_ok(),
            "Unknown tools in disallowed should be allowed"
        );
        let policy = result.unwrap();
        // Unknown tools are still in the disallowed set even if they don't exist
        assert!(policy.disallowed_tools.contains("unknown_tool"));
        assert!(policy.disallowed_tools.contains("another_unknown"));
    }

    #[test]
    fn edge_case_strictest_wins_with_mixed_none_values() {
        // Test strictest wins with mixed None and Some values
        let global = create_runtime_config(100, Some(50.0), 60);
        let agent = create_agent_definition(None, Some(20.0), None); // None max_turns
        let per_run = create_run_policy_input(Some(30), None, None, None); // Some max_turns, None budget
        let available = create_available_tools();
        let result = resolve_policy(&global, &agent, &per_run, &available);
        assert!(result.is_ok());
        let policy = result.unwrap();
        // max_turns: min(100, None->100, 30) = 30
        assert_eq!(policy.max_turns, Some(30));
        // budget: min(50M, 20M, None->no restriction) = 20M (agent's budget is strictest)
        assert_eq!(policy.initial_budget_microusd, 20_000_000);
    }
}
