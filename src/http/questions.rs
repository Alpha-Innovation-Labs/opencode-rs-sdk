//! Questions API for OpenCode.
//!
//! Endpoints for listing and responding to interactive question requests.

use crate::error::Result;
use crate::http::HttpClient;
use crate::types::question::{QuestionReplyRequest, QuestionRequest};
use reqwest::Method;

/// Questions API client.
#[derive(Clone)]
pub struct QuestionsApi {
    http: HttpClient,
}

impl QuestionsApi {
    /// Create a new Questions API client.
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// List pending question requests.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn list(&self) -> Result<Vec<QuestionRequest>> {
        self.http.request_json(Method::GET, "/question", None).await
    }

    /// Reply to a question request.
    ///
    /// Returns `true` on successful acceptance by the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn reply(&self, request_id: &str, req: &QuestionReplyRequest) -> Result<bool> {
        let body = serde_json::to_value(req)?;
        self.http
            .request_json(
                Method::POST,
                &format!("/question/{}/reply", request_id),
                Some(body),
            )
            .await
    }

    /// Reject a pending question request.
    ///
    /// Returns `true` on successful rejection by the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    pub async fn reject(&self, request_id: &str) -> Result<bool> {
        self.http
            .request_json(
                Method::POST,
                &format!("/question/{}/reject", request_id),
                None,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpConfig;
    use std::time::Duration;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_list_questions() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/question"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "q-1",
                    "sessionId": "s-1",
                    "questions": [
                        {
                            "question": "Select one",
                            "header": "Decision",
                            "options": [
                                {"label": "yes", "description": "Allow"},
                                {"label": "no", "description": "Reject"}
                            ]
                        }
                    ]
                }
            ])))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let questions = QuestionsApi::new(http);
        let list = questions.list().await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "q-1");
        assert_eq!(list[0].questions[0].header, "Decision");
    }

    #[tokio::test]
    async fn test_reply_question() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/question/q-1/reply"))
            .and(body_json(serde_json::json!({
                "answers": [["yes"]]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let questions = QuestionsApi::new(http);
        let accepted = questions
            .reply(
                "q-1",
                &QuestionReplyRequest {
                    answers: vec![vec!["yes".to_string()]],
                },
            )
            .await
            .unwrap();
        assert!(accepted);
    }

    #[tokio::test]
    async fn test_reject_question() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/question/q-2/reject"))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let questions = QuestionsApi::new(http);
        let rejected = questions.reject("q-2").await.unwrap();
        assert!(rejected);
    }

    #[tokio::test]
    async fn test_question_reply_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/question/missing/reply"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "name": "NotFound",
                "message": "Question request not found"
            })))
            .mount(&mock_server)
            .await;

        let http = HttpClient::new(HttpConfig {
            base_url: mock_server.uri(),
            directory: None,
            timeout: Duration::from_secs(30),
        })
        .unwrap();

        let questions = QuestionsApi::new(http);
        let result = questions
            .reply(
                "missing",
                &QuestionReplyRequest {
                    answers: vec![vec!["yes".to_string()]],
                },
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().is_not_found());
    }
}
