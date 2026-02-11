//! High-level client API for OpenCode.
//!
//! This module provides the ergonomic `Client` and `ClientBuilder` types.

use crate::error::{OpencodeError, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[cfg(feature = "http")]
use crate::http::{HttpClient, HttpConfig};

/// OpenCode client for interacting with the server.
#[derive(Clone)]
pub struct Client {
    #[cfg(feature = "http")]
    http: HttpClient,
    // Used by SSE subscriber for reconnection (Phase 5)
    #[allow(dead_code)]
    last_event_id: Arc<RwLock<Option<String>>>,
    #[cfg(all(feature = "http", feature = "sse"))]
    session_event_router: Arc<RwLock<Option<crate::sse::SessionEventRouter>>>,
}

/// Builder for creating a [`Client`].
#[derive(Clone)]
pub struct ClientBuilder {
    base_url: String,
    directory: Option<String>,
    timeout: Duration,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:4096".to_string(),
            directory: None,
            timeout: Duration::from_secs(300), // 5 min for long AI requests
        }
    }
}

impl ClientBuilder {
    /// Create a new client builder with default settings.
    ///
    /// Default settings:
    /// - Base URL: `http://127.0.0.1:4096`
    /// - Timeout: 300 seconds (5 minutes)
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the base URL for the OpenCode server.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the directory context for requests.
    ///
    /// This sets the `x-opencode-directory` header on all requests.
    pub fn directory(mut self, dir: impl Into<String>) -> Self {
        self.directory = Some(dir.into());
        self
    }

    /// Set the request timeout in seconds.
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    /// Build the client.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built or if the
    /// `http` feature is not enabled.
    #[cfg(feature = "http")]
    pub fn build(self) -> Result<Client> {
        let http = HttpClient::new(HttpConfig {
            base_url: self.base_url.trim_end_matches('/').to_string(),
            directory: self.directory,
            timeout: self.timeout,
        })?;

        Ok(Client {
            http,
            last_event_id: Arc::new(RwLock::new(None)),
            #[cfg(all(feature = "http", feature = "sse"))]
            session_event_router: Arc::new(RwLock::new(None)),
        })
    }

    /// Build the client.
    ///
    /// # Errors
    ///
    /// Returns an error because the `http` feature is required.
    #[cfg(not(feature = "http"))]
    pub fn build(self) -> Result<Client> {
        Err(OpencodeError::InvalidConfig(
            "http feature required to build client".into(),
        ))
    }
}

impl Client {
    /// Create a new client builder.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Get the sessions API.
    #[cfg(feature = "http")]
    pub fn sessions(&self) -> crate::http::sessions::SessionsApi {
        crate::http::sessions::SessionsApi::new(self.http.clone())
    }

    /// Get the messages API.
    #[cfg(feature = "http")]
    pub fn messages(&self) -> crate::http::messages::MessagesApi {
        crate::http::messages::MessagesApi::new(self.http.clone())
    }

    /// Get the parts API.
    #[cfg(feature = "http")]
    pub fn parts(&self) -> crate::http::parts::PartsApi {
        crate::http::parts::PartsApi::new(self.http.clone())
    }

    /// Get the permissions API.
    #[cfg(feature = "http")]
    pub fn permissions(&self) -> crate::http::permissions::PermissionsApi {
        crate::http::permissions::PermissionsApi::new(self.http.clone())
    }

    /// Get the questions API.
    #[cfg(feature = "http")]
    pub fn questions(&self) -> crate::http::questions::QuestionsApi {
        crate::http::questions::QuestionsApi::new(self.http.clone())
    }

    /// Get the files API.
    #[cfg(feature = "http")]
    pub fn files(&self) -> crate::http::files::FilesApi {
        crate::http::files::FilesApi::new(self.http.clone())
    }

    /// Get the find API.
    #[cfg(feature = "http")]
    pub fn find(&self) -> crate::http::find::FindApi {
        crate::http::find::FindApi::new(self.http.clone())
    }

    /// Get the providers API.
    #[cfg(feature = "http")]
    pub fn providers(&self) -> crate::http::providers::ProvidersApi {
        crate::http::providers::ProvidersApi::new(self.http.clone())
    }

    /// Get the MCP API.
    #[cfg(feature = "http")]
    pub fn mcp(&self) -> crate::http::mcp::McpApi {
        crate::http::mcp::McpApi::new(self.http.clone())
    }

    /// Get the PTY API.
    #[cfg(feature = "http")]
    pub fn pty(&self) -> crate::http::pty::PtyApi {
        crate::http::pty::PtyApi::new(self.http.clone())
    }

    /// Get the config API.
    #[cfg(feature = "http")]
    pub fn config(&self) -> crate::http::config::ConfigApi {
        crate::http::config::ConfigApi::new(self.http.clone())
    }

    /// Get the tools API.
    #[cfg(feature = "http")]
    pub fn tools(&self) -> crate::http::tools::ToolsApi {
        crate::http::tools::ToolsApi::new(self.http.clone())
    }

    /// Get the project API.
    #[cfg(feature = "http")]
    pub fn project(&self) -> crate::http::project::ProjectApi {
        crate::http::project::ProjectApi::new(self.http.clone())
    }

    /// Get the worktree API.
    #[cfg(feature = "http")]
    pub fn worktree(&self) -> crate::http::worktree::WorktreeApi {
        crate::http::worktree::WorktreeApi::new(self.http.clone())
    }

    /// Get the misc API.
    #[cfg(feature = "http")]
    pub fn misc(&self) -> crate::http::misc::MiscApi {
        crate::http::misc::MiscApi::new(self.http.clone())
    }

    /// Simple helper to create session and send a text prompt.
    ///
    /// Note: This method returns immediately after sending the prompt.
    /// The AI response will arrive asynchronously via SSE events.
    /// Use [`subscribe_session`] to receive the response.
    ///
    /// # Errors
    ///
    /// Returns an error if session creation or prompt fails.
    #[cfg(feature = "http")]
    pub async fn run_simple_text(
        &self,
        text: impl Into<String>,
    ) -> Result<crate::types::session::Session> {
        use crate::types::message::{PromptPart, PromptRequest};
        use crate::types::session::CreateSessionRequest;

        let session = self
            .sessions()
            .create(&CreateSessionRequest::default())
            .await?;

        self.messages()
            .prompt(
                &session.id,
                &PromptRequest {
                    parts: vec![PromptPart::Text {
                        text: text.into(),
                        synthetic: None,
                        ignored: None,
                        metadata: None,
                    }],
                    message_id: None,
                    model: None,
                    agent: None,
                    no_reply: None,
                    system: None,
                    variant: None,
                },
            )
            .await?;

        Ok(session)
    }

    /// Create a session with a title.
    ///
    /// This convenience helper wraps [`crate::http::sessions::SessionsApi::create_with`].
    ///
    /// # Errors
    ///
    /// Returns an error if session creation fails.
    #[cfg(feature = "http")]
    pub async fn create_session_with_title(
        &self,
        title: impl Into<String>,
    ) -> Result<crate::types::session::Session> {
        self.sessions()
            .create_with(crate::types::session::SessionCreateOptions::new().with_title(title))
            .await
    }

    /// Send plain text asynchronously to a session.
    ///
    /// This convenience helper wraps [`crate::http::messages::MessagesApi::send_text_async`].
    /// The server returns an empty body on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    #[cfg(feature = "http")]
    pub async fn send_text_async(
        &self,
        session_id: &str,
        text: impl Into<String>,
        model: Option<crate::types::project::ModelRef>,
    ) -> Result<()> {
        self.messages()
            .send_text_async(session_id, text, model)
            .await
    }

    /// Send plain text asynchronously to a session object.
    ///
    /// The server returns an empty body on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    #[cfg(feature = "http")]
    pub async fn send_text_async_for_session(
        &self,
        session: &crate::types::session::Session,
        text: impl Into<String>,
        model: Option<crate::types::project::ModelRef>,
    ) -> Result<()> {
        self.send_text_async(&session.id, text, model).await
    }

    /// Set the last event ID (for SSE reconnection).
    #[cfg(feature = "sse")]
    #[allow(dead_code)] // Used by SSE subscriber in Phase 5
    pub(crate) async fn set_last_event_id(&self, id: Option<String>) {
        *self.last_event_id.write().await = id;
    }

    /// Get the last event ID.
    #[cfg(feature = "sse")]
    #[allow(dead_code)] // Used by SSE subscriber in Phase 5
    pub(crate) async fn last_event_id(&self) -> Option<String> {
        self.last_event_id.read().await.clone()
    }

    /// Get the HTTP client.
    #[cfg(feature = "http")]
    #[allow(dead_code)] // May be used by external crates
    pub(crate) fn http(&self) -> &HttpClient {
        &self.http
    }

    /// Get the last event ID handle for SSE.
    #[cfg(feature = "sse")]
    #[allow(dead_code)] // May be used by external crates
    pub(crate) fn last_event_id_handle(&self) -> Arc<RwLock<Option<String>>> {
        self.last_event_id.clone()
    }

    #[cfg(all(feature = "http", feature = "sse"))]
    async fn default_session_event_router(&self) -> Result<crate::sse::SessionEventRouter> {
        if let Some(existing) = self.session_event_router.read().await.clone() {
            return Ok(existing);
        }

        let router = self
            .sse_subscriber()
            .session_event_router(crate::sse::SessionEventRouterOptions::default())
            .await?;

        let mut guard = self.session_event_router.write().await;
        if let Some(existing) = guard.clone() {
            return Ok(existing);
        }

        *guard = Some(router.clone());
        Ok(router)
    }
}

#[cfg(all(feature = "http", feature = "sse"))]
impl Client {
    /// Wait until a session reaches idle state and collect streamed assistant text.
    ///
    /// This helper listens to `message.part.updated` and idle/status events for the session.
    ///
    /// # Errors
    ///
    /// Returns an error if the stream closes, times out, or the session emits `session.error`.
    pub async fn wait_for_idle_text(&self, session_id: &str, timeout: Duration) -> Result<String> {
        let subscription = self.subscribe_session(session_id).await?;
        self.collect_idle_text(subscription, timeout).await
    }

    /// Send text asynchronously and wait until session idle while collecting text output.
    ///
    /// This subscribes before sending to avoid missing early stream events.
    ///
    /// # Errors
    ///
    /// Returns an error if sending fails, stream closes, times out, or session emits `session.error`.
    pub async fn send_text_async_and_wait_for_idle(
        &self,
        session_id: &str,
        text: impl Into<String>,
        model: Option<crate::types::project::ModelRef>,
        timeout: Duration,
    ) -> Result<String> {
        let subscription = self.subscribe_session(session_id).await?;
        self.send_text_async(session_id, text, model).await?;
        self.collect_idle_text(subscription, timeout).await
    }

    async fn collect_idle_text(
        &self,
        mut subscription: crate::sse::SseSubscription,
        timeout: Duration,
    ) -> Result<String> {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        let deadline = tokio::time::Instant::now() + timeout;
        let mut output = String::new();

        loop {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                return Err(OpencodeError::ServerTimeout { timeout_ms });
            }

            let remaining = deadline.saturating_duration_since(now);
            let event = match tokio::time::timeout(remaining, subscription.recv()).await {
                Ok(Some(event)) => event,
                Ok(None) => return Err(OpencodeError::StreamClosed),
                Err(_) => return Err(OpencodeError::ServerTimeout { timeout_ms }),
            };

            match event {
                crate::types::event::Event::MessagePartUpdated { properties } => {
                    if let Some(delta) = properties.delta.as_deref()
                        && matches!(
                            properties.part.as_ref(),
                            Some(crate::types::message::Part::Text { .. })
                        )
                    {
                        output.push_str(delta);
                    }
                }
                crate::types::event::Event::SessionStatus { properties } => {
                    let is_idle = properties
                        .get("status")
                        .and_then(|status| status.get("type"))
                        .and_then(serde_json::Value::as_str)
                        == Some("idle");
                    if is_idle {
                        break;
                    }
                }
                crate::types::event::Event::SessionIdle { .. } => break,
                crate::types::event::Event::SessionError { properties } => {
                    return Err(OpencodeError::State(format!(
                        "session.error: {:?}",
                        properties.error
                    )));
                }
                _ => {}
            }
        }

        Ok(output.trim().to_string())
    }

    /// Get an SSE subscriber for streaming events.
    pub fn sse_subscriber(&self) -> crate::sse::SseSubscriber {
        crate::sse::SseSubscriber::new(
            self.http.base().to_string(),
            self.http.directory().map(|s| s.to_string()),
            self.last_event_id.clone(),
        )
    }

    /// Subscribe to all events for the configured directory with default options.
    ///
    /// This subscribes to the `/event` endpoint which streams all events
    /// for the directory specified in the client configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe(&self) -> Result<crate::sse::SseSubscription> {
        self.subscribe_typed().await
    }

    /// Subscribe to all events for the configured directory as typed events.
    ///
    /// This is equivalent to [`Self::subscribe`], but explicitly named to
    /// distinguish it from [`Self::subscribe_raw`].
    pub async fn subscribe_typed(&self) -> Result<crate::sse::SseSubscription> {
        self.sse_subscriber()
            .subscribe_typed(crate::sse::SseOptions::default())
            .await
    }

    /// Subscribe to events filtered by session ID with default options.
    ///
    /// Events are filtered client-side to only include events matching
    /// the specified session ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_session(&self, session_id: &str) -> Result<crate::sse::SseSubscription> {
        let router = self.default_session_event_router().await?;
        Ok(router.subscribe(session_id).await)
    }

    /// Create a new session event router with one upstream `/event` subscription.
    ///
    /// This returns a dedicated router instance and does not modify the default
    /// cached router used by [`Self::subscribe_session`].
    pub async fn session_event_router_with_options(
        &self,
        opts: crate::sse::SessionEventRouterOptions,
    ) -> Result<crate::sse::SessionEventRouter> {
        self.sse_subscriber().session_event_router(opts).await
    }

    /// Get the default session event router.
    ///
    /// The first call lazily creates the router; subsequent calls return the
    /// same router instance.
    pub async fn session_event_router(&self) -> Result<crate::sse::SessionEventRouter> {
        self.default_session_event_router().await
    }

    /// Subscribe to raw JSON frames from `/event` for debugging.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_raw(&self) -> Result<crate::sse::RawSseSubscription> {
        self.sse_subscriber()
            .subscribe_raw(crate::sse::SseOptions::default())
            .await
    }

    /// Subscribe to global events with default options (all directories).
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_global(&self) -> Result<crate::sse::SseSubscription> {
        self.subscribe_typed_global().await
    }

    /// Subscribe to global events as typed events (all directories).
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_typed_global(&self) -> Result<crate::sse::SseSubscription> {
        self.sse_subscriber()
            .subscribe_typed_global(crate::sse::SseOptions::default())
            .await
    }
}

#[cfg(test)]
mod tests {
    // TODO(3): Add integration tests with mocked HTTP/SSE backends for Client API methods
    use super::*;

    #[test]
    fn test_client_builder_defaults() {
        let builder = ClientBuilder::new();
        assert_eq!(builder.base_url, "http://127.0.0.1:4096");
        assert_eq!(builder.timeout, Duration::from_secs(300));
        assert!(builder.directory.is_none());
    }

    #[test]
    fn test_client_builder_customization() {
        let builder = ClientBuilder::new()
            .base_url("http://localhost:8080")
            .directory("/my/project")
            .timeout_secs(60);

        assert_eq!(builder.base_url, "http://localhost:8080");
        assert_eq!(builder.directory, Some("/my/project".to_string()));
        assert_eq!(builder.timeout, Duration::from_secs(60));
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_client_build() {
        let client = ClientBuilder::new().build();
        assert!(client.is_ok());
    }
}
