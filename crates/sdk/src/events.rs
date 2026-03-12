//! SDK Events module
//!
//! Event types and streaming interfaces for the SDK.

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::Stream;
use serde::{Deserialize, Serialize};
use types::{StopReason, ToolCall, UsageUpdate};

/// Result of a single-turn or multi-turn run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    /// The final response text from the agent.
    pub response: String,
    /// Why the run stopped.
    pub stop_reason: StopReason,
    /// Token usage information (if available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<UsageUpdate>,
    /// Tool calls made during the run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl RunResult {
    /// Create a new RunResult with the given response and stop reason.
    pub fn new(response: impl Into<String>, stop_reason: StopReason) -> Self {
        Self {
            response: response.into(),
            stop_reason,
            usage: None,
            tool_calls: Vec::new(),
        }
    }

    /// Add usage information to the result.
    pub fn with_usage(mut self, usage: UsageUpdate) -> Self {
        self.usage = Some(usage);
        self
    }

    /// Add tool calls to the result.
    pub fn with_tool_calls(mut self, tool_calls: Vec<ToolCall>) -> Self {
        self.tool_calls = tool_calls;
        self
    }
}

/// Events emitted during a streaming run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum RunEvent {
    /// Text delta from the assistant.
    Text(String),
    /// A tool call was initiated.
    ToolCall(ToolCall),
    /// Result from a tool execution.
    ToolResult {
        /// The tool call ID this result corresponds to.
        call_id: String,
        /// The result content (success or error message).
        content: String,
        /// Whether the tool execution succeeded.
        success: bool,
    },
    /// Budget update (tokens used, cost, etc.).
    BudgetUpdate {
        /// Tokens used so far.
        tokens_used: u64,
        /// Estimated cost in micro-USD.
        cost_microusd: u64,
        /// Remaining budget in micro-USD (if applicable).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        remaining_budget: Option<u64>,
    },
    /// Policy stopped the run (e.g., max turns exceeded).
    PolicyStop {
        /// The reason the policy stopped the run.
        reason: String,
        /// The stop reason category.
        stop_reason: StopReason,
    },
    /// Run completed successfully.
    Completed(RunResult),
}

/// A stream of run events.
pub struct RunEventStream {
    inner: Pin<Box<dyn Stream<Item = RunEvent> + Send>>,
}

impl RunEventStream {
    /// Create a new RunEventStream from an existing stream.
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = RunEvent> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl Stream for RunEventStream {
    type Item = RunEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

/// Internal event types used during streaming execution.
#[derive(Debug, Clone)]
pub(crate) enum InternalRunEvent {
    Text(String),
    ToolCall(ToolCall),
    ToolResult {
        call_id: String,
        content: String,
        success: bool,
    },
    UsageUpdate(UsageUpdate),
    Progress {
        message: String,
        turn: usize,
        max_turns: usize,
    },
    Completed {
        response: String,
        stop_reason: StopReason,
        usage: Option<UsageUpdate>,
        tool_calls: Vec<ToolCall>,
    },
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_result_creation() {
        let result = RunResult::new("Hello, world!", StopReason::Completed);
        assert_eq!(result.response, "Hello, world!");
        assert_eq!(result.stop_reason, StopReason::Completed);
        assert!(result.usage.is_none());
        assert!(result.tool_calls.is_empty());
    }

    #[test]
    fn test_run_result_with_usage() {
        let usage = UsageUpdate {
            prompt_tokens: Some(10),
            completion_tokens: Some(20),
            total_tokens: Some(30),
        };
        let result = RunResult::new("Hello", StopReason::Completed).with_usage(usage.clone());
        assert_eq!(result.usage, Some(usage));
    }

    #[test]
    fn test_run_event_serialization() {
        let event = RunEvent::Text("Hello".to_string());
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("text"));
        assert!(json.contains("Hello"));
    }
}
