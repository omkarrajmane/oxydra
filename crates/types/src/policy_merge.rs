//! Policy merge logic for combining global, agent-level, and per-run policy constraints.
//!
//! This module implements strictest-wins semantics for merging policy constraints
//! across three layers:
//! - Global runtime configuration (RuntimeConfig)
//! - Agent definition constraints (AgentDefinition)
//! - Per-run policy overrides (RunPolicyInput)
//!
//! The merge rules follow the principle that the most restrictive constraint wins.

use std::collections::HashSet;

use chrono::Utc;

use crate::{
    AgentDefinition, EffectiveRunPolicy, FunctionDecl, RolloutMode, RunPolicyInput, RuntimeConfig,
    ToolPolicyInput,
};

/// Merges global, agent-level, and per-run policy constraints using strictest-wins semantics.
///
/// # Merge Rules
///
/// ## Numeric Limits (max_turns, max_budget)
/// - The minimum value across all layers wins
/// - `None` means "no restriction" and is ignored in the min calculation
/// - If all layers are `None`, falls back to global default
///
/// ## Runtime Limit (max_runtime)
/// - Calculated as: min(global_turn_timeout * effective_max_turns, per_run_max_runtime)
/// - This ensures the runtime limit accounts for per-turn timeouts
///
/// ## Toolset (allowed tools)
/// - Intersection of: available_tools ∩ agent_allowlist ∩ per_run_toolset
/// - Empty per_run_toolset means "all tools" (no restriction)
/// - Empty agent_allowlist means "all tools" (no restriction)
///
/// ## Disallowed Tools
/// - Union of all disallowed tool sets
/// - Disallowed always wins - if a tool is in this set, it's blocked regardless of allowlists
///
/// ## Auto-approve Tools
/// - Intersection of: allowed_tools ∩ per_run_auto_approve
/// - Only tools that are both allowed AND marked for auto-approval are included
///
/// # Arguments
///
/// * `global` - Global runtime configuration with baseline limits
/// * `agent` - Agent definition with agent-specific constraints
/// * `per_run` - Per-run policy overrides
/// * `available_tools` - The complete set of tools available in the system
///
/// # Returns
///
/// The merged effective run policy with all constraints resolved.
pub fn merge_policy(
    global: &RuntimeConfig,
    agent: &AgentDefinition,
    per_run: &RunPolicyInput,
    available_tools: &[FunctionDecl],
) -> EffectiveRunPolicy {
    let max_turns = merge_max_turns(global, agent, per_run);
    let initial_budget = merge_max_budget(global, agent, per_run);
    let deadline = merge_deadline(global, per_run, max_turns);

    let (toolset, disallowed_tools, auto_approve_tools) = merge_tool_policies(
        available_tools,
        agent.tools.as_ref(),
        per_run.tool_policy.as_ref(),
    );

    EffectiveRunPolicy {
        started_at: Utc::now(),
        deadline,
        initial_budget_microusd: initial_budget,
        remaining_budget_microusd: initial_budget,
        toolset,
        disallowed_tools,
        auto_approve_tools,
        parent_run_id: None,
        max_turns: max_turns.map(|v| v as u32),
        rollout_mode: RolloutMode::Enforce,
    }
}

fn merge_max_turns(
    global: &RuntimeConfig,
    agent: &AgentDefinition,
    per_run: &RunPolicyInput,
) -> Option<usize> {
    let mut result = Some(global.max_turns);

    if let Some(agent_max) = agent.max_turns {
        result = result.map(|r| r.min(agent_max));
    }

    if let Some(per_run_max) = per_run.max_turns {
        result = result.map(|r| r.min(per_run_max));
    }

    result
}

fn merge_max_budget(
    global: &RuntimeConfig,
    agent: &AgentDefinition,
    per_run: &RunPolicyInput,
) -> u64 {
    let global_budget = global.max_cost.map(|c| (c * 1_000_000.0) as u64);
    let agent_budget = agent.max_cost.map(|c| (c * 1_000_000.0) as u64);
    let per_run_budget = per_run.max_budget_microusd;

    let limits: Vec<u64> = [global_budget, agent_budget, per_run_budget]
        .into_iter()
        .flatten()
        .collect();

    if limits.is_empty() {
        0
    } else {
        limits.into_iter().min().unwrap()
    }
}

fn merge_deadline(
    global: &RuntimeConfig,
    per_run: &RunPolicyInput,
    effective_max_turns: Option<usize>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let max_turns = effective_max_turns.unwrap_or(global.max_turns);
    let global_runtime_secs = global.turn_timeout_secs.saturating_mul(max_turns as u64);

    let runtime_secs = match per_run.max_runtime {
        Some(per_run_max) => {
            let per_run_secs = per_run_max.as_secs();
            global_runtime_secs.min(per_run_secs)
        }
        None => global_runtime_secs,
    };

    if runtime_secs > 0 {
        Some(Utc::now() + chrono::Duration::seconds(runtime_secs as i64))
    } else {
        None
    }
}

fn merge_tool_policies(
    available_tools: &[FunctionDecl],
    agent_allowlist: Option<&Vec<String>>,
    per_run_tool_policy: Option<&ToolPolicyInput>,
) -> (Vec<FunctionDecl>, HashSet<String>, HashSet<String>) {
    let mut toolset: Vec<FunctionDecl> = available_tools.to_vec();

    if let Some(allowlist) = agent_allowlist
        && !allowlist.is_empty()
    {
        toolset.retain(|tool| allowlist.contains(&tool.name));
    }

    let mut disallowed_tools = HashSet::new();
    let mut auto_approve_tools = HashSet::new();

    if let Some(tool_policy) = per_run_tool_policy {
        if let Some(ref per_run_toolset) = tool_policy.toolset
            && !per_run_toolset.is_empty()
        {
            toolset.retain(|tool| per_run_toolset.contains(&tool.name));
        }

        if let Some(ref disallowed) = tool_policy.disallowed_tools {
            disallowed_tools.extend(disallowed.iter().cloned());
        }

        if let Some(ref auto_approve) = tool_policy.auto_approve_tools
            && !auto_approve.is_empty()
        {
            let allowed_names: HashSet<String> = toolset.iter().map(|t| t.name.clone()).collect();
            auto_approve_tools = allowed_names
                .intersection(&auto_approve.iter().cloned().collect())
                .cloned()
                .collect();
        }
    }

    let final_toolset: Vec<FunctionDecl> = toolset
        .into_iter()
        .filter(|tool| !disallowed_tools.contains(&tool.name))
        .collect();

    let final_auto_approve: HashSet<String> = auto_approve_tools
        .intersection(&final_toolset.iter().map(|t| t.name.clone()).collect())
        .cloned()
        .collect();

    (final_toolset, disallowed_tools, final_auto_approve)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

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
                crate::ToolParameterSchema::default(),
            ),
            FunctionDecl::new(
                "tool_b",
                Some("Tool B".to_string()),
                crate::ToolParameterSchema::default(),
            ),
            FunctionDecl::new(
                "tool_c",
                Some("Tool C".to_string()),
                crate::ToolParameterSchema::default(),
            ),
            FunctionDecl::new(
                "tool_d",
                Some("Tool D".to_string()),
                crate::ToolParameterSchema::default(),
            ),
            FunctionDecl::new(
                "tool_e",
                Some("Tool E".to_string()),
                crate::ToolParameterSchema::default(),
            ),
        ]
    }

    #[test]
    fn test_merge_max_turns_global_only() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.max_turns, Some(50));
    }

    #[test]
    fn test_merge_max_turns_agent_more_restrictive() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(Some(30), None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.max_turns, Some(30));
    }

    #[test]
    fn test_merge_max_turns_per_run_most_restrictive() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(Some(30), None, None);
        let per_run = create_run_policy_input(Some(10), None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.max_turns, Some(10));
    }

    #[test]
    fn test_merge_max_turns_global_most_restrictive() {
        let global = create_runtime_config(20, None, 30);
        let agent = create_agent_definition(Some(30), None, None);
        let per_run = create_run_policy_input(Some(50), None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.max_turns, Some(20));
    }

    #[test]
    fn test_merge_max_budget_no_limits() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.initial_budget_microusd, 0);
    }

    #[test]
    fn test_merge_max_budget_global_only() {
        let global = create_runtime_config(50, Some(10.0), 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.initial_budget_microusd, 10_000_000);
    }

    #[test]
    fn test_merge_max_budget_strictest_wins() {
        let global = create_runtime_config(50, Some(10.0), 30);
        let agent = create_agent_definition(None, Some(5.0), None);
        let per_run = create_run_policy_input(None, Some(8_000_000), None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.initial_budget_microusd, 5_000_000);
    }

    #[test]
    fn test_merge_max_budget_per_run_strictest() {
        let global = create_runtime_config(50, Some(10.0), 30);
        let agent = create_agent_definition(None, Some(8.0), None);
        let per_run = create_run_policy_input(None, Some(3_000_000), None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.initial_budget_microusd, 3_000_000);
    }

    #[test]
    fn test_merge_deadline_calculated_from_turn_timeout() {
        let global = create_runtime_config(10, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert!(result.deadline.is_some());
        let expected_duration = 30 * 10;
        let actual_duration = (result.deadline.unwrap() - result.started_at).num_seconds();
        assert!((actual_duration - expected_duration as i64).abs() <= 1);
    }

    #[test]
    fn test_merge_deadline_per_run_more_restrictive() {
        let global = create_runtime_config(10, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, Some(Duration::from_secs(120)), None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert!(result.deadline.is_some());
        let actual_duration = (result.deadline.unwrap() - result.started_at).num_seconds();
        assert!((actual_duration - 120).abs() <= 1);
    }

    #[test]
    fn test_merge_toolset_all_tools_available() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.toolset.len(), 5);
    }

    #[test]
    fn test_merge_toolset_agent_allowlist_restricts() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(
            None,
            None,
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
        );
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.toolset.len(), 2);
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
        assert!(result.toolset.iter().any(|t| t.name == "tool_b"));
    }

    #[test]
    fn test_merge_toolset_per_run_restricts() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let tool_policy = create_tool_policy_input(
            Some(vec!["tool_a".to_string(), "tool_c".to_string()]),
            None,
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.toolset.len(), 2);
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
        assert!(result.toolset.iter().any(|t| t.name == "tool_c"));
    }

    #[test]
    fn test_merge_toolset_intersection_all_layers() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(
            None,
            None,
            Some(vec![
                "tool_a".to_string(),
                "tool_b".to_string(),
                "tool_c".to_string(),
            ]),
        );
        let tool_policy = create_tool_policy_input(
            Some(vec![
                "tool_a".to_string(),
                "tool_c".to_string(),
                "tool_d".to_string(),
            ]),
            None,
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.toolset.len(), 2);
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
        assert!(result.toolset.iter().any(|t| t.name == "tool_c"));
    }

    #[test]
    fn test_merge_toolset_empty_agent_allowlist_means_all() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, Some(vec![]));
        let tool_policy = create_tool_policy_input(Some(vec!["tool_a".to_string()]), None, None);
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.toolset.len(), 1);
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
    }

    #[test]
    fn test_merge_toolset_empty_per_run_toolset_means_all() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(
            None,
            None,
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
        );
        let tool_policy = create_tool_policy_input(Some(vec![]), None, None);
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.toolset.len(), 2);
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
        assert!(result.toolset.iter().any(|t| t.name == "tool_b"));
    }

    #[test]
    fn test_merge_disallowed_tools_empty_by_default() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert!(result.disallowed_tools.is_empty());
    }

    #[test]
    fn test_merge_disallowed_tools_from_per_run() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let tool_policy = create_tool_policy_input(
            None,
            Some(vec!["tool_b".to_string(), "tool_d".to_string()]),
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.disallowed_tools.len(), 2);
        assert!(result.disallowed_tools.contains("tool_b"));
        assert!(result.disallowed_tools.contains("tool_d"));
    }

    #[test]
    fn test_disallowed_tools_always_wins() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(
            None,
            None,
            Some(vec![
                "tool_a".to_string(),
                "tool_b".to_string(),
                "tool_c".to_string(),
            ]),
        );
        let tool_policy = create_tool_policy_input(
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
            Some(vec!["tool_b".to_string()]),
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.toolset.len(), 1);
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
        assert!(!result.toolset.iter().any(|t| t.name == "tool_b"));
    }

    #[test]
    fn test_merge_auto_approve_empty_by_default() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert!(result.auto_approve_tools.is_empty());
    }

    #[test]
    fn test_merge_auto_approve_intersection_with_allowed() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(
            None,
            None,
            Some(vec![
                "tool_a".to_string(),
                "tool_b".to_string(),
                "tool_c".to_string(),
            ]),
        );
        let tool_policy = create_tool_policy_input(
            None,
            None,
            Some(vec![
                "tool_b".to_string(),
                "tool_c".to_string(),
                "tool_d".to_string(),
            ]),
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.auto_approve_tools.len(), 2);
        assert!(result.auto_approve_tools.contains("tool_b"));
        assert!(result.auto_approve_tools.contains("tool_c"));
    }

    #[test]
    fn test_merge_auto_approve_respects_disallowed() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(
            None,
            None,
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
        );
        let tool_policy = create_tool_policy_input(
            None,
            Some(vec!["tool_b".to_string()]),
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
        assert!(!result.toolset.iter().any(|t| t.name == "tool_b"));
        assert!(result.auto_approve_tools.contains("tool_a"));
        assert!(!result.auto_approve_tools.contains("tool_b"));
    }

    #[test]
    fn test_merge_policy_full_integration() {
        let global = create_runtime_config(100, Some(50.0), 60);
        let agent = create_agent_definition(
            Some(50),
            Some(30.0),
            Some(vec![
                "tool_a".to_string(),
                "tool_b".to_string(),
                "tool_c".to_string(),
            ]),
        );
        let tool_policy = create_tool_policy_input(
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
            Some(vec!["tool_b".to_string()]),
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
        );
        let per_run = create_run_policy_input(
            Some(25),
            Some(20_000_000),
            Some(Duration::from_secs(600)),
            Some(tool_policy),
        );
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.max_turns, Some(25));
        assert_eq!(result.initial_budget_microusd, 20_000_000);
        assert!(result.deadline.is_some());

        assert_eq!(result.toolset.len(), 1);
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));

        assert_eq!(result.disallowed_tools.len(), 1);
        assert!(result.disallowed_tools.contains("tool_b"));

        assert_eq!(result.auto_approve_tools.len(), 1);
        assert!(result.auto_approve_tools.contains("tool_a"));
    }

    #[test]
    fn test_merge_policy_none_means_no_restriction() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.max_turns, Some(50));
        assert_eq!(result.initial_budget_microusd, 0);
        assert!(result.deadline.is_some());
        assert_eq!(result.toolset.len(), 5);
        assert!(result.disallowed_tools.is_empty());
        assert!(result.auto_approve_tools.is_empty());
    }

    #[test]
    fn test_merge_policy_empty_toolsets() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, Some(vec![]));
        let tool_policy = create_tool_policy_input(Some(vec![]), None, None);
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.toolset.len(), 5);
    }

    #[test]
    fn test_merge_max_turns_zero_edge_case() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(Some(0), None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.max_turns, Some(0));
    }

    #[test]
    fn test_merge_max_budget_zero() {
        let global = create_runtime_config(50, Some(10.0), 30);
        let agent = create_agent_definition(None, Some(0.0), None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(result.initial_budget_microusd, 0);
    }

    #[test]
    fn test_merge_disallowed_tools_filters_all_layers() {
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, Some(vec!["tool_a".to_string()]));
        let tool_policy = create_tool_policy_input(None, Some(vec!["tool_a".to_string()]), None);
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);
        assert!(result.disallowed_tools.contains("tool_a"));
    }

    // ============================================================================
    // Edge Case Tests - Task 21
    // ============================================================================

    #[test]
    fn edge_case_empty_toolset_means_all_tools_allowed() {
        // Scenario 6: Empty toolset = all tools allowed
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, Some(vec![])); // Empty agent toolset
        let tool_policy = create_tool_policy_input(Some(vec![]), None, None); // Empty per-run toolset
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        // Empty toolsets at all layers means all available tools are allowed
        assert_eq!(
            result.toolset.len(),
            5,
            "Empty toolsets should allow all tools"
        );
    }

    #[test]
    fn edge_case_disallowed_wins_over_allowlist() {
        // Scenario 7: Disallowed always wins over allowlists
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(
            None,
            None,
            Some(vec![
                "tool_a".to_string(),
                "tool_b".to_string(),
                "tool_c".to_string(),
            ]),
        );
        // Tool is in allowlist but also in disallowed
        let tool_policy = create_tool_policy_input(
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
            Some(vec!["tool_b".to_string()]), // tool_b is disallowed
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        // Disallowed wins - tool_b should not be in final toolset
        assert_eq!(result.toolset.len(), 1, "Only tool_a should be allowed");
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
        assert!(!result.toolset.iter().any(|t| t.name == "tool_b"));
        assert!(result.disallowed_tools.contains("tool_b"));
    }

    #[test]
    fn edge_case_disallowed_wins_even_when_only_tool() {
        // Extreme case: only allowed tool is also disallowed
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(None, None, Some(vec!["tool_a".to_string()]));
        let tool_policy = create_tool_policy_input(
            Some(vec!["tool_a".to_string()]), // Only tool_a is allowed
            Some(vec!["tool_a".to_string()]), // But tool_a is also disallowed
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        // Disallowed wins - no tools should be available
        assert!(
            result.toolset.is_empty(),
            "Disallowed should win even when it's the only tool"
        );
        assert!(result.disallowed_tools.contains("tool_a"));
    }

    #[test]
    fn edge_case_parent_denies_child_allows_is_denied() {
        // Scenario 4: Parent denies + child allows = denied (disallowed wins)
        // This simulates the delegation scenario where parent policy has disallowed tools
        // and child agent tries to allow them
        let global = create_runtime_config(50, None, 30);
        // Parent (global) disallows tool_b
        let agent = create_agent_definition(
            None,
            None,
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]), // Child tries to allow both
        );
        let tool_policy = create_tool_policy_input(
            None,
            Some(vec!["tool_b".to_string()]), // Parent disallows tool_b
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        // Parent's disallow wins - tool_b should be denied
        assert_eq!(result.toolset.len(), 1);
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
        assert!(!result.toolset.iter().any(|t| t.name == "tool_b"));
    }

    #[test]
    fn edge_case_multiple_disallowed_layers_union() {
        // Test that disallowed tools from multiple layers are unioned
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(
            None,
            None,
            Some(vec![
                "tool_a".to_string(),
                "tool_b".to_string(),
                "tool_c".to_string(),
            ]),
        );
        let tool_policy = create_tool_policy_input(
            None,
            Some(vec!["tool_b".to_string(), "tool_d".to_string()]), // Disallow tool_b and tool_d
            None,
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        // Both tool_b (in agent allowlist) and tool_d (not in agent allowlist) should be disallowed
        assert!(result.disallowed_tools.contains("tool_b"));
        assert!(result.disallowed_tools.contains("tool_d"));
        // Only tool_a and tool_c should be allowed
        assert_eq!(result.toolset.len(), 2);
        assert!(result.toolset.iter().any(|t| t.name == "tool_a"));
        assert!(result.toolset.iter().any(|t| t.name == "tool_c"));
    }

    #[test]
    fn edge_case_auto_approve_respects_disallowed() {
        // Auto-approve should not include disallowed tools
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(
            None,
            None,
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]),
        );
        let tool_policy = create_tool_policy_input(
            None,
            Some(vec!["tool_b".to_string()]), // Disallow tool_b
            Some(vec!["tool_a".to_string(), "tool_b".to_string()]), // Try to auto-approve both
        );
        let per_run = create_run_policy_input(None, None, None, Some(tool_policy));
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        // Only tool_a should be auto-approved (tool_b is disallowed)
        assert_eq!(result.auto_approve_tools.len(), 1);
        assert!(result.auto_approve_tools.contains("tool_a"));
        assert!(!result.auto_approve_tools.contains("tool_b"));
    }

    #[test]
    fn edge_case_strictest_wins_across_all_numeric_limits() {
        // Comprehensive test that strictest wins across all numeric fields
        let global = create_runtime_config(100, Some(50.0), 60);
        let agent = create_agent_definition(Some(50), Some(30.0), None);
        let per_run = create_run_policy_input(Some(25), Some(20_000_000), None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        // All should be the minimum (strictest)
        assert_eq!(result.max_turns, Some(25), "max_turns should be minimum");
        assert_eq!(
            result.initial_budget_microusd, 20_000_000,
            "budget should be minimum in micro-USD"
        );
    }

    #[test]
    fn edge_case_zero_max_turns_is_valid() {
        // Zero max_turns is valid and means no turns allowed
        let global = create_runtime_config(50, None, 30);
        let agent = create_agent_definition(Some(0), None, None);
        let per_run = create_run_policy_input(None, None, None, None);
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        assert_eq!(
            result.max_turns,
            Some(0),
            "Zero max_turns should be preserved"
        );
    }

    #[test]
    fn edge_case_none_values_mean_no_restriction() {
        // None values at any layer should not restrict
        let global = create_runtime_config(50, Some(10.0), 30);
        let agent = create_agent_definition(None, None, None); // All None
        let per_run = create_run_policy_input(None, None, None, None); // All None
        let available = create_available_tools();

        let result = merge_policy(&global, &agent, &per_run, &available);

        // Should use global values
        assert_eq!(result.max_turns, Some(50));
        assert_eq!(result.initial_budget_microusd, 10_000_000);
        assert_eq!(result.toolset.len(), 5); // All tools available
        assert_eq!(result.toolset.len(), 5); // All tools available
    }
}
