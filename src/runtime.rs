//! Higher-level runtime helpers combining managed server + client setup.

use crate::client::{Client, ClientBuilder};
use crate::error::{OpencodeError, Result};
use crate::server::{ManagedServer, ServerOptions};
use std::path::PathBuf;
use std::time::Duration;

/// Managed runtime context that owns both a server process and connected client.
pub struct ManagedRuntime {
    server: ManagedServer,
    client: Client,
    request_directory: String,
}

/// Builder for [`ManagedRuntime`].
#[derive(Debug, Clone)]
pub struct ManagedRuntimeBuilder {
    server_options: ServerOptions,
    request_directory: Option<String>,
}

impl Default for ManagedRuntimeBuilder {
    fn default() -> Self {
        Self {
            server_options: ServerOptions::new()
                .hostname("127.0.0.1")
                .startup_timeout_ms(10_000),
            request_directory: None,
        }
    }
}

impl ManagedRuntimeBuilder {
    /// Create a new managed runtime builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set server hostname.
    pub fn hostname(mut self, hostname: impl Into<String>) -> Self {
        self.server_options = self.server_options.hostname(hostname);
        self
    }

    /// Set server port.
    pub fn port(mut self, port: u16) -> Self {
        self.server_options = self.server_options.port(port);
        self
    }

    /// Set server startup timeout in milliseconds.
    pub fn startup_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.server_options = self.server_options.startup_timeout_ms(timeout_ms);
        self
    }

    /// Set server startup timeout as a [`Duration`].
    pub fn startup_timeout(mut self, timeout: Duration) -> Self {
        let timeout_ms = u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX);
        self.server_options = self.server_options.startup_timeout_ms(timeout_ms);
        self
    }

    /// Set working directory for server process and request header.
    pub fn directory(mut self, directory: impl Into<PathBuf>) -> Self {
        let directory = directory.into();
        self.request_directory = Some(directory.to_string_lossy().to_string());
        self.server_options = self.server_options.directory(directory);
        self
    }

    /// Override the request directory sent as `x-opencode-directory`.
    pub fn request_directory(mut self, directory: impl Into<String>) -> Self {
        self.request_directory = Some(directory.into());
        self
    }

    /// Set server config JSON via `OPENCODE_CONFIG_CONTENT`.
    pub fn config_json(mut self, config_json: impl Into<String>) -> Self {
        self.server_options = self.server_options.config_json(config_json);
        self
    }

    /// Set opencode binary path.
    pub fn binary(mut self, binary: impl Into<String>) -> Self {
        self.server_options = self.server_options.binary(binary);
        self
    }

    /// Start managed runtime using configured options.
    pub async fn start(mut self) -> Result<ManagedRuntime> {
        let request_directory = if let Some(request_directory) = self.request_directory {
            request_directory
        } else {
            let cwd = std::env::current_dir()?;
            if self.server_options.directory.is_none() {
                self.server_options = self.server_options.directory(cwd.clone());
            }
            cwd.to_string_lossy().to_string()
        };

        ManagedRuntime::start(self.server_options, request_directory).await
    }
}

impl ManagedRuntime {
    /// Create a managed runtime builder.
    pub fn builder() -> ManagedRuntimeBuilder {
        ManagedRuntimeBuilder::new()
    }

    /// Start a managed server in the current working directory and connect a client.
    ///
    /// Uses `127.0.0.1`, random port, and a 10 second startup timeout.
    pub async fn start_for_cwd() -> Result<Self> {
        Self::builder().start().await
    }

    /// Start a managed server with custom options and connect a client.
    pub async fn start(opts: ServerOptions, request_directory: impl Into<String>) -> Result<Self> {
        let request_directory = request_directory.into();
        let timeout_ms = opts.startup_timeout_ms;

        let server = ManagedServer::start(opts).await?;
        let base_url = server.url().to_string();
        let client = ClientBuilder::new()
            .base_url(base_url.clone())
            .directory(&request_directory)
            .build()?;

        if !wait_for_session_api_ready(&base_url, &request_directory, timeout_ms).await {
            tracing::warn!(
                "Managed runtime startup timed out endpoint={}/session directory={} timeout_ms={}",
                base_url,
                request_directory,
                timeout_ms
            );
            return Err(OpencodeError::ServerTimeout { timeout_ms });
        }

        Ok(Self {
            server,
            client,
            request_directory,
        })
    }

    /// Access the connected client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Access the managed server.
    pub fn server(&self) -> &ManagedServer {
        &self.server
    }

    /// Get request directory used for `x-opencode-directory`.
    pub fn request_directory(&self) -> &str {
        &self.request_directory
    }

    /// Stop the managed server.
    ///
    /// This is optional because dropping the runtime also stops the server.
    pub async fn stop(self) -> Result<()> {
        self.server.stop().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_managed_runtime_builder_defaults() {
        let builder = ManagedRuntimeBuilder::new();
        assert_eq!(builder.server_options.hostname, "127.0.0.1");
        assert_eq!(builder.server_options.startup_timeout_ms, 10_000);
        assert!(builder.server_options.port.is_none());
        assert!(builder.server_options.directory.is_none());
        assert!(builder.request_directory.is_none());
    }

    #[test]
    fn test_managed_runtime_builder_directory_sets_request_directory() {
        let builder = ManagedRuntimeBuilder::new().directory("/tmp/opencode");
        assert_eq!(builder.request_directory.as_deref(), Some("/tmp/opencode"));
        assert_eq!(
            builder.server_options.directory,
            Some(PathBuf::from("/tmp/opencode"))
        );
    }
}

async fn wait_for_session_api_ready(base_url: &str, directory: &str, timeout_ms: u64) -> bool {
    let session_url = format!("{}/session", base_url.trim_end_matches('/'));
    let sleep_ms = 250;
    let attempts = (timeout_ms / sleep_ms).max(1);

    for _ in 0..attempts {
        let ready = match reqwest::Client::new()
            .get(&session_url)
            .header("x-opencode-directory", directory)
            .send()
            .await
        {
            Ok(resp) => match resp.text().await {
                Ok(text) => {
                    let trimmed = text.trim_start();
                    !trimmed.is_empty() && (trimmed.starts_with('[') || trimmed.starts_with('{'))
                }
                Err(err) => {
                    tracing::debug!(
                        "Session readiness body read failed endpoint={} directory={}: {}",
                        session_url,
                        directory,
                        err
                    );
                    false
                }
            },
            Err(err) => {
                tracing::debug!(
                    "Session readiness request failed endpoint={} directory={}: {}",
                    session_url,
                    directory,
                    err
                );
                false
            }
        };

        if ready {
            return true;
        }

        tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
    }

    false
}
