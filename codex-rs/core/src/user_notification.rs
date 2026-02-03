use serde::Serialize;
use tracing::error;
use tracing::warn;

/// Manages sending notifications to an external program configured by the user.
///
/// The notifier invokes the configured command with a JSON payload as an argument
/// for each notification event. This enables external tools to monitor Codex sessions.
#[derive(Debug, Default, Clone)]
pub struct UserNotifier {
    notify_command: Option<Vec<String>>,
}

impl UserNotifier {
    /// Send a notification to the configured external program.
    ///
    /// If no notify command is configured, this is a no-op.
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

    /// Create a new UserNotifier with the given command.
    ///
    /// The command is a vector of strings where the first element is the program
    /// and subsequent elements are arguments. The JSON notification payload will
    /// be appended as the final argument.
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

/// Notification events sent to an external program configured via `notify` in config.
///
/// Each notification is serialized as JSON and passed as an argument to the configured
/// program. This enables external tools (like session monitors) to track Codex activity.
///
/// # Events
///
/// | Event | When Fired | Typical State |
/// |-------|------------|---------------|
/// | `session-start` | New session begins | idle |
/// | `session-end` | Session ends | (remove session) |
/// | `user-prompt-submit` | User submits a prompt | busy |
/// | `approval-requested` | Agent needs user approval | permission |
/// | `approval-response` | User responds to approval | busy or idle |
/// | `turn-cancelled` | User interrupts agent (Escape) | idle |
/// | `agent-turn-complete` | Agent completes a turn | idle |
///
/// # Example Payloads
///
/// ```json
/// {"type":"session-start","thread-id":"uuid","cwd":"/path","pid":12345}
/// {"type":"approval-requested","thread-id":"uuid","turn-id":"1","approval-type":"exec","description":"cargo build"}
/// {"type":"approval-response","thread-id":"uuid","turn-id":"1","approved":true}
/// ```
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum UserNotification {
    /// Fired when a new session/conversation starts.
    ///
    /// External tools should create a new session entry when receiving this event.
    #[serde(rename_all = "kebab-case")]
    SessionStart {
        thread_id: String,
        cwd: String,
        /// Process ID of the Codex process.
        pid: u32,
    },

    /// Fired when a session/conversation ends.
    ///
    /// External tools should remove the session entry when receiving this event.
    #[serde(rename_all = "kebab-case")]
    SessionEnd { thread_id: String },

    /// Fired when the user submits a prompt (agent starts working).
    ///
    /// This indicates the agent is now busy processing the user's request.
    #[serde(rename_all = "kebab-case")]
    UserPromptSubmit {
        thread_id: String,
        turn_id: String,
        cwd: String,
        /// The prompt text submitted by the user.
        prompt: String,
    },

    /// Fired when the agent needs user approval (exec, patch, or elicitation).
    ///
    /// This indicates the agent is waiting for user input. The `approval_type` field
    /// indicates what kind of approval is needed.
    ///
    /// For exec and patch approvals, `turn_id` is set.
    /// For MCP elicitation approvals, `request_id` is set.
    #[serde(rename_all = "kebab-case")]
    ApprovalRequested {
        thread_id: String,
        /// Present for exec/patch approvals - the Codex turn ID.
        #[serde(skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        /// Present for MCP elicitation approvals - the MCP request ID.
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        approval_type: ApprovalType,
        /// Human-readable description of what needs approval.
        description: String,
    },

    /// Fired when the user responds to an approval request.
    ///
    /// If `approved` is true, the agent continues working (busy state).
    /// If `approved` is false, the agent stops and returns to idle.
    ///
    /// For exec and patch approvals, `turn_id` is set.
    /// For MCP elicitation approvals, `request_id` is set.
    #[serde(rename_all = "kebab-case")]
    ApprovalResponse {
        thread_id: String,
        /// Present for exec/patch approvals - the Codex turn ID.
        #[serde(skip_serializing_if = "Option::is_none")]
        turn_id: Option<String>,
        /// Present for MCP elicitation approvals - the MCP request ID.
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        /// Whether the user approved the request.
        approved: bool,
    },

    /// Fired when the agent completes a turn.
    ///
    /// This indicates the agent has finished processing and returned to idle.
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

    /// Fired when the user cancels/interrupts the agent while it's working.
    ///
    /// This is triggered when the user presses Escape to stop the agent.
    /// The agent returns to idle state.
    #[serde(rename_all = "kebab-case")]
    TurnCancelled { thread_id: String, turn_id: String },
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
            turn_id: Some("1".to_string()),
            request_id: None,
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
            turn_id: Some("1".to_string()),
            request_id: None,
            approval_type: ApprovalType::Patch,
            description: "Edit src/main.rs".to_string(),
        };
        let serialized = serde_json::to_string(&notification)?;
        assert!(serialized.contains(r#""approval-type":"patch""#));
        assert!(serialized.contains(r#""turn-id":"1""#));
        assert!(!serialized.contains("request-id"));
        Ok(())
    }

    #[test]
    fn test_approval_requested_elicitation() -> Result<()> {
        let notification = UserNotification::ApprovalRequested {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: None,
            request_id: Some("mcp-req-123".to_string()),
            approval_type: ApprovalType::Elicitation,
            description: "OAuth consent required".to_string(),
        };
        let serialized = serde_json::to_string(&notification)?;
        assert!(serialized.contains(r#""approval-type":"elicitation""#));
        assert!(serialized.contains(r#""request-id":"mcp-req-123""#));
        assert!(!serialized.contains("turn-id"));
        Ok(())
    }

    #[test]
    fn test_approval_response_exec() -> Result<()> {
        let notification = UserNotification::ApprovalResponse {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: Some("1".to_string()),
            request_id: None,
            approved: true,
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"approval-response","thread-id":"b5f6c1c2-1111-2222-3333-444455556666","turn-id":"1","approved":true}"#
        );
        Ok(())
    }

    #[test]
    fn test_approval_response_elicitation() -> Result<()> {
        let notification = UserNotification::ApprovalResponse {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: None,
            request_id: Some("mcp-req-123".to_string()),
            approved: false,
        };
        let serialized = serde_json::to_string(&notification)?;
        assert!(serialized.contains(r#""request-id":"mcp-req-123""#));
        assert!(serialized.contains(r#""approved":false"#));
        assert!(!serialized.contains("turn-id"));
        Ok(())
    }

    #[test]
    fn test_turn_cancelled() -> Result<()> {
        let notification = UserNotification::TurnCancelled {
            thread_id: "b5f6c1c2-1111-2222-3333-444455556666".to_string(),
            turn_id: "1".to_string(),
        };
        let serialized = serde_json::to_string(&notification)?;
        assert_eq!(
            serialized,
            r#"{"type":"turn-cancelled","thread-id":"b5f6c1c2-1111-2222-3333-444455556666","turn-id":"1"}"#
        );
        Ok(())
    }
}
