use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::Duration;

use crate::tool::FunctionDecl;

/// Input type for configuring runtime policy constraints.
///
/// All fields are optional and default to `None`, meaning no constraint is applied.
/// This type is used to specify policy overrides when creating or modifying sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RunPolicyInput {
    /// Maximum runtime duration for the session/run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runtime: Option<Duration>,

    /// Maximum budget in micro-USD (1/1,000,000 of a USD).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_budget_microusd: Option<u64>,

    /// Maximum number of turns allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<usize>,

    /// Tool-specific policy configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_policy: Option<ToolPolicyInput>,
}

/// Input type for configuring tool policy constraints.
///
/// All fields are optional and default to `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ToolPolicyInput {
    /// List of tool names that are allowed for use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolset: Option<Vec<String>>,

    /// List of tool names that can be auto-approved without user confirmation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_approve_tools: Option<Vec<String>>,

    /// List of tool names that are explicitly disallowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disallowed_tools: Option<Vec<String>>,
}

/// The effective runtime policy for a specific run, combining inherited
/// constraints with local overrides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectiveRunPolicy {
    /// When the run started
    pub started_at: DateTime<Utc>,
    /// Deadline for the run (if any)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Utc>>,
    /// Initial budget in micro-USD
    pub initial_budget_microusd: u64,
    /// Remaining budget in micro-USD
    pub remaining_budget_microusd: u64,
    /// Available tools for this run
    pub toolset: Vec<FunctionDecl>,
    /// Tools that can be auto-approved without user confirmation
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub auto_approve_tools: HashSet<String>,
    /// Tools that are explicitly disallowed
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub disallowed_tools: HashSet<String>,
    /// Parent run ID for tracking delegation chains
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    /// Maximum number of turns allowed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// How to handle policy violations
    #[serde(default)]
    pub rollout_mode: RolloutMode,
}

/// Why a run stopped.
///
/// This enum is marked as `#[non_exhaustive]` to allow adding new
/// stop reasons in the future without breaking existing code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum StopReason {
    /// Run completed successfully
    Completed,
    /// Run was cancelled by user or system
    Cancelled,
    /// Maximum number of turns exceeded
    MaxTurns,
    /// Maximum runtime exceeded
    MaxRuntimeExceeded,
    /// Maximum budget exceeded
    MaxBudgetExceeded,
    /// Attempted to use a disallowed tool
    ToolDisallowed,
    /// Tool permission was denied
    ToolPermissionDenied,
    /// Provider timed out
    ProviderTimedOut,
}

/// How to handle policy violations during rollout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RolloutMode {
    /// Strictly enforce policy constraints
    #[default]
    Enforce,
    /// Allow violations but log them
    SoftFail,
    /// Only observe and record, never block
    ObserveOnly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_policy_input_construction() {
        let policy = RunPolicyInput {
            max_runtime: Some(Duration::from_secs(300)),
            max_budget_microusd: Some(1_000_000),
            max_turns: Some(50),
            tool_policy: Some(ToolPolicyInput {
                toolset: Some(vec!["read_file".to_string(), "write_file".to_string()]),
                auto_approve_tools: Some(vec!["read_file".to_string()]),
                disallowed_tools: Some(vec!["shell".to_string()]),
            }),
        };

        assert_eq!(policy.max_runtime, Some(Duration::from_secs(300)));
        assert_eq!(policy.max_budget_microusd, Some(1_000_000));
        assert_eq!(policy.max_turns, Some(50));
        assert!(policy.tool_policy.is_some());
    }

    #[test]
    fn test_tool_policy_input_construction() {
        let tool_policy = ToolPolicyInput {
            toolset: Some(vec!["tool1".to_string(), "tool2".to_string()]),
            auto_approve_tools: Some(vec!["tool1".to_string()]),
            disallowed_tools: Some(vec!["dangerous_tool".to_string()]),
        };

        assert_eq!(tool_policy.toolset, Some(vec!["tool1".to_string(), "tool2".to_string()]));
        assert_eq!(tool_policy.auto_approve_tools, Some(vec!["tool1".to_string()]));
        assert_eq!(tool_policy.disallowed_tools, Some(vec!["dangerous_tool".to_string()]));
    }

    #[test]
    fn test_run_policy_input_default() {
        let policy = RunPolicyInput::default();

        assert!(policy.max_runtime.is_none());
        assert!(policy.max_budget_microusd.is_none());
        assert!(policy.max_turns.is_none());
        assert!(policy.tool_policy.is_none());
    }

    #[test]
    fn test_tool_policy_input_default() {
        let tool_policy = ToolPolicyInput::default();

        assert!(tool_policy.toolset.is_none());
        assert!(tool_policy.auto_approve_tools.is_none());
        assert!(tool_policy.disallowed_tools.is_none());
    }

    #[test]
    fn test_run_policy_input_serde_roundtrip() {
        let original = RunPolicyInput {
            max_runtime: Some(Duration::from_secs(600)),
            max_budget_microusd: Some(5_000_000),
            max_turns: Some(100),
            tool_policy: Some(ToolPolicyInput {
                toolset: Some(vec!["tool_a".to_string()]),
                auto_approve_tools: None,
                disallowed_tools: Some(vec!["tool_b".to_string()]),
            }),
        };

        let json = serde_json::to_string(&original).expect("serialization failed");
        let deserialized: RunPolicyInput = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_tool_policy_input_serde_roundtrip() {
        let original = ToolPolicyInput {
            toolset: Some(vec!["read".to_string(), "write".to_string()]),
            auto_approve_tools: Some(vec!["read".to_string()]),
            disallowed_tools: Some(vec!["exec".to_string()]),
        };

        let json = serde_json::to_string(&original).expect("serialization failed");
        let deserialized: ToolPolicyInput = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_run_policy_input_serde_empty() {
        // Test that an empty JSON object deserializes to all None values
        let json = "{}";
        let policy: RunPolicyInput = serde_json::from_str(json).expect("deserialization failed");

        assert!(policy.max_runtime.is_none());
        assert!(policy.max_budget_microusd.is_none());
        assert!(policy.max_turns.is_none());
        assert!(policy.tool_policy.is_none());
    }

    #[test]
    fn test_tool_policy_input_serde_empty() {
        // Test that an empty JSON object deserializes to all None values
        let json = "{}";
        let tool_policy: ToolPolicyInput = serde_json::from_str(json).expect("deserialization failed");

        assert!(tool_policy.toolset.is_none());
        assert!(tool_policy.auto_approve_tools.is_none());
        assert!(tool_policy.disallowed_tools.is_none());
    }

    #[test]
    fn test_run_policy_input_serde_partial() {
        // Test partial JSON with only some fields
        let json = r#"{"max_turns": 25}"#;
        let policy: RunPolicyInput = serde_json::from_str(json).expect("deserialization failed");

        assert!(policy.max_runtime.is_none());
        assert!(policy.max_budget_microusd.is_none());
        assert_eq!(policy.max_turns, Some(25));
        assert!(policy.tool_policy.is_none());
    }

    #[test]
    fn test_serialized_json_omits_none_fields() {
        // Test that None fields are skipped during serialization
        let policy = RunPolicyInput {
            max_runtime: None,
            max_budget_microusd: Some(1_000_000),
            max_turns: None,
            tool_policy: None,
        };

        let json = serde_json::to_string(&policy).expect("serialization failed");
        
        // Should only contain max_budget_microusd
        assert!(json.contains("max_budget_microusd"));
        assert!(!json.contains("max_runtime"));
        assert!(!json.contains("max_turns"));
        assert!(!json.contains("tool_policy"));
    }

    #[test]
    fn test_stop_reason_variant_construction() {
        // Test that all variants can be constructed
        let _completed = StopReason::Completed;
        let _cancelled = StopReason::Cancelled;
        let _max_turns = StopReason::MaxTurns;
        let _max_runtime = StopReason::MaxRuntimeExceeded;
        let _max_budget = StopReason::MaxBudgetExceeded;
        let _tool_disallowed = StopReason::ToolDisallowed;
        let _tool_permission = StopReason::ToolPermissionDenied;
        let _provider_timeout = StopReason::ProviderTimedOut;
    }

    #[test]
    fn test_stop_reason_wildcard_matching() {
        // Test wildcard matching with non_exhaustive enum
        // This pattern should compile and work correctly
        let reason = StopReason::Completed;
        let matched = match reason {
            StopReason::Completed => "completed",
            StopReason::Cancelled => "cancelled",
            StopReason::MaxTurns => "max_turns",
            StopReason::MaxRuntimeExceeded => "max_runtime",
            StopReason::MaxBudgetExceeded => "max_budget",
            StopReason::ToolDisallowed => "tool_disallowed",
            StopReason::ToolPermissionDenied => "tool_permission_denied",
            StopReason::ProviderTimedOut => "provider_timeout",
            _ => "unknown", // Wildcard for future variants
        };
        assert_eq!(matched, "completed");

        // Test with a different variant
        let reason2 = StopReason::MaxTurns;
        let matched2 = match reason2 {
            StopReason::Completed => "completed",
            StopReason::Cancelled => "cancelled",
            StopReason::MaxTurns => "max_turns",
            StopReason::MaxRuntimeExceeded => "max_runtime",
            StopReason::MaxBudgetExceeded => "max_budget",
            StopReason::ToolDisallowed => "tool_disallowed",
            StopReason::ToolPermissionDenied => "tool_permission_denied",
            StopReason::ProviderTimedOut => "provider_timeout",
            _ => "unknown",
        };
        assert_eq!(matched2, "max_turns");
    }

    #[test]
    fn test_stop_reason_equality() {
        assert_eq!(StopReason::Completed, StopReason::Completed);
        assert_ne!(StopReason::Completed, StopReason::Cancelled);
        assert_eq!(StopReason::MaxTurns, StopReason::MaxTurns);
        assert_ne!(StopReason::MaxBudgetExceeded, StopReason::MaxRuntimeExceeded);
    }

    #[test]
    fn test_rollout_mode_default() {
        let mode: RolloutMode = Default::default();
        assert_eq!(mode, RolloutMode::Enforce);
    }

    #[test]
    fn test_rollout_mode_variants() {
        assert_eq!(RolloutMode::Enforce as i32, 0);
        assert_eq!(RolloutMode::SoftFail as i32, 1);
        assert_eq!(RolloutMode::ObserveOnly as i32, 2);
    }

    #[test]
    fn test_rollout_mode_serialization() {
        // Test serialization roundtrip
        let enforce = RolloutMode::Enforce;
        let json = serde_json::to_string(&enforce).unwrap();
        assert_eq!(json, "\"enforce\"");

        let soft_fail = RolloutMode::SoftFail;
        let json = serde_json::to_string(&soft_fail).unwrap();
        assert_eq!(json, "\"soft_fail\"");

        let observe_only = RolloutMode::ObserveOnly;
        let json = serde_json::to_string(&observe_only).unwrap();
        assert_eq!(json, "\"observe_only\"");
    }

    #[test]
    fn test_rollout_mode_deserialization() {
        let enforce: RolloutMode = serde_json::from_str("\"enforce\"").unwrap();
        assert_eq!(enforce, RolloutMode::Enforce);

        let soft_fail: RolloutMode = serde_json::from_str("\"soft_fail\"").unwrap();
        assert_eq!(soft_fail, RolloutMode::SoftFail);

        let observe_only: RolloutMode = serde_json::from_str("\"observe_only\"").unwrap();
        assert_eq!(observe_only, RolloutMode::ObserveOnly);
    }

    #[test]
    fn test_effective_run_policy_construction() {
        let policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: None,
            initial_budget_microusd: 1000000,
            remaining_budget_microusd: 1000000,
            toolset: vec![],
            auto_approve_tools: HashSet::new(),
            disallowed_tools: HashSet::new(),
            parent_run_id: None,
            max_turns: Some(10),
            rollout_mode: RolloutMode::Enforce,
        };

        assert_eq!(policy.initial_budget_microusd, 1000000);
        assert_eq!(policy.max_turns, Some(10));
        assert_eq!(policy.rollout_mode, RolloutMode::Enforce);
    }

    #[test]
    fn test_effective_run_policy_with_optional_fields() {
        let deadline = Utc::now() + chrono::Duration::hours(1);
        let mut auto_approve = HashSet::new();
        auto_approve.insert("safe_tool".to_string());
        let mut disallowed = HashSet::new();
        disallowed.insert("dangerous_tool".to_string());

        let policy = EffectiveRunPolicy {
            started_at: Utc::now(),
            deadline: Some(deadline),
            initial_budget_microusd: 500000,
            remaining_budget_microusd: 400000,
            toolset: vec![],
            auto_approve_tools: auto_approve,
            disallowed_tools: disallowed,
            parent_run_id: Some("parent-123".to_string()),
            max_turns: Some(5),
            rollout_mode: RolloutMode::SoftFail,
        };

        assert!(policy.deadline.is_some());
        assert!(policy.parent_run_id.is_some());
        assert!(policy.auto_approve_tools.contains("safe_tool"));
        assert!(policy.disallowed_tools.contains("dangerous_tool"));
    }

    #[test]
    fn test_stop_reason_clone() {
        let reason = StopReason::MaxBudgetExceeded;
        let cloned = reason.clone();
        assert_eq!(reason, cloned);
    }

    #[test]
    fn test_stop_reason_debug() {
        let reason = StopReason::ToolDisallowed;
        let debug = format!("{:?}", reason);
        assert!(debug.contains("ToolDisallowed"));
    }

}


// ============================================================================
// Tool Permission Handler Types
// ============================================================================

use async_trait::async_trait;

/// Context provided to permission handlers for making authorization decisions.
#[derive(Debug, Clone)]
pub struct ToolPermissionContext {
    /// Unique identifier for the session
    pub session_id: String,
    /// Identifier for the user making the request
    pub user_id: String,
    /// Current turn number in the session
    pub turn: u32,
    /// Remaining budget for the session
    pub remaining_budget: u64,
}

/// Decision returned by a permission handler for a tool execution request.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolPermissionDecision {
    /// Allow the tool execution with the original arguments.
    Allow,
    /// Deny the tool execution with a reason.
    Deny { reason: String },
    /// Allow the tool execution with modified arguments.
    AllowWithModification { modified_args: serde_json::Value },
}

/// Trait for handling tool permission checks.
///
/// Implementations can inspect the tool name, arguments, and context
/// to determine whether to allow, deny, or modify the tool execution.
#[async_trait]
pub trait ToolPermissionHandler: Send + Sync {
    /// Check if the tool execution should be allowed.
    ///
    /// # Arguments
    /// * `tool_name` - The name of the tool being invoked
    /// * `arguments` - The arguments provided for the tool execution
    /// * `context` - Context about the current session and user
    ///
    /// # Returns
    /// A decision indicating whether to allow, deny, or modify the execution
    async fn check_permission(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        context: &ToolPermissionContext,
    ) -> ToolPermissionDecision;
}

/// Default permission handler that always allows tool executions.
///
/// This is useful as a fallback when no custom permission logic is needed.
pub struct DefaultToolPermissionHandler;

impl DefaultToolPermissionHandler {
    /// Create a new default permission handler.
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultToolPermissionHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolPermissionHandler for DefaultToolPermissionHandler {
    async fn check_permission(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
        _context: &ToolPermissionContext,
    ) -> ToolPermissionDecision {
        ToolPermissionDecision::Allow
    }
}

#[cfg(test)]
mod permission_tests {
    use super::*;

    fn create_test_context() -> ToolPermissionContext {
        ToolPermissionContext {
            session_id: "test-session-123".to_string(),
            user_id: "user-456".to_string(),
            turn: 1,
            remaining_budget: 1000,
        }
    }

    #[tokio::test]
    async fn test_default_handler_returns_allow() {
        let handler = DefaultToolPermissionHandler::new();
        let context = create_test_context();
        let args = serde_json::json!({"key": "value"});

        let decision = handler
            .check_permission("test_tool", &args, &context)
            .await;

        assert_eq!(decision, ToolPermissionDecision::Allow);
    }

    #[tokio::test]
    async fn test_default_handler_ignores_tool_name() {
        let handler = DefaultToolPermissionHandler::new();
        let context = create_test_context();
        let args = serde_json::json!({});

        // Should return Allow regardless of tool name
        let decision = handler
            .check_permission("any_tool_name", &args, &context)
            .await;

        assert_eq!(decision, ToolPermissionDecision::Allow);
    }

    #[tokio::test]
    async fn test_deny_decision_propagates_reason() {
        // Test that a custom handler can create a Deny decision with a reason
        struct DenyAllHandler {
            reason: String,
        }

        #[async_trait]
        impl ToolPermissionHandler for DenyAllHandler {
            async fn check_permission(
                &self,
                _tool_name: &str,
                _arguments: &serde_json::Value,
                _context: &ToolPermissionContext,
            ) -> ToolPermissionDecision {
                ToolPermissionDecision::Deny {
                    reason: self.reason.clone(),
                }
            }
        }

        let expected_reason = "Tool execution blocked by policy".to_string();
        let handler = DenyAllHandler {
            reason: expected_reason.clone(),
        };
        let context = create_test_context();
        let args = serde_json::json!({"command": "rm -rf /"});

        let decision = handler
            .check_permission("dangerous_tool", &args, &context)
            .await;

        match decision {
            ToolPermissionDecision::Deny { reason } => {
                assert_eq!(reason, expected_reason);
            }
            _ => panic!("Expected Deny decision, got {:?}", decision),
        }
    }

    #[tokio::test]
    async fn test_allow_with_modification_decision() {
        // Test that a custom handler can create an AllowWithModification decision
        struct ModifyArgsHandler;

        #[async_trait]
        impl ToolPermissionHandler for ModifyArgsHandler {
            async fn check_permission(
                &self,
                _tool_name: &str,
                _arguments: &serde_json::Value,
                _context: &ToolPermissionContext,
            ) -> ToolPermissionDecision {
                let modified = serde_json::json!({"sanitized": true, "original": false});
                ToolPermissionDecision::AllowWithModification {
                    modified_args: modified,
                }
            }
        }

        let handler = ModifyArgsHandler;
        let context = create_test_context();
        let args = serde_json::json!({"original": true});

        let decision = handler
            .check_permission("some_tool", &args, &context)
            .await;

        match decision {
            ToolPermissionDecision::AllowWithModification { modified_args } => {
                assert_eq!(modified_args["sanitized"], true);
                assert_eq!(modified_args["original"], false);
            }
            _ => panic!(
                "Expected AllowWithModification decision, got {:?}",
                decision
            ),
        }
    }

    #[test]
    fn test_tool_permission_context_creation() {
        let context = ToolPermissionContext {
            session_id: "session-abc".to_string(),
            user_id: "user-xyz".to_string(),
            turn: 5,
            remaining_budget: 500,
        };

        assert_eq!(context.session_id, "session-abc");
        assert_eq!(context.user_id, "user-xyz");
        assert_eq!(context.turn, 5);
        assert_eq!(context.remaining_budget, 500);
    }

    #[test]
    fn test_tool_permission_decision_equality() {
        assert_eq!(ToolPermissionDecision::Allow, ToolPermissionDecision::Allow);

        let deny1 = ToolPermissionDecision::Deny {
            reason: "test".to_string(),
        };
        let deny2 = ToolPermissionDecision::Deny {
            reason: "test".to_string(),
        };
        assert_eq!(deny1, deny2);

        let deny3 = ToolPermissionDecision::Deny {
            reason: "different".to_string(),
        };
        assert_ne!(deny1, deny3);

        let modified1 = ToolPermissionDecision::AllowWithModification {
            modified_args: serde_json::json!({"a": 1}),
        };
        let modified2 = ToolPermissionDecision::AllowWithModification {
            modified_args: serde_json::json!({"a": 1}),
        };
        assert_eq!(modified1, modified2);
    }

    #[test]
    fn test_default_tool_permission_handler_default() {
        let handler: DefaultToolPermissionHandler = Default::default();
        // Just verify it creates successfully
        let _ = handler;
    }
}