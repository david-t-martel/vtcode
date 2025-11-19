//! Defines the agent's high-level operational state for the TUI.

use std::fmt;

/// Represents the high-level state of the agent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AgentState {
    #[default]
    /// The agent is idle and waiting for user input.
    Idle,
    /// The agent is processing a request or thinking.
    Thinking,
    /// The agent is executing a tool or command.
    Executing,
    /// The agent is streaming a response from the LLM.
    Streaming,
    /// The agent is waiting for user confirmation.
    Waiting,
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentState::Idle => write!(f, "Idle"),
            AgentState::Thinking => write!(f, "Thinking..."),
            AgentState::Executing => write!(f, "Executing..."),
            AgentState::Streaming => write!(f, "Streaming..."),
            AgentState::Waiting => write!(f, "Waiting for confirmation..."),
        }
    }
}
