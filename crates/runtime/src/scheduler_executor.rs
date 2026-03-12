use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use memory_crate::SchedulerStore;
use memory_crate::cadence::next_run_for_cadence;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use types::{
    AgentDefinition, EffectiveRunPolicy, FunctionDecl, GatewayScheduledNotification,
    GatewayServerFrame, NotificationPolicy, RunPolicyInput, RuntimeConfig, ScheduleCadence,
    ScheduleDefinition, ScheduleRunRecord, ScheduleRunStatus, ScheduleStatus, SchedulerConfig,
};

use crate::{ScheduledTurnRunner, policy_guard};

/// Callback trait for publishing scheduled notifications to connected users.
#[async_trait]
pub trait SchedulerNotifier: Send + Sync {
    async fn notify_user(&self, schedule: &ScheduleDefinition, frame: GatewayServerFrame);
}

pub struct SchedulerExecutor {
    store: Arc<dyn SchedulerStore>,
    turn_runner: Arc<dyn ScheduledTurnRunner>,
    notifier: Arc<dyn SchedulerNotifier>,
    config: SchedulerConfig,
    cancellation: CancellationToken,
}

impl SchedulerExecutor {
    pub fn new(
        store: Arc<dyn SchedulerStore>,
        turn_runner: Arc<dyn ScheduledTurnRunner>,
        notifier: Arc<dyn SchedulerNotifier>,
        config: SchedulerConfig,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            store,
            turn_runner,
            notifier,
            config,
            cancellation,
        }
    }

    pub async fn run(&self) {
        let mut interval =
            tokio::time::interval(Duration::from_secs(self.config.poll_interval_secs));
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = self.cancellation.cancelled() => {
                    tracing::info!("scheduler executor shutting down");
                    break;
                }
                _ = interval.tick() => {
                    self.tick().await;
                }
            }
        }
    }

    pub(crate) async fn tick(&self) {
        let now = Utc::now().to_rfc3339();
        let due = match self
            .store
            .due_schedules(&now, self.config.max_concurrent)
            .await
        {
            Ok(due) => due,
            Err(e) => {
                tracing::warn!("scheduler poll failed: {e}");
                return;
            }
        };

        if due.is_empty() {
            return;
        }

        tracing::debug!("scheduler: {} due schedule(s)", due.len());

        let futs: Vec<_> = due.into_iter().map(|s| self.execute_schedule(s)).collect();
        futures::future::join_all(futs).await;
    }

    async fn execute_schedule(&self, schedule: ScheduleDefinition) {
        let run_id = uuid::Uuid::new_v4().to_string();
        let session_id = format!("scheduled:{}", schedule.schedule_id);
        let started_at = Utc::now().to_rfc3339();

        let prompt = if schedule.notification_policy == NotificationPolicy::Conditional {
            format!(
                "{}\n\n---\nYou are executing a scheduled task. If the result warrants notifying \
                 the user, begin your response with [NOTIFY] followed by the notification message. \
                 If no notification is needed, respond normally without [NOTIFY].",
                schedule.goal
            )
        } else {
            schedule.goal.clone()
        };

        let child_cancellation = self.cancellation.child_token();

        // Resolve policy from schedule configuration
        let policy = self.resolve_schedule_policy(&schedule).await;

        let result = self
            .turn_runner
            .run_scheduled_turn(
                &schedule.user_id,
                &session_id,
                prompt,
                child_cancellation,
                policy,
            )
            .await;

        let finished_at = Utc::now().to_rfc3339();

        let (status, response_text) = match result {
            Ok(text) => (ScheduleRunStatus::Success, text),
            Err(types::RuntimeError::Cancelled) => (ScheduleRunStatus::Cancelled, String::new()),
            Err(e) => {
                tracing::warn!(
                    schedule_id = %schedule.schedule_id,
                    error = %e,
                    "scheduled turn failed"
                );
                (ScheduleRunStatus::Failed, format!("Error: {e}"))
            }
        };

        // Strip [NOTIFY] marker early to produce a clean text for storage and display.
        let clean_text = if schedule.notification_policy == NotificationPolicy::Conditional
            && response_text.starts_with("[NOTIFY]")
        {
            response_text
                .strip_prefix("[NOTIFY]")
                .map(|s| s.trim_start())
                .unwrap_or(&response_text)
                .to_owned()
        } else {
            response_text.clone()
        };

        let output_summary = if clean_text.is_empty() {
            None
        } else if clean_text.len() > 500 {
            Some(format!("{}...", &clean_text[..497]))
        } else {
            Some(clean_text.clone())
        };

        let output = if clean_text.is_empty() {
            None
        } else {
            Some(clean_text.clone())
        };

        let notified = self
            .handle_notification(&schedule, status, &response_text, &clean_text)
            .await;

        let (next_run_at, new_status) = self.compute_reschedule(&schedule, status);

        // Send operational failure notifications.
        self.handle_failure_notifications(&schedule, status, &clean_text)
            .await;

        let run_record = ScheduleRunRecord {
            run_id,
            schedule_id: schedule.schedule_id.clone(),
            started_at,
            finished_at,
            status,
            output_summary,
            turn_count: 0,
            cost: 0.0,
            notified,
            output,
        };

        if let Err(e) = self
            .store
            .record_run_and_reschedule(&schedule.schedule_id, &run_record, next_run_at, new_status)
            .await
        {
            tracing::error!(
                schedule_id = %schedule.schedule_id,
                error = %e,
                "failed to record run and reschedule"
            );
        }

        if let Err(e) = self
            .store
            .prune_run_history(&schedule.schedule_id, self.config.max_run_history)
            .await
        {
            tracing::warn!(
                schedule_id = %schedule.schedule_id,
                error = %e,
                "failed to prune run history"
            );
        }
    }

    async fn handle_notification(
        &self,
        schedule: &ScheduleDefinition,
        status: ScheduleRunStatus,
        response_text: &str,
        clean_text: &str,
    ) -> bool {
        if status != ScheduleRunStatus::Success {
            return false;
        }

        let should_notify = match schedule.notification_policy {
            NotificationPolicy::Always => true,
            NotificationPolicy::Conditional => response_text.starts_with("[NOTIFY]"),
            NotificationPolicy::Never => false,
        };

        if should_notify {
            let message = match schedule.notification_policy {
                NotificationPolicy::Conditional => clean_text.to_owned(),
                _ => clean_text.to_owned(),
            };

            self.notifier
                .notify_user(
                    schedule,
                    GatewayServerFrame::ScheduledNotification(GatewayScheduledNotification {
                        schedule_id: schedule.schedule_id.clone(),
                        schedule_name: schedule.name.clone(),
                        message,
                    }),
                )
                .await;
            return true;
        }

        false
    }

    /// Send operational failure notifications when consecutive failures
    /// hit the configured threshold or when the schedule is auto-disabled.
    async fn handle_failure_notifications(
        &self,
        schedule: &ScheduleDefinition,
        status: ScheduleRunStatus,
        clean_text: &str,
    ) {
        if status == ScheduleRunStatus::Success || status == ScheduleRunStatus::Cancelled {
            return;
        }

        let new_consecutive_failures = schedule.consecutive_failures + 1;
        let name = schedule.name.as_deref().unwrap_or(&schedule.schedule_id);
        let error_summary = if clean_text.len() > 200 {
            format!("{}...", &clean_text[..197])
        } else {
            clean_text.to_owned()
        };

        // Notify when consecutive failures hit the threshold.
        if self.config.notify_after_failures > 0
            && new_consecutive_failures == self.config.notify_after_failures
        {
            let message = format!(
                "❌ Scheduled task '{}' has failed {} times in a row. Latest error: {}",
                name, new_consecutive_failures, error_summary
            );
            self.notifier
                .notify_user(
                    schedule,
                    GatewayServerFrame::ScheduledNotification(GatewayScheduledNotification {
                        schedule_id: schedule.schedule_id.clone(),
                        schedule_name: schedule.name.clone(),
                        message,
                    }),
                )
                .await;
        }

        // Notify when the schedule is about to be auto-disabled.
        if new_consecutive_failures >= self.config.auto_disable_after_failures {
            let message = format!(
                "⛔ Scheduled task '{}' was disabled after {} consecutive failures.",
                name, new_consecutive_failures
            );
            self.notifier
                .notify_user(
                    schedule,
                    GatewayServerFrame::ScheduledNotification(GatewayScheduledNotification {
                        schedule_id: schedule.schedule_id.clone(),
                        schedule_name: schedule.name.clone(),
                        message,
                    }),
                )
                .await;
        }
    }

    fn compute_reschedule(
        &self,
        schedule: &ScheduleDefinition,
        status: ScheduleRunStatus,
    ) -> (Option<String>, Option<ScheduleStatus>) {
        let is_one_shot = matches!(schedule.cadence, ScheduleCadence::Once { .. });

        if is_one_shot && status == ScheduleRunStatus::Success {
            return (None, Some(ScheduleStatus::Completed));
        }

        let consecutive_failures = if status == ScheduleRunStatus::Success {
            0
        } else {
            schedule.consecutive_failures + 1
        };

        if consecutive_failures >= self.config.auto_disable_after_failures {
            return (None, Some(ScheduleStatus::Disabled));
        }

        let now = Utc::now();
        match next_run_for_cadence(&schedule.cadence, now) {
            Ok(Some(next)) => (Some(next.to_rfc3339()), None),
            Ok(None) => (None, Some(ScheduleStatus::Completed)),
            Err(e) => {
                tracing::warn!(
                    schedule_id = %schedule.schedule_id,
                    error = %e,
                    "failed to compute next run; disabling schedule"
                );
                (None, Some(ScheduleStatus::Disabled))
            }
        }
    }

    /// Resolves the effective run policy for a scheduled execution.
    ///
    /// This combines the global SchedulerConfig limits with any per-schedule
    /// policy overrides using strictest-wins semantics (minimum for limits,
    /// intersection for tools).
    async fn resolve_schedule_policy(
        &self,
        schedule: &ScheduleDefinition,
    ) -> Option<EffectiveRunPolicy> {
        // If no per-schedule policy is defined, use SchedulerConfig defaults
        let per_run = schedule.policy.clone().unwrap_or_default();

        // Build RuntimeConfig from SchedulerConfig (global limits)
        let global_config = RuntimeConfig {
            turn_timeout_secs: 60, // Default turn timeout
            max_turns: self.config.max_turns,
            max_cost: Some(self.config.max_cost),
            context_budget: Default::default(),
            summarization: Default::default(),
        };

        // Use empty agent definition (schedules run as default agent)
        let agent_def = AgentDefinition {
            system_prompt: None,
            system_prompt_file: None,
            selection: None,
            tools: None,
            max_turns: None,
            max_cost: None,
        };

        // Get available tools from turn_runner if possible
        // For now, use empty tool list - the runtime will provide actual tools
        let available_tools: Vec<FunctionDecl> = Vec::new();

        // Merge schedule policy with SchedulerConfig using strictest-wins
        let merged_per_run = RunPolicyInput {
            max_turns: per_run
                .max_turns
                .map(|t| t.min(self.config.max_turns))
                .or(Some(self.config.max_turns)),
            max_budget_microusd: per_run.max_budget_microusd.or(
                (self.config.max_cost > 0.0).then_some((self.config.max_cost * 1_000_000.0) as u64)
            ),
            max_runtime: per_run.max_runtime,
            tool_policy: per_run.tool_policy.clone(),
        };

        // Resolve the policy
        match policy_guard::resolve_policy(
            &global_config,
            &agent_def,
            &merged_per_run,
            &available_tools,
        ) {
            Ok(policy) => {
                tracing::debug!(
                    schedule_id = %schedule.schedule_id,
                    max_turns = ?policy.max_turns,
                    budget = policy.initial_budget_microusd,
                    "resolved schedule policy"
                );
                Some(policy)
            }
            Err(e) => {
                tracing::warn!(
                    schedule_id = %schedule.schedule_id,
                    error = %e,
                    "failed to resolve schedule policy, using defaults"
                );
                // Fall back to SchedulerConfig defaults
                let fallback_per_run = RunPolicyInput {
                    max_turns: Some(self.config.max_turns),
                    max_budget_microusd: if self.config.max_cost > 0.0 {
                        Some((self.config.max_cost * 1_000_000.0) as u64)
                    } else {
                        None
                    },
                    max_runtime: None,
                    tool_policy: None,
                };
                policy_guard::resolve_policy(
                    &global_config,
                    &agent_def,
                    &fallback_per_run,
                    &available_tools,
                )
                .ok()
            }
        }
    }
}
