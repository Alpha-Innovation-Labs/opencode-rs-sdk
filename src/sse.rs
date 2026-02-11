//! SSE (Server-Sent Events) streaming support.
//!
//! This module provides SSE subscription with reconnection and backoff.

use crate::error::Result;
use crate::types::event::Event;
use backon::{BackoffBuilder, ExponentialBuilder};
use futures::StreamExt;
use reqwest::Client as ReqClient;
use reqwest_eventsource::{Event as EsEvent, EventSource};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{RwLock, broadcast, mpsc};
use tokio_util::sync::CancellationToken;

fn extract_session_id_from_raw_event(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let event_type = value.get("type")?.as_str()?;
    let properties = value.get("properties")?;

    match event_type {
        // JS SDK routes token stream events via properties.part.sessionID
        "message.part.updated" => properties
            .get("part")
            .and_then(|p| p.get("sessionID").or_else(|| p.get("sessionId")))
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned),
        "session.idle" | "session.error" => properties
            .get("sessionID")
            .or_else(|| properties.get("sessionId"))
            .and_then(|v| v.as_str())
            .map(ToOwned::to_owned),
        _ => None,
    }
}

fn should_forward_event(session_filter: Option<&str>, raw: &str, ev: &Event) -> bool {
    match session_filter {
        None => true,
        Some(expected_session_id) => extract_session_id_for_routing(raw, ev)
            .map(|actual_session_id| actual_session_id == expected_session_id)
            .unwrap_or(false),
    }
}

fn extract_session_id_for_routing(raw: &str, ev: &Event) -> Option<String> {
    if matches!(
        ev,
        Event::MessagePartUpdated { .. } | Event::SessionIdle { .. } | Event::SessionError { .. }
    ) {
        return extract_session_id_from_raw_event(raw);
    }

    ev.session_id().map(ToOwned::to_owned)
}

/// Options for SSE subscription.
#[derive(Clone, Copy, Debug)]
pub struct SseOptions {
    /// Channel capacity (default: 256).
    pub capacity: usize,
    /// Initial backoff interval (default: 250ms).
    pub initial_interval: Duration,
    /// Max backoff interval (default: 30s).
    pub max_interval: Duration,
}

impl Default for SseOptions {
    fn default() -> Self {
        Self {
            capacity: 256,
            initial_interval: Duration::from_millis(250),
            max_interval: Duration::from_secs(30),
        }
    }
}

/// Snapshot of SSE stream diagnostics counters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SseStreamStats {
    /// Number of SSE message frames received from the server.
    pub events_in: u64,
    /// Number of events successfully emitted to the subscription receiver.
    pub events_out: u64,
    /// Number of events dropped before delivery.
    pub dropped: u64,
    /// Number of JSON parse errors while decoding typed SSE events.
    pub parse_errors: u64,
    /// Number of reconnect attempts after stream interruption.
    pub reconnects: u64,
    /// Last observed `Last-Event-ID` value, if any.
    pub last_event_id: Option<String>,
}

#[derive(Debug, Default)]
struct SharedSseStreamStats {
    events_in: AtomicU64,
    events_out: AtomicU64,
    dropped: AtomicU64,
    parse_errors: AtomicU64,
    reconnects: AtomicU64,
    last_event_id: StdRwLock<Option<String>>,
}

impl SharedSseStreamStats {
    fn snapshot(&self) -> SseStreamStats {
        SseStreamStats {
            events_in: self.events_in.load(Ordering::Relaxed),
            events_out: self.events_out.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            parse_errors: self.parse_errors.load(Ordering::Relaxed),
            reconnects: self.reconnects.load(Ordering::Relaxed),
            last_event_id: self
                .last_event_id
                .read()
                .ok()
                .and_then(|value| value.clone()),
        }
    }

    fn set_last_event_id(&self, id: Option<String>) {
        if let Ok(mut guard) = self.last_event_id.write() {
            *guard = id;
        }
    }
}

/// Handle to an active SSE subscription.
///
/// Dropping this handle will cancel the subscription.
pub struct SseSubscription {
    rx: mpsc::Receiver<Event>,
    stats: Arc<SharedSseStreamStats>,
    cancel: CancellationToken,
    _task: tokio::task::JoinHandle<()>,
}

/// Raw SSE message frame as delivered by the `/event` endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSseEvent {
    /// Last-Event-ID value from the server, if any.
    pub id: String,
    /// SSE event name from the server frame.
    pub event: String,
    /// Raw JSON payload in the `data:` frame.
    pub data: String,
}

/// Handle to an active raw SSE subscription.
///
/// Dropping this handle will cancel the subscription.
pub struct RawSseSubscription {
    rx: mpsc::Receiver<RawSseEvent>,
    stats: Arc<SharedSseStreamStats>,
    cancel: CancellationToken,
    _task: tokio::task::JoinHandle<()>,
}

/// Options for [`SessionEventRouter`].
#[derive(Clone, Copy, Debug)]
pub struct SessionEventRouterOptions {
    /// Upstream `/event` raw stream subscription options.
    pub upstream: SseOptions,
    /// Per-session fan-out channel capacity (default: 256).
    pub session_capacity: usize,
    /// Per-subscriber output channel capacity (default: 256).
    pub subscriber_capacity: usize,
}

impl Default for SessionEventRouterOptions {
    fn default() -> Self {
        Self {
            upstream: SseOptions::default(),
            session_capacity: 256,
            subscriber_capacity: 256,
        }
    }
}

#[derive(Debug)]
struct SessionEventRouterInner {
    per_session_channels: Arc<RwLock<HashMap<String, broadcast::Sender<Event>>>>,
    session_capacity: usize,
    subscriber_capacity: usize,
    upstream_stats: Arc<SharedSseStreamStats>,
    cancel: CancellationToken,
    _task: tokio::task::JoinHandle<()>,
}

/// Multiplexes one upstream `/event` stream into per-session subscriptions.
#[derive(Clone, Debug)]
pub struct SessionEventRouter {
    inner: Arc<SessionEventRouterInner>,
}

impl SessionEventRouter {
    /// Subscribe to typed events for a single session ID.
    pub async fn subscribe(&self, session_id: &str) -> SseSubscription {
        let sender = {
            let mut channels = self.inner.per_session_channels.write().await;
            channels
                .entry(session_id.to_string())
                .or_insert_with(|| {
                    let (tx, _rx) = broadcast::channel(self.inner.session_capacity);
                    tx
                })
                .clone()
        };

        let mut session_rx = sender.subscribe();
        let (tx, rx) = mpsc::channel(self.inner.subscriber_capacity);
        let stats = Arc::new(SharedSseStreamStats::default());
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let stats_task = Arc::clone(&stats);

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = cancel_clone.cancelled() => {
                        return;
                    }
                    recv = session_rx.recv() => {
                        match recv {
                            Ok(ev) => {
                                stats_task.events_in.fetch_add(1, Ordering::Relaxed);
                                if tx.send(ev).await.is_err() {
                                    stats_task.dropped.fetch_add(1, Ordering::Relaxed);
                                    return;
                                }
                                stats_task.events_out.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                                stats_task.dropped.fetch_add(skipped, Ordering::Relaxed);
                                tracing::warn!(
                                    "SessionEventRouter subscription lagged by {} event(s)",
                                    skipped
                                );
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                return;
                            }
                        }
                    }
                }
            }
        });

        SseSubscription {
            rx,
            stats,
            cancel,
            _task: task,
        }
    }

    /// Get diagnostics for the upstream `/event` stream used by this router.
    pub fn stats(&self) -> SseStreamStats {
        self.inner.upstream_stats.snapshot()
    }

    /// Stop the router and all upstream activity.
    pub fn close(&self) {
        self.inner.cancel.cancel();
    }
}

impl Drop for SessionEventRouter {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            self.inner.cancel.cancel();
        }
    }
}

impl RawSseSubscription {
    /// Receive the next raw SSE message.
    ///
    /// Returns `None` if the stream is closed.
    pub async fn recv(&mut self) -> Option<RawSseEvent> {
        self.rx.recv().await
    }

    /// Get a snapshot of stream diagnostics.
    pub fn stats(&self) -> SseStreamStats {
        self.stats.snapshot()
    }

    /// Close the subscription explicitly.
    pub fn close(&self) {
        self.cancel.cancel();
    }
}

impl Drop for RawSseSubscription {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl SseSubscription {
    /// Receive the next event.
    ///
    /// Returns `None` if the stream is closed.
    pub async fn recv(&mut self) -> Option<Event> {
        self.rx.recv().await
    }

    /// Get a snapshot of stream diagnostics.
    pub fn stats(&self) -> SseStreamStats {
        self.stats.snapshot()
    }

    /// Close the subscription explicitly.
    pub fn close(&self) {
        self.cancel.cancel();
    }
}

impl Drop for SseSubscription {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// SSE subscriber for OpenCode events.
#[derive(Clone)]
pub struct SseSubscriber {
    http: ReqClient,
    base_url: String,
    directory: Option<String>,
    last_event_id: Arc<RwLock<Option<String>>>,
}

impl SseSubscriber {
    // TODO(3): Accept optional ReqClient to allow connection pool sharing with HttpClient

    /// Create a new SSE subscriber.
    pub fn new(
        base_url: String,
        directory: Option<String>,
        last_event_id: Arc<RwLock<Option<String>>>,
    ) -> Self {
        Self {
            http: ReqClient::new(),
            base_url,
            directory,
            last_event_id,
        }
    }

    /// Subscribe to events, optionally filtered by session ID.
    ///
    /// OpenCode's `/event` endpoint streams all events for the configured directory.
    /// If `session_id` is provided, events will be filtered client-side to only
    /// include events for that session.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_session(
        &self,
        session_id: &str,
        opts: SseOptions,
    ) -> Result<SseSubscription> {
        let url = format!("{}/event", self.base_url);
        self.subscribe_filtered(url, Some(session_id.to_string()), opts)
            .await
    }

    /// Subscribe to all events for the configured directory.
    ///
    /// This uses the `/event` endpoint which streams all events for the
    /// directory specified via the `x-opencode-directory` header.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe(&self, opts: SseOptions) -> Result<SseSubscription> {
        self.subscribe_typed(opts).await
    }

    /// Subscribe to all events for the configured directory as typed [`Event`] values.
    ///
    /// This is equivalent to [`Self::subscribe`], but explicitly named to distinguish
    /// it from [`Self::subscribe_raw`].
    pub async fn subscribe_typed(&self, opts: SseOptions) -> Result<SseSubscription> {
        let url = format!("{}/event", self.base_url);
        self.subscribe_filtered(url, None, opts).await
    }

    /// Subscribe to global events (all directories).
    ///
    /// This uses the `/global/event` endpoint which streams events from all
    /// OpenCode instances across all directories. Events are wrapped in a
    /// `GlobalEventEnvelope` with directory context.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_global(&self, opts: SseOptions) -> Result<SseSubscription> {
        self.subscribe_typed_global(opts).await
    }

    /// Subscribe to global events as typed [`Event`] values (all directories).
    pub async fn subscribe_typed_global(&self, opts: SseOptions) -> Result<SseSubscription> {
        let url = format!("{}/global/event", self.base_url);
        self.subscribe_filtered(url, None, opts).await
    }

    /// Subscribe to raw JSON SSE frames from the configured directory's `/event` stream.
    ///
    /// This is intended for debugging and parity verification.
    pub async fn subscribe_raw(&self, opts: SseOptions) -> Result<RawSseSubscription> {
        let url = format!("{}/event", self.base_url);
        self.subscribe_raw_inner(url, opts).await
    }

    /// Create a session event router with one upstream `/event` subscription.
    pub async fn session_event_router(
        &self,
        opts: SessionEventRouterOptions,
    ) -> Result<SessionEventRouter> {
        let mut upstream = self.subscribe_raw(opts.upstream).await?;
        let upstream_stats = Arc::clone(&upstream.stats);
        let endpoint = format!("{}/event", self.base_url);
        let directory = self.directory.clone();
        let channels = Arc::new(RwLock::new(
            HashMap::<String, broadcast::Sender<Event>>::new(),
        ));
        let channels_task = channels.clone();
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = cancel_clone.cancelled() => {
                        upstream.close();
                        return;
                    }
                    maybe_raw = upstream.recv() => {
                        let Some(raw) = maybe_raw else {
                            return;
                        };

                        let event = match serde_json::from_str::<Event>(&raw.data) {
                            Ok(ev) => ev,
                            Err(e) => {
                                tracing::warn!(
                                    "SessionEventRouter failed to parse raw event endpoint={} directory={:?} last_event_id={}: {} - Raw data: {}",
                                    endpoint,
                                    directory,
                                    raw.id,
                                    e,
                                    raw.data
                                );
                                continue;
                            }
                        };

                        let Some(session_id) = extract_session_id_for_routing(&raw.data, &event) else {
                            continue;
                        };

                        let sender = channels_task.read().await.get(&session_id).cloned();
                        if let Some(sender) = sender {
                            if sender.receiver_count() == 0 {
                                channels_task.write().await.remove(&session_id);
                                continue;
                            }

                            let _ = sender.send(event);
                        }
                    }
                }
            }
        });

        Ok(SessionEventRouter {
            inner: Arc::new(SessionEventRouterInner {
                per_session_channels: channels,
                session_capacity: opts.session_capacity,
                subscriber_capacity: opts.subscriber_capacity,
                upstream_stats,
                cancel,
                _task: task,
            }),
        })
    }

    async fn subscribe_filtered(
        &self,
        url: String,
        session_filter: Option<String>,
        opts: SseOptions,
    ) -> Result<SseSubscription> {
        let (tx, rx) = mpsc::channel(opts.capacity);
        let stats = Arc::new(SharedSseStreamStats::default());
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let http = self.http.clone();
        let dir = self.directory.clone();
        let lei = self.last_event_id.clone();
        let initial = opts.initial_interval;
        let max = opts.max_interval;
        let endpoint = url.clone();
        let stats_task = Arc::clone(&stats);

        stats.set_last_event_id(lei.read().await.clone());
        let filter = session_filter;

        let task = tokio::spawn(async move {
            // Note: No max_times means the subscriber will retry indefinitely.
            // This is intentional for long-lived SSE connections that should reconnect
            // on any transient network failure.
            let backoff_builder = ExponentialBuilder::default()
                .with_min_delay(initial)
                .with_max_delay(max)
                .with_factor(2.0)
                .with_jitter();

            let mut backoff = backoff_builder.build();

            loop {
                if cancel_clone.is_cancelled() {
                    break;
                }

                let mut req = http.get(&url);
                if let Some(d) = &dir {
                    req = req.header("x-opencode-directory", d);
                }
                if let Some(id) = lei.read().await.clone() {
                    req = req.header("Last-Event-ID", id);
                }

                let es_result = EventSource::new(req);
                let mut es = match es_result {
                    Ok(es) => es,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create EventSource endpoint={} directory={:?} session_filter={:?}: {:?}",
                            endpoint,
                            dir,
                            filter,
                            e
                        );
                        if let Some(delay) = backoff.next() {
                            stats_task.reconnects.fetch_add(1, Ordering::Relaxed);
                            tokio::select! {
                                () = tokio::time::sleep(delay) => {}
                                () = cancel_clone.cancelled() => { return; }
                            }
                        }
                        continue;
                    }
                };

                while let Some(event) = es.next().await {
                    if cancel_clone.is_cancelled() {
                        es.close();
                        return;
                    }

                    match event {
                        Ok(EsEvent::Open) => {
                            // Reset backoff on successful connection
                            backoff = backoff_builder.build();
                            tracing::debug!(
                                "SSE connection opened endpoint={} directory={:?} session_filter={:?}",
                                endpoint,
                                dir,
                                filter
                            );
                        }
                        Ok(EsEvent::Message(msg)) => {
                            stats_task.events_in.fetch_add(1, Ordering::Relaxed);
                            // Track last event ID
                            if !msg.id.is_empty() {
                                *lei.write().await = Some(msg.id.clone());
                                stats_task.set_last_event_id(Some(msg.id.clone()));
                            }

                            // Parse event
                            match serde_json::from_str::<Event>(&msg.data) {
                                Ok(ev) => {
                                    tracing::debug!(
                                        "Parsed SSE event endpoint={} directory={:?} session_filter={:?}: {:?}",
                                        endpoint,
                                        dir,
                                        filter,
                                        ev
                                    );
                                    // Apply session filter if specified
                                    let should_send =
                                        should_forward_event(filter.as_deref(), &msg.data, &ev);

                                    if should_send {
                                        if tx.send(ev).await.is_err() {
                                            stats_task.dropped.fetch_add(1, Ordering::Relaxed);
                                            es.close();
                                            return;
                                        }
                                        stats_task.events_out.fetch_add(1, Ordering::Relaxed);
                                    } else {
                                        stats_task.dropped.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                                Err(e) => {
                                    stats_task.parse_errors.fetch_add(1, Ordering::Relaxed);
                                    stats_task.dropped.fetch_add(1, Ordering::Relaxed);
                                    tracing::warn!(
                                        "Failed to parse SSE event endpoint={} directory={:?} session_filter={:?}: {} - Raw data: {}",
                                        endpoint,
                                        dir,
                                        filter,
                                        e,
                                        msg.data
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "SSE error endpoint={} directory={:?} session_filter={:?}: {:?}",
                                endpoint,
                                dir,
                                filter,
                                e
                            );
                            es.close();
                            break; // Break inner loop to reconnect
                        }
                    }
                }

                // Apply backoff before reconnecting
                if let Some(delay) = backoff.next() {
                    stats_task.reconnects.fetch_add(1, Ordering::Relaxed);
                    tracing::debug!(
                        "SSE reconnecting endpoint={} directory={:?} session_filter={:?} after {:?}",
                        endpoint,
                        dir,
                        filter,
                        delay
                    );
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        () = cancel_clone.cancelled() => { return; }
                    }
                }
            }
        });

        Ok(SseSubscription {
            rx,
            stats,
            cancel,
            _task: task,
        })
    }

    async fn subscribe_raw_inner(
        &self,
        url: String,
        opts: SseOptions,
    ) -> Result<RawSseSubscription> {
        let (tx, rx) = mpsc::channel(opts.capacity);
        let stats = Arc::new(SharedSseStreamStats::default());
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        let http = self.http.clone();
        let dir = self.directory.clone();
        let lei = self.last_event_id.clone();
        let initial = opts.initial_interval;
        let max = opts.max_interval;
        let endpoint = url.clone();
        let stats_task = Arc::clone(&stats);

        stats.set_last_event_id(lei.read().await.clone());

        let task = tokio::spawn(async move {
            let backoff_builder = ExponentialBuilder::default()
                .with_min_delay(initial)
                .with_max_delay(max)
                .with_factor(2.0)
                .with_jitter();

            let mut backoff = backoff_builder.build();

            loop {
                if cancel_clone.is_cancelled() {
                    break;
                }

                let mut req = http.get(&url);
                if let Some(d) = &dir {
                    req = req.header("x-opencode-directory", d);
                }
                if let Some(id) = lei.read().await.clone() {
                    req = req.header("Last-Event-ID", id);
                }

                let es_result = EventSource::new(req);
                let mut es = match es_result {
                    Ok(es) => es,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create raw EventSource endpoint={} directory={:?}: {:?}",
                            endpoint,
                            dir,
                            e
                        );
                        if let Some(delay) = backoff.next() {
                            stats_task.reconnects.fetch_add(1, Ordering::Relaxed);
                            tokio::select! {
                                () = tokio::time::sleep(delay) => {}
                                () = cancel_clone.cancelled() => { return; }
                            }
                        }
                        continue;
                    }
                };

                while let Some(event) = es.next().await {
                    if cancel_clone.is_cancelled() {
                        es.close();
                        return;
                    }

                    match event {
                        Ok(EsEvent::Open) => {
                            backoff = backoff_builder.build();
                            tracing::debug!(
                                "SSE raw connection opened endpoint={} directory={:?}",
                                endpoint,
                                dir
                            );
                        }
                        Ok(EsEvent::Message(msg)) => {
                            stats_task.events_in.fetch_add(1, Ordering::Relaxed);
                            if !msg.id.is_empty() {
                                *lei.write().await = Some(msg.id.clone());
                                stats_task.set_last_event_id(Some(msg.id.clone()));
                            }

                            let raw = RawSseEvent {
                                id: msg.id,
                                event: msg.event,
                                data: msg.data,
                            };

                            if tx.send(raw).await.is_err() {
                                stats_task.dropped.fetch_add(1, Ordering::Relaxed);
                                es.close();
                                return;
                            }
                            stats_task.events_out.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "SSE raw error endpoint={} directory={:?}: {:?}",
                                endpoint,
                                dir,
                                e
                            );
                            es.close();
                            break;
                        }
                    }
                }

                if let Some(delay) = backoff.next() {
                    stats_task.reconnects.fetch_add(1, Ordering::Relaxed);
                    tracing::debug!(
                        "SSE raw reconnecting endpoint={} directory={:?} after {:?}",
                        endpoint,
                        dir,
                        delay
                    );
                    tokio::select! {
                        () = tokio::time::sleep(delay) => {}
                        () = cancel_clone.cancelled() => { return; }
                    }
                }
            }
        });

        Ok(RawSseSubscription {
            rx,
            stats,
            cancel,
            _task: task,
        })
    }
}

#[cfg(test)]
mod tests {
    // TODO(2): Add tests for session filtering logic (lines 216-219), Last-Event-ID
    // tracking/resume behavior (lines 208-210, 176-178), and backoff timing (with
    // tokio time mocking).
    use super::*;

    #[test]
    fn test_sse_options_defaults() {
        let opts = SseOptions::default();
        assert_eq!(opts.capacity, 256);
        assert_eq!(opts.initial_interval, Duration::from_millis(250));
        assert_eq!(opts.max_interval, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_subscription_cancel_on_close() {
        let subscriber = SseSubscriber::new(
            "http://localhost:9999".to_string(),
            None,
            Arc::new(RwLock::new(None)),
        );

        // This will fail to connect but we can test cancellation
        let opts = SseOptions {
            capacity: 1,
            initial_interval: Duration::from_millis(10),
            max_interval: Duration::from_millis(50),
        };

        let subscription = subscriber.subscribe_global(opts).await.unwrap();
        assert_eq!(subscription.stats().events_in, 0);
        subscription.close();
        // Subscription should be cancelled
        assert!(subscription.cancel.is_cancelled());
    }

    #[test]
    fn test_extract_session_id_from_raw_event_accepts_session_id_variants() {
        let message_part_with_pascal =
            r#"{"type":"message.part.updated","properties":{"part":{"sessionID":"sess-a"}}}"#;
        assert_eq!(
            extract_session_id_from_raw_event(message_part_with_pascal),
            Some("sess-a".to_string())
        );

        let message_part_with_camel =
            r#"{"type":"message.part.updated","properties":{"part":{"sessionId":"sess-b"}}}"#;
        assert_eq!(
            extract_session_id_from_raw_event(message_part_with_camel),
            Some("sess-b".to_string())
        );

        let session_idle_with_camel =
            r#"{"type":"session.idle","properties":{"sessionId":"sess-c"}}"#;
        assert_eq!(
            extract_session_id_from_raw_event(session_idle_with_camel),
            Some("sess-c".to_string())
        );
    }

    #[test]
    fn test_should_forward_event_drops_events_without_session_id_when_filtered() {
        let unknown_json = r#"{"type":"server.connected","properties":{}}"#;
        let event: Event = serde_json::from_str(unknown_json).unwrap();

        assert!(should_forward_event(None, unknown_json, &event));
        assert!(!should_forward_event(
            Some("sess-123"),
            unknown_json,
            &event
        ));
    }

    #[test]
    fn test_should_forward_event_for_message_part_uses_raw_js_parity_fields() {
        // sessionId is present only at the top-level properties object.
        // For message.part.updated filtering we intentionally use raw parity
        // with JS (`properties.part.sessionID|sessionId`) and drop when missing.
        let json =
            r#"{"type":"message.part.updated","properties":{"sessionId":"sess-top","delta":"hi"}}"#;
        let event: Event = serde_json::from_str(json).unwrap();

        assert!(!should_forward_event(Some("sess-top"), json, &event));
    }

    #[test]
    fn test_extract_session_id_for_routing_prefers_raw_parity_fields() {
        let json = r#"{"type":"message.part.updated","properties":{"part":{"type":"text","text":"","sessionID":"sess-nested"},"sessionId":"sess-top"}}"#;
        let event: Event = serde_json::from_str(json).unwrap();

        assert_eq!(
            extract_session_id_for_routing(json, &event),
            Some("sess-nested".to_string())
        );
    }

    #[test]
    fn test_extract_session_id_for_routing_falls_back_to_typed_fields() {
        let json = r#"{"type":"message.updated","properties":{"info":{"id":"m1","sessionId":"sess-typed","role":"assistant","time":{"created":1}}}}"#;
        let event: Event = serde_json::from_str(json).unwrap();

        assert_eq!(
            extract_session_id_for_routing(json, &event),
            Some("sess-typed".to_string())
        );
    }

    #[test]
    fn test_concurrent_session_filtering_no_delta_cross_contamination() {
        let raw_events = [
            r#"{"type":"message.part.updated","properties":{"part":{"type":"text","text":"","sessionID":"sess-a"},"delta":"alpha"}}"#,
            r#"{"type":"message.part.updated","properties":{"part":{"type":"text","text":"","sessionID":"sess-b"},"delta":"bravo"}}"#,
            r#"{"type":"server.heartbeat","properties":{}}"#,
            r#"{"type":"message.part.updated","properties":{"part":{"type":"text","text":"","sessionID":"sess-a"},"delta":"-2"}}"#,
            r#"{"type":"message.part.updated","properties":{"part":{"type":"text","text":"","sessionID":"sess-b"},"delta":"-2"}}"#,
        ];

        let mut a = String::new();
        let mut b = String::new();

        for raw in raw_events {
            let ev: Event = serde_json::from_str(raw).unwrap();

            if should_forward_event(Some("sess-a"), raw, &ev)
                && let Event::MessagePartUpdated { properties } = &ev
                && let Some(delta) = &properties.delta
            {
                a.push_str(delta);
            }

            if should_forward_event(Some("sess-b"), raw, &ev)
                && let Event::MessagePartUpdated { properties } = &ev
                && let Some(delta) = &properties.delta
            {
                b.push_str(delta);
            }
        }

        assert_eq!(a, "alpha-2");
        assert_eq!(b, "bravo-2");
    }

    #[tokio::test]
    async fn test_subscribe_raw_yields_payloads() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);

            let body = concat!(
                "id: 1\n",
                "event: message\n",
                "data: {\"type\":\"server.connected\",\"properties\":{}}\n",
                "\n",
                "id: 2\n",
                "event: message\n",
                "data: {\"type\":\"server.heartbeat\",\"properties\":{}}\n",
                "\n"
            );

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );

            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        });

        let subscriber = SseSubscriber::new(
            format!("http://{}", addr),
            None,
            Arc::new(RwLock::new(None)),
        );

        let mut sub = subscriber
            .subscribe_raw(SseOptions {
                capacity: 8,
                initial_interval: Duration::from_millis(10),
                max_interval: Duration::from_millis(20),
            })
            .await
            .unwrap();

        let first = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first.id, "1");
        assert_eq!(first.event, "message");
        assert!(first.data.contains("server.connected"));

        let second = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(second.id, "2");
        assert!(second.data.contains("server.heartbeat"));

        let stats = sub.stats();
        assert_eq!(stats.events_in, 2);
        assert_eq!(stats.events_out, 2);
        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.parse_errors, 0);
        assert_eq!(stats.last_event_id.as_deref(), Some("2"));

        sub.close();
        let _ = server.join();
    }

    #[tokio::test]
    async fn test_subscribe_typed_tracks_parse_errors_and_drops() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);

            let body = concat!(
                "id: 1\n",
                "event: message\n",
                "data: {\"type\":\"server.connected\",\"properties\":{}}\n",
                "\n",
                "id: 2\n",
                "event: message\n",
                "data: not-json\n",
                "\n",
                "id: 3\n",
                "event: message\n",
                "data: {\"type\":\"server.heartbeat\",\"properties\":{}}\n",
                "\n"
            );

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );

            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        });

        let subscriber = SseSubscriber::new(
            format!("http://{}", addr),
            None,
            Arc::new(RwLock::new(None)),
        );

        let mut sub = subscriber
            .subscribe_typed(SseOptions {
                capacity: 8,
                initial_interval: Duration::from_millis(10),
                max_interval: Duration::from_millis(20),
            })
            .await
            .unwrap();

        let first = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(first, Event::ServerConnected { .. }));

        let second = tokio::time::timeout(Duration::from_secs(2), sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(second, Event::ServerHeartbeat { .. }));

        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = sub.stats();
        assert_eq!(stats.events_in, 3);
        assert_eq!(stats.events_out, 2);
        assert_eq!(stats.dropped, 1);
        assert_eq!(stats.parse_errors, 1);
        assert_eq!(stats.last_event_id.as_deref(), Some("3"));

        sub.close();
        let _ = server.join();
    }

    #[tokio::test]
    async fn test_session_event_router_exposes_upstream_stats() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();

            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf);

            let body = concat!(
                "id: 9\n",
                "event: message\n",
                "data: {\"type\":\"message.removed\",\"properties\":{\"sessionId\":\"sess-a\",\"messageId\":\"msg-1\"}}\n",
                "\n"
            );

            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );

            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        });

        let subscriber = SseSubscriber::new(
            format!("http://{}", addr),
            None,
            Arc::new(RwLock::new(None)),
        );

        let router = subscriber
            .session_event_router(SessionEventRouterOptions {
                upstream: SseOptions {
                    capacity: 8,
                    initial_interval: Duration::from_millis(10),
                    max_interval: Duration::from_millis(20),
                },
                session_capacity: 8,
                subscriber_capacity: 8,
            })
            .await
            .unwrap();

        let mut session_sub = router.subscribe("sess-a").await;
        let event = tokio::time::timeout(Duration::from_secs(2), session_sub.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, Event::MessageRemoved { .. }));

        tokio::time::sleep(Duration::from_millis(50)).await;

        let stats = router.stats();
        assert_eq!(stats.events_in, 1);
        assert_eq!(stats.events_out, 1);
        assert_eq!(stats.dropped, 0);
        assert_eq!(stats.parse_errors, 0);
        assert_eq!(stats.last_event_id.as_deref(), Some("9"));

        session_sub.close();
        router.close();
        let _ = server.join();
    }
}
