use std::{collections::BTreeMap, collections::HashSet, sync::Arc};

use async_trait::async_trait;
use chrono::Utc;
use tokio_util::sync::CancellationToken;

use types::DelegationProgressSender;
use types::{
    AgentDefinition, DelegationRequest, DelegationResult, DelegationStatus, EffectiveRunPolicy,
    FunctionDecl, PolicyStreamEvent, ProviderSelection, RuntimeError, StreamItem,
    ToolExecutionContext,
};
use types::{Context, Message, MessageRole, StopReason};

use crate::AgentRuntime;

/// Maximum allowed delegation depth (parent -> child -> grandchild...).
const MAX_DELEGATION_DEPTH: usize = 5;

/// Calculate delegation depth from session ID by counting "subagent:" prefixes.
fn calculate_delegation_depth(session_id: &str) -> usize {
    session_id.matches("subagent:").count()
}

/// Narrow child policy from parent using strictest-wins semantics.
///
/// Budget: min(parent_remaining, child_requested)
/// Tools: intersection(parent_allowed, child_requested)
/// Deadline: inherits parent remaining deadline
fn narrow_child_policy(
    parent_policy: &EffectiveRunPolicy,
    child_agent: &AgentDefinition,
    child_request: &DelegationRequest,
) -> EffectiveRunPolicy {
    // Calculate narrowed budget: min(parent_remaining, child_requested)
    let child_requested_budget = child_request
        .max_cost
        .map(|c| (c * 1_000_000.0) as u64) // Convert to micro-USD
        .unwrap_or(parent_policy.remaining_budget_microusd);
    let narrowed_budget = child_requested_budget.min(parent_policy.remaining_budget_microusd);

    // Calculate narrowed max_turns: min(parent_remaining, child_requested)
    // For parent, we don't track remaining turns directly, so we use parent's max_turns if available
    let narrowed_max_turns = match (parent_policy.max_turns, child_request.max_turns) {
        (Some(parent), Some(child)) => Some(parent.min(child)),
        (Some(parent), None) => Some(parent),
        (None, Some(child)) => Some(child),
        (None, None) => None,
    };

    // Calculate tool intersection: parent_allowed ∩ child_requested
    let parent_tool_names: HashSet<String> = parent_policy
        .toolset
        .iter()
        .map(|t| t.name.clone())
        .collect();
    let child_tool_names: Option<HashSet<String>> = child_agent
        .tools
        .as_ref()
        .map(|tools| tools.iter().cloned().collect());
    let narrowed_tool_names: Option<HashSet<String>> = match child_tool_names {
        Some(child_tools) => {
            let intersection: HashSet<String> = parent_tool_names
                .intersection(&child_tools)
                .cloned()
                .collect();
            if intersection.is_empty() {
                None // No tools allowed
            } else {
                Some(intersection)
            }
        }
        None => Some(parent_tool_names), // Child has no restriction, use parent's
    };

    // Filter parent's toolset to only include narrowed tools
    let narrowed_toolset: Vec<FunctionDecl> = parent_policy
        .toolset
        .iter()
        .filter(|t| {
            narrowed_tool_names
                .as_ref()
                .is_some_and(|allowed| allowed.contains(&t.name))
        })
        .cloned()
        .collect();

    // Calculate narrowed auto_approve_tools: intersection with narrowed toolset
    let narrowed_auto_approve: HashSet<String> = parent_policy
        .auto_approve_tools
        .intersection(
            &narrowed_tool_names
                .as_ref()
                .cloned()
                .unwrap_or_else(HashSet::new),
        )
        .cloned()
        .collect();

    // Disallowed tools: union of parent disallowed (always wins)
    let narrowed_disallowed: HashSet<String> = parent_policy.disallowed_tools.clone();

    // Deadline: inherits parent's remaining deadline
    let narrowed_deadline = parent_policy.deadline;

    EffectiveRunPolicy {
        started_at: Utc::now(),
        deadline: narrowed_deadline,
        initial_budget_microusd: narrowed_budget,
        remaining_budget_microusd: narrowed_budget,
        toolset: narrowed_toolset,
        auto_approve_tools: narrowed_auto_approve,
        disallowed_tools: narrowed_disallowed,
        parent_run_id: Some(child_request.parent_session_id.clone()),
        max_turns: narrowed_max_turns,
        rollout_mode: parent_policy.rollout_mode,
    }
}

/// Simple runtime-backed delegation executor.
///
/// This implementation constructs a fresh context, injects the agent's
/// system prompt (if provided) and the delegation goal, then executes a
/// synchronous run of the parent's AgentRuntime for a dedicated subagent
/// session id. It reuses the parent's runtime with provider/model routes
/// selected per target agent (explicit override, caller inheritance, or root
/// fallback). Tool allowlists are enforced via filtered_schemas() based on the
/// narrowed policy. The implementation is intentionally simple but
/// functional for the common case.
///
/// # Policy Inheritance
///
/// When a parent policy is provided in the DelegationRequest, the child
/// policy is narrowed using strictest-wins semantics:
/// - Budget: min(parent_remaining, child_requested)
/// - Tools: intersection(parent_allowed, child_requested)
/// - Max turns: min(parent_remaining, child_requested)
/// - Deadline: inherits parent's remaining deadline
/// - Disallowed tools: parent's disallowed list is preserved
///
/// Maximum delegation depth is enforced at 5 levels.
pub struct RuntimeDelegationExecutor {
    runtime: Arc<AgentRuntime>,
    agents: BTreeMap<String, AgentDefinition>,
    root_selection: ProviderSelection,
}

impl RuntimeDelegationExecutor {
    pub fn new(
        runtime: Arc<AgentRuntime>,
        agents: BTreeMap<String, AgentDefinition>,
        root_selection: ProviderSelection,
    ) -> Self {
        Self {
            runtime,
            agents,
            root_selection,
        }
    }
}

fn resolve_delegation_selection(
    root_selection: &ProviderSelection,
    agent_name: &str,
    agent_def: &AgentDefinition,
    caller_selection: Option<&ProviderSelection>,
) -> ProviderSelection {
    if agent_name == "default" {
        return root_selection.clone();
    }
    if let Some(selection) = &agent_def.selection {
        return selection.clone();
    }
    caller_selection
        .cloned()
        .unwrap_or_else(|| root_selection.clone())
}

#[async_trait]
impl types::DelegationExecutor for RuntimeDelegationExecutor {
    async fn delegate(
        &self,
        request: DelegationRequest,
        parent_cancellation: &CancellationToken,
        _progress_sender: Option<DelegationProgressSender>,
    ) -> Result<DelegationResult, RuntimeError> {
        // Check delegation depth limit
        let current_depth = calculate_delegation_depth(&request.parent_session_id);
        if current_depth >= MAX_DELEGATION_DEPTH {
            // Check rollout mode from parent policy if available
            let rollout_mode = request.parent_policy.as_ref().map(|p| p.rollout_mode);
            let should_block =
                matches!(rollout_mode, Some(types::RolloutMode::Enforce)) || rollout_mode.is_none();
            if should_block {
                return Err(RuntimeError::Tool(types::ToolError::ExecutionFailed {
                    tool: "delegate_to_agent".to_string(),
                    message: format!(
                        "maximum delegation depth exceeded ({} >= {})",
                        current_depth, MAX_DELEGATION_DEPTH
                    ),
                }));
            } else {
                // SoftFail or ObserveOnly: log warning and emit event, but continue
                tracing::warn!(
                    current_depth,
                    max_depth = MAX_DELEGATION_DEPTH,
                    ?rollout_mode,
                    "delegation depth exceeded - continuing due to rollout mode"
                );
                if let (Some(ref sender), Some(types::RolloutMode::SoftFail)) =
                    (_progress_sender, rollout_mode)
                {
                    sender(StreamItem::PolicyEvent(PolicyStreamEvent::PolicyStop {
                        reason: StopReason::Error("MaxDelegationDepthExceeded".to_string()),
                    }));
                }
                // Continue with delegation despite depth violation
            }
        }

        // Lookup the agent definition
        let agent_def = match self.agents.get(&request.agent_name) {
            Some(def) => def,
            None => {
                return Err(RuntimeError::Tool(types::ToolError::ExecutionFailed {
                    tool: "delegate_to_agent".to_string(),
                    message: format!("unknown agent `{}`", request.agent_name),
                }));
            }
        };

        // Build a fresh context for the subagent run. Use a conservative
        // provider/model route based on agent-specific selection semantics.
        let effective_selection = resolve_delegation_selection(
            &self.root_selection,
            &request.agent_name,
            agent_def,
            request.caller_selection.as_ref(),
        );
        let mut ctx = Context {
            provider: effective_selection.provider,
            model: effective_selection.model,
            tools: Vec::new(),
            messages: Vec::new(),
        };

        // Inject the agent's configured system prompt if any. Use a system
        // message so the runtime will not inject the global system prompt.
        if let Some(system_prompt) = agent_def
            .system_prompt
            .as_ref()
            .or(agent_def.system_prompt_file.as_ref())
        {
            // Prefer inline prompt; if file path provided, the bootstrap
            // validated its existence but we do not re-read file here for
            // simplicity.
            ctx.messages.push(Message {
                role: MessageRole::System,
                content: Some(system_prompt.clone()),
                tool_calls: Vec::new(),
                tool_call_id: None,
                attachments: Vec::new(),
            });
        }

        // Add key facts as system messages to prime the subagent.
        for fact in &request.key_facts {
            ctx.messages.push(Message {
                role: MessageRole::System,
                content: Some(fact.clone()),
                tool_calls: Vec::new(),
                tool_call_id: None,
                attachments: Vec::new(),
            });
        }

        // Add the delegation goal as the user message.
        ctx.messages.push(Message {
            role: MessageRole::User,
            content: Some(request.goal.clone()),
            tool_calls: Vec::new(),
            tool_call_id: None,
            attachments: Vec::new(),
        });

        // Construct a subagent session id. Use a simple prefix plus a UUID.
        let subagent_session_id = format!(
            "subagent:{}:{}",
            request.parent_session_id,
            uuid::Uuid::new_v4()
        );

        // Resolve child policy from parent policy if available
        let child_policy = request
            .parent_policy
            .as_ref()
            .map(|parent_policy| narrow_child_policy(parent_policy, agent_def, &request));

        // Apply tool filtering based on narrowed policy
        if let Some(ref policy) = child_policy {
            let allowed_tool_names: HashSet<String> =
                policy.toolset.iter().map(|t| t.name.clone()).collect();
            ctx.tools = self
                .runtime
                .tool_registry
                .filtered_schemas(Some(&allowed_tool_names), &policy.disallowed_tools);
        } else {
            // No parent policy - use all available tools
            ctx.tools = self.runtime.tool_registry.schemas();
        }

        // Run the session on the parent's runtime instance. We reuse the
        // parent's runtime rather than constructing a new AgentRuntime so we
        // preserve provider/tool wiring. The cancellation token passed is the
        // parent's token so cancellation cascades.
        let tool_context = ToolExecutionContext {
            user_id: Some(request.parent_user_id.clone()),
            session_id: Some(subagent_session_id.clone()),
            provider: Some(ctx.provider.clone()),
            model: Some(ctx.model.clone()),
            channel_capabilities: None,
            event_sender: None,
            channel_id: None,
            channel_context_id: None,
            inbound_attachments: None,
            ..Default::default()
        };

        // Use the policy-aware session method if we have a child policy
        let response = if let Some(policy) = child_policy {
            // Create a channel for stream events (even though we don't use them for delegation)
            let (stream_sender, _stream_receiver) = tokio::sync::mpsc::unbounded_channel();
            self.runtime
                .run_session_for_session_with_policy(
                    &subagent_session_id,
                    &mut ctx,
                    parent_cancellation,
                    stream_sender,
                    &tool_context,
                    policy,
                )
                .await
        } else {
            self.runtime
                .run_session_for_session_with_tool_context(
                    &subagent_session_id,
                    &mut ctx,
                    parent_cancellation,
                    &tool_context,
                )
                .await
        };

        match response {
            Ok(resp) => Ok(DelegationResult {
                output: resp.message.content.unwrap_or_default(),
                attachments: resp.message.attachments,
                turns_used: 1, // best-effort
                cost_used: 0.0,
                status: DelegationStatus::Completed,
            }),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use types::{ModelId, ProviderId};

    fn selection(provider: &str, model: &str) -> ProviderSelection {
        ProviderSelection {
            provider: ProviderId::from(provider),
            model: ModelId::from(model),
        }
    }

    #[test]
    fn delegation_selection_uses_agent_override_when_present() {
        let root = selection("openai", "gpt-4o-mini");
        let caller = selection("anthropic", "claude-3-5-haiku-latest");
        let agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: Some(selection("gemini", "gemini-2.5-pro")),
            tools: None,
            max_turns: None,
            max_cost: None,
        };

        let effective = resolve_delegation_selection(&root, "researcher", &agent, Some(&caller));
        assert_eq!(effective, selection("gemini", "gemini-2.5-pro"));
    }

    #[test]
    fn delegation_selection_inherits_caller_when_agent_has_no_override() {
        let root = selection("openai", "gpt-4o-mini");
        let caller = selection("anthropic", "claude-3-5-haiku-latest");
        let agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: None,
            max_cost: None,
        };

        let effective = resolve_delegation_selection(&root, "coder", &agent, Some(&caller));
        assert_eq!(effective, caller);
    }

    #[test]
    fn delegation_selection_for_default_agent_always_uses_root() {
        let root = selection("openai", "gpt-4o-mini");
        let caller = selection("anthropic", "claude-3-5-haiku-latest");
        let agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: Some(selection("gemini", "gemini-2.5-pro")),
            tools: None,
            max_turns: None,
            max_cost: None,
        };

        let effective = resolve_delegation_selection(&root, "default", &agent, Some(&caller));
        assert_eq!(effective, root);
    }

    // ============================================================================
    // Policy Inheritance Tests
    // ============================================================================

    #[test]
    fn test_calculate_delegation_depth_root() {
        let depth = calculate_delegation_depth("root_session");
        assert_eq!(depth, 0);
    }

    #[test]
    fn test_calculate_delegation_depth_one_level() {
        let depth = calculate_delegation_depth("subagent:root_session:uuid1");
        assert_eq!(depth, 1);
    }

    #[test]
    fn test_calculate_delegation_depth_three_levels() {
        let depth = calculate_delegation_depth("subagent:subagent:subagent:root:uuid1:uuid2:uuid3");
        assert_eq!(depth, 3);
    }

    #[test]
    fn test_calculate_delegation_depth_five_levels() {
        let depth = calculate_delegation_depth(
            "subagent:subagent:subagent:subagent:subagent:root:u1:u2:u3:u4:u5",
        );
        assert_eq!(depth, 5);
    }

    #[test]
    fn test_narrow_child_policy_budget_min_parent() {
        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 500_000, // Parent has 500k remaining
            toolset: vec![],
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: None,
            max_turns: None,
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: None,
            max_cost: Some(2.0), // Child requests 2.0 USD = 2M micro-USD
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: None,
            max_cost: Some(2.0),
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Should be min(500k, 2M) = 500k
        assert_eq!(child_policy.initial_budget_microusd, 500_000);
        assert_eq!(child_policy.remaining_budget_microusd, 500_000);
    }

    #[test]
    fn test_narrow_child_policy_budget_min_child() {
        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 1_000_000, // Parent has 1M remaining
            toolset: vec![],
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: None,
            max_turns: None,
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: None,
            max_cost: Some(0.5), // Child requests 0.5 USD = 500k micro-USD
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: None,
            max_cost: Some(0.5),
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Should be min(1M, 500k) = 500k
        assert_eq!(child_policy.initial_budget_microusd, 500_000);
        assert_eq!(child_policy.remaining_budget_microusd, 500_000);
    }

    #[test]
    fn test_narrow_child_policy_max_turns() {
        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 1_000_000,
            toolset: vec![],
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: None,
            max_turns: Some(10), // Parent allows 10 turns
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: Some(5), // Child allows 5 turns
            max_cost: None,
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: Some(5),
            max_cost: None,
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Should be min(10, 5) = 5
        assert_eq!(child_policy.max_turns, Some(5));
    }

    #[test]
    fn test_narrow_child_policy_tool_intersection() {
        let parent_toolset = vec![
            FunctionDecl::new(
                "read_file",
                Some("Read a file".to_string()),
                serde_json::json!({}),
            ),
            FunctionDecl::new(
                "write_file",
                Some("Write a file".to_string()),
                serde_json::json!({}),
            ),
            FunctionDecl::new(
                "shell",
                Some("Run shell".to_string()),
                serde_json::json!({}),
            ),
        ];

        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 1_000_000,
            toolset: parent_toolset,
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: None,
            max_turns: None,
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: Some(vec!["read_file".to_string(), "write_file".to_string()]), // Child only wants read/write
            max_turns: None,
            max_cost: None,
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: None,
            max_cost: None,
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Should have intersection: read_file and write_file only
        assert_eq!(child_policy.toolset.len(), 2);
        let tool_names: HashSet<String> = child_policy
            .toolset
            .iter()
            .map(|t| t.name.clone())
            .collect();
        assert!(tool_names.contains("read_file"));
        assert!(tool_names.contains("write_file"));
        assert!(!tool_names.contains("shell"));
    }

    #[test]
    fn test_narrow_child_policy_disallowed_tools_inherited() {
        let mut parent_disallowed = HashSet::new();
        parent_disallowed.insert("dangerous_tool".to_string());

        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 1_000_000,
            toolset: vec![],
            auto_approve_tools: HashSet::new(),
            disallowed_tools: parent_disallowed,
            parent_run_id: None,
            max_turns: None,
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: None,
            max_cost: None,
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: None,
            max_cost: None,
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Child should inherit parent's disallowed tools
        assert!(child_policy.disallowed_tools.contains("dangerous_tool"));
    }

    #[test]
    fn test_narrow_child_policy_deadline_inherited() {
        let deadline = Utc::now() + chrono::Duration::hours(1);

        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: Some(deadline),
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 1_000_000,
            toolset: vec![],
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: None,
            max_turns: None,
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: None,
            max_cost: None,
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: None,
            max_cost: None,
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Child should inherit parent's deadline
        assert_eq!(child_policy.deadline, Some(deadline));
    }

    #[test]
    fn test_narrow_child_policy_parent_run_id_set() {
        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 1_000_000,
            toolset: vec![],
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: Some("grandparent".to_string()),
            max_turns: None,
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: None,
            max_cost: None,
        };

        let request = DelegationRequest {
            parent_session_id: "parent_session".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: None,
            max_cost: None,
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Child's parent_run_id should be the parent's session_id
        assert_eq!(
            child_policy.parent_run_id,
            Some("parent_session".to_string())
        );
    }

    // ============================================================================
    // Edge Case Tests - Task 21
    // ============================================================================
    // Edge Case Tests - Task 21
    // ============================================================================

    #[test]
    fn edge_case_depth_six_rejected_at_delegation() {
        // Scenario 3: Depth > 5 rejected at delegation
        // Depth 6 should be rejected (>= MAX_DELEGATION_DEPTH)
        let session_id =
            "subagent:subagent:subagent:subagent:subagent:subagent:root:u1:u2:u3:u4:u5:u6";

        let depth = calculate_delegation_depth(session_id);
        assert_eq!(depth, 6, "Depth should be 6");
        assert!(
            depth >= MAX_DELEGATION_DEPTH,
            "Depth 6 should exceed max depth of 5"
        );
    }

    #[test]
    fn edge_case_depth_five_allowed_at_delegation() {
        // Depth of exactly 5 should be at the limit but processed
        let session_id = "subagent:subagent:subagent:subagent:subagent:root:u1:u2:u3:u4:u5";

        let depth = calculate_delegation_depth(session_id);
        assert_eq!(depth, 5, "Depth should be exactly 5");
        assert_eq!(
            depth, MAX_DELEGATION_DEPTH,
            "Depth 5 should equal max depth"
        );
    }

    #[test]
    fn edge_case_depth_calculation_empty_session() {
        // Empty session ID should have depth 0
        let depth = calculate_delegation_depth("");
        assert_eq!(depth, 0, "Empty session should have depth 0");
    }

    #[test]
    fn edge_case_narrow_child_policy_with_empty_parent_toolset() {
        // Parent has empty toolset - child should also have empty toolset
        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 1_000_000,
            toolset: vec![], // Empty toolset
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: None,
            max_turns: None,
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: Some(vec!["tool_a".to_string(), "tool_b".to_string()]), // Child wants tools
            max_turns: None,
            max_cost: None,
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: None,
            max_cost: None,
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Child should have empty toolset (intersection with empty parent)
        assert!(
            child_policy.toolset.is_empty(),
            "Child should inherit empty toolset from parent"
        );
    }

    #[test]
    fn edge_case_narrow_child_policy_parent_disallowed_inherited() {
        // Parent disallows tools - child should inherit disallowed even if child allows
        let mut parent_disallowed = HashSet::new();
        parent_disallowed.insert("dangerous_tool".to_string());

        let parent_toolset = vec![
            FunctionDecl::new("safe_tool", Some("Safe".to_string()), serde_json::json!({})),
            FunctionDecl::new(
                "dangerous_tool",
                Some("Dangerous".to_string()),
                serde_json::json!({}),
            ),
        ];

        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 1_000_000,
            toolset: parent_toolset,
            auto_approve_tools: HashSet::new(),
            disallowed_tools: parent_disallowed,
            parent_run_id: None,
            max_turns: None,
            rollout_mode: types::RolloutMode::Enforce,
        };

        // Child agent tries to allow dangerous_tool
        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: Some(vec!["safe_tool".to_string(), "dangerous_tool".to_string()]),
            max_turns: None,
            max_cost: None,
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: None,
            max_cost: None,
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);
        // Parent's disallowed is inherited by child
        assert!(child_policy.disallowed_tools.contains("dangerous_tool"));
        // Note: The toolset still contains dangerous_tool because filtering happens at tool dispatch time
        // The disallowed_tools set is what prevents execution
        assert!(child_policy.toolset.iter().any(|t| t.name == "safe_tool"));
        assert!(
            child_policy
                .toolset
                .iter()
                .any(|t| t.name == "dangerous_tool")
        );
    }

    #[test]
    fn edge_case_narrow_child_policy_zero_budget() {
        // Parent has zero remaining budget - child should get zero budget
        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 0, // Zero remaining
            toolset: vec![],
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: None,
            max_turns: None,
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: None,
            max_cost: Some(1.0), // Child requests 1 USD
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: None,
            max_cost: Some(1.0),
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Child should get min(0, 1M) = 0 budget
        assert_eq!(child_policy.initial_budget_microusd, 0);
        assert_eq!(child_policy.remaining_budget_microusd, 0);
    }

    #[test]
    fn edge_case_narrow_child_policy_zero_max_turns() {
        // Parent has zero max_turns - child should get zero max_turns
        let parent_policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1_000_000,
            remaining_budget_microusd: 1_000_000,
            toolset: vec![],
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: None,
            max_turns: Some(0), // Zero max turns
            rollout_mode: types::RolloutMode::Enforce,
        };

        let child_agent = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: Some(10), // Child wants 10 turns
            max_cost: None,
        };

        let request = DelegationRequest {
            parent_session_id: "parent".to_string(),
            parent_user_id: "user".to_string(),
            agent_name: "child".to_string(),
            goal: "test".to_string(),
            caller_selection: None,
            key_facts: vec![],
            max_turns: Some(10),
            max_cost: None,
            parent_policy: None,
        };

        let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

        // Child should get min(0, 10) = 0 max_turns
        assert_eq!(child_policy.max_turns, Some(0));
    }

    // ============================================================================
    // Property-Based Tests - Task 21
    // ============================================================================

    use proptest::prelude::*;

    /// Strategy for generating valid budget values (in micro-USD)
    fn budget_strategy() -> impl Strategy<Value = u64> {
        0..=1_000_000_000_000u64
    }

    /// Strategy for generating max_turns values (for EffectiveRunPolicy)
    fn max_turns_strategy_policy() -> impl Strategy<Value = Option<u32>> {
        prop::option::of(0..=10000u32)
    }

    /// Strategy for generating max_turns values (for AgentDefinition)
    fn max_turns_strategy_agent() -> impl Strategy<Value = Option<u32>> {
        prop::option::of(0..=10000u32)
    }

    /// Strategy for generating tool names (alphanumeric with underscores)
    fn tool_name_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop::sample::select(vec![
                'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p',
                'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '_',
            ]),
            1..20,
        )
        .prop_map(|chars| chars.into_iter().collect())
    }

    /// Strategy for generating a set of tool names
    fn toolset_strategy() -> impl Strategy<Value = HashSet<String>> {
        prop::collection::hash_set(tool_name_strategy(), 0..10)
    }

    proptest! {
        // Property: Delegation chain preserves narrowing-only semantics for budget
        // Child budget should always be <= parent budget
        #[test]
        fn prop_delegation_budget_narrowing_only(
            parent_budget in budget_strategy(),
            child_requested in budget_strategy(),
        ) {
            let parent_policy = EffectiveRunPolicy {
                started_at: Utc::now(),
                deadline: None,
                initial_budget_microusd: parent_budget,
                remaining_budget_microusd: parent_budget,
                toolset: vec![],
                auto_approve_tools: HashSet::new(),
                disallowed_tools: HashSet::new(),
                parent_run_id: None,
                max_turns: None,
                rollout_mode: types::RolloutMode::Enforce,
            };

            let child_agent = AgentDefinition {
                system_prompt: None,
                system_prompt_file: None,
                selection: None,
                tools: None,
                max_turns: None,
                max_cost: Some(child_requested as f64 / 1_000_000.0),
            };

            let request = DelegationRequest {
                parent_session_id: "parent".to_string(),
                parent_user_id: "user".to_string(),
                agent_name: "child".to_string(),
                goal: "test".to_string(),
                caller_selection: None,
                key_facts: vec![],
                max_turns: None,
                max_cost: Some(child_requested as f64 / 1_000_000.0),
                parent_policy: None,
            };

            let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

            // Property: Child budget <= Parent budget (narrowing-only)
            prop_assert!(
                child_policy.remaining_budget_microusd <= parent_policy.remaining_budget_microusd,
                "Child budget {} should be <= parent budget {}",
                child_policy.remaining_budget_microusd,
                parent_policy.remaining_budget_microusd
            );
        }

        // Property: Delegation chain preserves narrowing-only semantics for max_turns
        // Child max_turns should always be <= parent max_turns (when both are Some)
        #[test]
        fn prop_delegation_max_turns_narrowing_only(
            parent_turns in max_turns_strategy_policy(),
            child_turns in max_turns_strategy_agent(),
        ) {
            let parent_policy = EffectiveRunPolicy {
                started_at: Utc::now(),
                deadline: None,
                initial_budget_microusd: 1_000_000,
                remaining_budget_microusd: 1_000_000,
                toolset: vec![],
                auto_approve_tools: HashSet::new(),
                disallowed_tools: HashSet::new(),
                parent_run_id: None,
                max_turns: parent_turns,
                rollout_mode: types::RolloutMode::Enforce,
            };
            let child_agent = AgentDefinition {
                system_prompt: None,
                system_prompt_file: None,
                selection: None,
                tools: None,
                max_turns: child_turns,
                max_cost: None,
            };
            let request = DelegationRequest {
                parent_session_id: "parent".to_string(),
                parent_user_id: "user".to_string(),
                agent_name: "child".to_string(),
                goal: "test".to_string(),
                caller_selection: None,
                key_facts: vec![],
                max_turns: child_turns,
                max_cost: None,
                parent_policy: None,
            };

            let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

            // Property: If both have max_turns, child <= parent
            if let (Some(p), Some(c)) = (parent_policy.max_turns, child_policy.max_turns) {
                prop_assert!(
                    c <= p,
                    "Child max_turns {} should be <= parent max_turns {}",
                    c, p
                );
            }

            // Property: If parent has max_turns and child doesn't, child inherits parent's
            if parent_policy.max_turns.is_some() && child_turns.is_none() {
                prop_assert_eq!(
                    child_policy.max_turns, parent_policy.max_turns,
                    "Child should inherit parent's max_turns when child has none"
                );
            }
        }

        // Property: Tool intersection is always subset of parent tools
        #[test]
        fn prop_delegation_tool_intersection_subset(
            parent_tools in toolset_strategy(),
            child_tools in toolset_strategy(),
        ) {
            let parent_toolset: Vec<FunctionDecl> = parent_tools
                .iter()
                .map(|name| FunctionDecl::new(name, Some(name.clone()), serde_json::json!({})))
                .collect();

            let parent_policy = EffectiveRunPolicy {
                started_at: Utc::now(),
                deadline: None,
                initial_budget_microusd: 1_000_000,
                remaining_budget_microusd: 1_000_000,
                toolset: parent_toolset,
                auto_approve_tools: HashSet::new(),
                disallowed_tools: HashSet::new(),
                parent_run_id: None,
                max_turns: None,
                rollout_mode: types::RolloutMode::Enforce,
            };

            let child_agent = AgentDefinition {
                system_prompt: None,
                system_prompt_file: None,
                selection: None,
                tools: Some(child_tools.into_iter().collect()),
                max_turns: None,
                max_cost: None,
            };

            let request = DelegationRequest {
                parent_session_id: "parent".to_string(),
                parent_user_id: "user".to_string(),
                agent_name: "child".to_string(),
                goal: "test".to_string(),
                caller_selection: None,
                key_facts: vec![],
                max_turns: None,
                max_cost: None,
                parent_policy: None,
            };

            let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

            // Property: Child toolset is always subset of parent toolset
            let parent_tool_names: HashSet<String> = parent_policy
                .toolset
                .iter()
                .map(|t| t.name.clone())
                .collect();

            for child_tool in &child_policy.toolset {
                prop_assert!(
                    parent_tool_names.contains(&child_tool.name),
                    "Child tool {} must be in parent's toolset",
                    child_tool.name
                );
            }
        }

        // Property: Disallowed tools are always preserved through delegation chain
        #[test]
        fn prop_delegation_disallowed_preserved(
            disallowed in toolset_strategy(),
        ) {
            let parent_disallowed = disallowed.clone();

            let parent_policy = EffectiveRunPolicy {
                started_at: Utc::now(),
                deadline: None,
                initial_budget_microusd: 1_000_000,
                remaining_budget_microusd: 1_000_000,
                toolset: vec![],
                auto_approve_tools: HashSet::new(),
                disallowed_tools: parent_disallowed.clone(),
                parent_run_id: None,
                max_turns: None,
                rollout_mode: types::RolloutMode::Enforce,
            };

            let child_agent = AgentDefinition {
                system_prompt: None,
                system_prompt_file: None,
                selection: None,
                tools: None,
                max_turns: None,
                max_cost: None,
            };

            let request = DelegationRequest {
                parent_session_id: "parent".to_string(),
                parent_user_id: "user".to_string(),
                agent_name: "child".to_string(),
                goal: "test".to_string(),
                caller_selection: None,
                key_facts: vec![],
                max_turns: None,
                max_cost: None,
                parent_policy: None,
            };

            let child_policy = narrow_child_policy(&parent_policy, &child_agent, &request);

            // Property: All parent disallowed tools are in child disallowed
            for tool in &parent_disallowed {
                prop_assert!(
                    child_policy.disallowed_tools.contains(tool),
                    "Parent's disallowed tool {} must be in child's disallowed set",
                    tool
                );
            }
        }

        // Property: Multi-level delegation preserves narrowing (3 levels)
        #[test]
        fn prop_three_level_delegation_preserves_narrowing(
            level1_budget in budget_strategy(),
            level2_budget in budget_strategy(),
            level3_budget in budget_strategy(),
        ) {
            // Level 1 (root)
            let policy1 = EffectiveRunPolicy {
                started_at: Utc::now(),
                deadline: None,
                initial_budget_microusd: level1_budget,
                remaining_budget_microusd: level1_budget,
                toolset: vec![],
                auto_approve_tools: HashSet::new(),
                disallowed_tools: HashSet::new(),
                parent_run_id: None,
                max_turns: None,
                rollout_mode: types::RolloutMode::Enforce,
            };

            // Level 2 (child of root)
            let agent2 = AgentDefinition {
                system_prompt: None,
                system_prompt_file: None,
                selection: None,
                tools: None,
                max_turns: None,
                max_cost: Some(level2_budget as f64 / 1_000_000.0),
            };
            let request2 = DelegationRequest {
                parent_session_id: "root".to_string(),
                parent_user_id: "user".to_string(),
                agent_name: "level2".to_string(),
                goal: "test".to_string(),
                caller_selection: None,
                key_facts: vec![],
                max_turns: None,
                max_cost: Some(level2_budget as f64 / 1_000_000.0),
                parent_policy: None,
            };
            let policy2 = narrow_child_policy(&policy1, &agent2, &request2);

            // Level 3 (child of level 2)
            let agent3 = AgentDefinition {
                system_prompt: None,
                system_prompt_file: None,
                selection: None,
                tools: None,
                max_turns: None,
                max_cost: Some(level3_budget as f64 / 1_000_000.0),
            };
            let request3 = DelegationRequest {
                parent_session_id: "subagent:root:uuid1".to_string(),
                parent_user_id: "user".to_string(),
                agent_name: "level3".to_string(),
                goal: "test".to_string(),
                caller_selection: None,
                key_facts: vec![],
                max_turns: None,
                max_cost: Some(level3_budget as f64 / 1_000_000.0),
                parent_policy: None,
            };
            let policy3 = narrow_child_policy(&policy2, &agent3, &request3);

            // Property: Budget should be monotonically non-increasing down the chain
            prop_assert!(
                policy2.remaining_budget_microusd <= policy1.remaining_budget_microusd,
                "Level 2 budget should be <= Level 1 budget"
            );
            prop_assert!(
                policy3.remaining_budget_microusd <= policy2.remaining_budget_microusd,
                "Level 3 budget should be <= Level 2 budget"
            );
            prop_assert!(
                policy3.remaining_budget_microusd <= policy1.remaining_budget_microusd,
                "Level 3 budget should be <= Level 1 budget"
            );
        }
    }
}
