use serde::Serialize;
use tracing::error;
use tracing::warn;

#[derive(Debug, Default, Clone)]
pub struct UserNotifier {
    notify_command: Option<Vec<String>>,
}

impl UserNotifier {
    pub fn notify(&self, notification: &UserNotification) {
        if let Some(notify_command) = &self.notify_command
            && !notify_command.is_empty()
        {
            self.invoke_notify(notify_command, notification)
        }
    }

    fn invoke_notify(&self, notify_command: &[String], notification: &UserNotification) {
        let Ok(json) = serde_json::to_string(&notification) else {
            error!("failed to serialise notification payload");
            return;
        };

        let mut command = std::process::Command::new(&notify_command[0]);
        if notify_command.len() > 1 {
            command.args(&notify_command[1..]);
        }
        command.arg(json);

        // Fire-and-forget – we do not wait for completion.
        if let Err(e) = command.spawn() {
            warn!("failed to spawn notifier '{}': {e}", notify_command[0]);
        }
    }

    pub fn new(notify: Option<Vec<String>>) -> Self {
        Self {
            notify_command: notify,
        }
    }
}

/// Type of approval being requested from the user.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalType {
    /// Shell command execution approval
    Exec,
    /// File edit/patch approval
    Patch,
    /// MCP tool input (elicitation) approval
    Elicitation,
}

/// User can configure a program that will receive notifications. Each
/// notification is serialized as JSON and passed as an argument to the
/// program.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UserNotification {
    /// Fired when a new session/conversation starts.
    #[serde(rename_all = "kebab-case")]
    SessionStart {
        thread_id: String,
        cwd: String,
        /// Process ID of the Codex process.
        pid: u32,
    },

    /// Fired when a session/conversation ends.
    #[serde(rename_all = "kebab-case")]
    SessionEnd { thread_id: String },

    /// Fired when the user submits a prompt (agent starts working).
    #[serde(rename_all = "kebab-case")]
    UserPromptSubmit {
        thread_id: String,
        turn_id: String,
        cwd: String,
        /// The prompt text submitted by the user.
        prompt: String,
    },

    /// Fired when the agent needs user approval (exec, patch, or elicitation).
    #[serde(rename_all = "kebab-case")]
    ApprovalRequested {
        thread_id: String,
        turn_id: String,
        approval_type: ApprovalType,
        /// Human-readable description of what needs approval.
        description: String,
    },

    /// Fired when the user responds to an approval request.
    #[serde(rename_all = "kebab-case")]
    ApprovalResponse {
        thread_id: String,
        turn_id: String,
        /// Whether the user approved the request.
        approved: bool,
    },

    /// Fired when the agent completes a turn (existing event, unchanged).
    #[serde(rename_all = "kebab-case")]
    AgentTurnComplete {
        thread_id: String,
        turn_id: String,
        cwd: String,

        /// Messages that the user sent to the agent to initiate the turn.
        input_messages: Vec<String>,

        /// The last message sent by the assistant in the turn.
        last_assistant_message: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn test_agent_turn_complete() -> Result<()> {
        let notification = UserNotification::AgentTurnComplete {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: "12345".to_string(),
            cwd: "/Users/example/project".to_string(),
            input_messages: vec!["Rename `foo` to `bar` and update the callsites.".to_string()],
            last_assistant_message: Some(
                "Rename complete and verified `cargo build` succeeds.".to_string(),
            ),
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"agent-turn-complete","thread-id":"b5f6c1c2-1111-2222-3333-444455556666","turn-id":"12345","cwd":"/Users/example/project","input-messages":["Rename `foo` to `bar` and update the callsites."],"last-assistant-message":"Rename complete and verified `cargo build` succeeds."}"#
        );
        Ok(())
    }

    #[test]
    fn test_session_start() -> Result<()> {
        let notification = UserNotification::SessionStart {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            cwd: "/Users/example/project".to_string(),
            pid: 12345,
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"session-start","thread-id":"b5f6c1c2-1111-2222-3333-444455556666","cwd":"/Users/example/project","pid":12345}"#
        );
        Ok(())
    }

    #[test]
    fn test_session_end() -> Result<()> {
        let notification = UserNotification::SessionEnd {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"session-end","thread-id":"b5f6c1c2-1111-2222-3333-444455556666"}"#
        );
        Ok(())
    }

    #[test]
    fn test_user_prompt_submit() -> Result<()> {
        let notification = UserNotification::UserPromptSubmit {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: "1".to_string(),
            cwd: "/Users/example/project".to_string(),
            prompt: "Fix the bug in main.rs".to_string(),
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"user-prompt-submit","thread-id":"b5f6c1c2-1111-2222-3333-444455556666","turn-id":"1","cwd":"/Users/example/project","prompt":"Fix the bug in main.rs"}"#
        );
        Ok(())
    }

    #[test]
    fn test_approval_requested_exec() -> Result<()> {
        let notification = UserNotification::ApprovalRequested {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: "1".to_string(),
            approval_type: ApprovalType::Exec,
            description: "cargo build --release".to_string(),
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"approval-requested","thread-id":"b5f6c1c2-1111-2222-3333-444455556666","turn-id":"1","approval-type":"exec","description":"cargo build --release"}"#
        );
        Ok(())
    }

    #[test]
    fn test_approval_requested_patch() -> Result<()> {
        let notification = UserNotification::ApprovalRequested {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: "1".to_string(),
            approval_type: ApprovalType::Patch,
            description: "Edit src/main.rs".to_string(),
        };
        let serialized = serde_json::to_string(&notification)?;
        assert!(serialized.contains(r#""approval-type":"patch""#));
        Ok(())
    }

    #[test]
    fn test_approval_response() -> Result<()> {
        let notification = UserNotification::ApprovalResponse {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: "1".to_string(),
            approved: true,
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"approval-response","thread-id":"b5f6c1c2-1111-2222-3333-444455556666","turn-id":"1","approved":true}"#
        );
        Ok(())
    }
}
