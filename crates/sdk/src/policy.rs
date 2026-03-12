//! SDK Policy module
//!
//! Defines runtime policies and enforcement configurations.

pub use types::{
    DefaultToolPermissionHandler, EffectiveRunPolicy, RolloutMode, RunPolicyInput, StopReason,
    ToolPermissionContext, ToolPermissionDecision, ToolPermissionHandler, ToolPolicyInput,
};

/// Configuration for the SDK client.
#[derive(Debug, Clone, Default)]
pub struct ClientConfig {
    /// The user ID for this client session.
    pub user_id: String,
    /// Optional session ID (if not provided, a new session will be created).
    pub session_id: Option<String>,
    /// The agent name to use for this session.
    pub agent_name: String,
    /// Optional policy overrides for this run.
    pub policy: Option<RunPolicyInput>,
}

impl ClientConfig {
    /// Create a new client config with the given user ID.
    pub fn new(user_id: impl Into<String>) -> Self {
        Self {
            user_id: user_id.into(),
            session_id: None,
            agent_name: "default".to_string(),
            policy: None,
        }
    }

    /// Set the session ID.
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Set the agent name.
    pub fn with_agent_name(mut self, agent_name: impl Into<String>) -> Self {
        self.agent_name = agent_name.into();
        self
    }

    /// Set the policy overrides.
    pub fn with_policy(mut self, policy: RunPolicyInput) -> Self {
        self.policy = Some(policy);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::new("user123");
        assert_eq!(config.user_id, "user123");
        assert_eq!(config.agent_name, "default");
        assert!(config.session_id.is_none());
        assert!(config.policy.is_none());
    }

    #[test]
    fn test_client_config_builder() {
        let policy = RunPolicyInput::default();
        let config = ClientConfig::new("user123")
            .with_session_id("session456")
            .with_agent_name("custom_agent")
            .with_policy(policy);

        assert_eq!(config.user_id, "user123");
        assert_eq!(config.session_id, Some("session456".to_string()));
        assert_eq!(config.agent_name, "custom_agent");
        assert!(config.policy.is_some());
    }
}
