//! Question types for opencode_rs.
//!
//! These types model interactive question/answer prompts exposed by the
//! OpenCode HTTP API.

use serde::{Deserialize, Serialize};

/// A selectable option for a question.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionOption {
    /// Label returned when selected.
    pub label: String,
    /// Human-readable description shown to users.
    pub description: String,
}

/// A single question item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionInfo {
    /// The question text.
    pub question: String,
    /// Short header/title for the question.
    pub header: String,
    /// Available options.
    pub options: Vec<QuestionOption>,
    /// Whether multiple options can be selected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiple: Option<bool>,
    /// Whether custom free-form input is allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<bool>,
}

/// Optional tool context for a question request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct QuestionToolRef {
    /// Message ID containing the originating tool call.
    pub message_id: String,
    /// Tool call ID.
    pub call_id: String,
}

/// A pending question request from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRequest {
    /// Unique request identifier.
    pub id: String,
    /// Session ID that this question belongs to.
    pub session_id: String,
    /// Question list (typically one item, but API supports many).
    pub questions: Vec<QuestionInfo>,
    /// Optional tool reference context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<QuestionToolRef>,
}

/// One answer entry corresponding to one question.
///
/// Each string is a selected option label.
pub type QuestionAnswer = Vec<String>;

/// Request body for replying to a question.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionReplyRequest {
    /// User answers in order of questions.
    pub answers: Vec<QuestionAnswer>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_question_request_deserialize() {
        let json = r#"{
            "id": "q-1",
            "sessionId": "s-1",
            "questions": [
                {
                    "question": "Pick one",
                    "header": "Choice",
                    "options": [
                        {"label": "yes", "description": "Allow"},
                        {"label": "no", "description": "Reject"}
                    ],
                    "multiple": false,
                    "custom": true
                }
            ],
            "tool": {"messageId": "m-1", "callId": "c-1"}
        }"#;

        let req: QuestionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.id, "q-1");
        assert_eq!(req.session_id, "s-1");
        assert_eq!(req.questions.len(), 1);
        assert_eq!(req.questions[0].header, "Choice");
        assert!(req.tool.is_some());
    }

    #[test]
    fn test_question_reply_request_serialize() {
        let body = QuestionReplyRequest {
            answers: vec![vec!["yes".to_string()], vec!["custom-value".to_string()]],
        };

        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains(r#""answers":[["yes"],["custom-value"]]"#));
    }
}
