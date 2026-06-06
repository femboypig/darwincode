use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TodoStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for TodoStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoPriority {
    High,
    Medium,
    Low,
}

impl TodoPriority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

impl fmt::Display for TodoPriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TodoItem {
    pub content: String,
    pub status: TodoStatus,
    pub priority: TodoPriority,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ChatMessage;
    use crate::config::StoredConfig;

    #[test]
    fn test_todo_lifecycle_validation() {
        use crate::app::App;

        // Create app with default config
        let mut app = App::new(Some(StoredConfig::default()));
        app.chat.session_id = "test_mock_todo_validation".to_owned();

        // Case 1: Initial list (one pending, one in_progress)
        app.chat.history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![serde_json::json!({
                "functionCall": {
                    "name": "todo",
                    "args": {
                        "todos": [
                            { "content": "Task 1", "status": "pending", "priority": "high" },
                            { "content": "Task 2", "status": "in_progress", "priority": "medium" }
                        ]
                    }
                }
            })],
        });

        app.complete_function_execution("todo".to_string(), serde_json::json!({ "success": true }));
        assert_eq!(app.chat.todos.len(), 2);
        assert_eq!(app.chat.todos[0].content, "Task 1");
        assert_eq!(app.chat.todos[0].status, TodoStatus::Pending);
        assert_eq!(app.chat.todos[1].content, "Task 2");
        assert_eq!(app.chat.todos[1].status, TodoStatus::InProgress);

        // Case 2: Transition pending -> completed directly (validation is relaxed, should succeed)
        app.chat.history.push(ChatMessage {
            role: "model".to_owned(),
            parts: vec![serde_json::json!({
                "functionCall": {
                    "name": "todo",
                    "args": {
                        "todos": [
                            { "content": "Task 1", "status": "completed", "priority": "high" },
                            { "content": "Task 2", "status": "in_progress", "priority": "medium" }
                        ]
                    }
                }
            })],
        });

        app.complete_function_execution("todo".to_string(), serde_json::json!({ "success": true }));
        assert_eq!(app.chat.todos[0].status, TodoStatus::Completed);
    }
}
