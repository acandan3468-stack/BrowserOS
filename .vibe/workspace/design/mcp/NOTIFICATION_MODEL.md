# browseros-mcp Notification Model

**Document:** NOTIFICATION_MODEL.md  
**Phase:** Design Freeze  
**Date:** 2026-07-16  

---

## 1. Architecture

```
EventBus (global)
    │
    ▼
NotificationDispatcher (per-server)
    │
    ├── Event → Notification mapper
    ├── Subscriber registry (RwLock<HashMap<SubscriberId, SubscriberInfo>>)
    │
    ├── For each subscriber:
    │     ├── Filter by session_id match
    │     ├── Filter by notification type mask
    │     ├── Apply throttle policy
    │     └── mpsc::Sender::try_send()
    │
    └── metrics: counter per notification type, dropped count
```

---

## 2. Notification Types and Payloads

### 2.1 Progress

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/progress",
  "params": {
    "session_id": "sess_01J3YF...",
    "execution_id": "exec_001",
    "tool": "browser/navigate",
    "progress": 0.0,
    "total": 1.0,
    "message": "Navigating to https://example.com"
  }
}
```

- **Ordering:** Per-execution_id, progress values are monotonically non-decreasing
- **Delivery:** Best-effort (lossy under backpressure)
- **Buffering:** 16 messages per subscriber channel
- **Backpressure:** Drop oldest when full (LowPriority throttle policy)
- **Cancellation:** When execution is cancelled, final progress has `"status": "cancelled"`
- **Interval throttle:** Max 10 notifications per second per execution_id

### 2.2 Browser Lifecycle

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/browser/created",
  "params": {
    "session_id": "sess_01J3YF...",
    "browser_type": "chromium",
    "version": "130.0",
    "pid": 12345,
    "created_at": "2026-07-16T12:00:00Z"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/browser/crashed",
  "params": {
    "session_id": "sess_01J3YF...",
    "reason": "out_of_memory",
    "exit_code": -6,
    "timestamp": "2026-07-16T12:00:00Z"
  }
}
```

- **Ordering:** Exactly one `browser/created` per session, at most one `browser/crashed` or `browser/closed`
- **Delivery:** Reliable (HighPriority throttle policy)
- **Buffering:** 64 messages
- **Backpressure:** Block until space available (max 100ms), then drop

### 2.3 Navigation

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/browser/navigated",
  "params": {
    "session_id": "sess_01J3YF...",
    "url": "https://example.com/page",
    "title": "Example Page",
    "status_code": 200,
    "navigation_id": "nav_001",
    "duration_ms": 1200,
    "timestamp": "2026-07-16T12:00:00Z"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/browser/navigation_failed",
  "params": {
    "session_id": "sess_01J3YF...",
    "url": "https://example.com/404",
    "status_code": 404,
    "error": "Not Found",
    "navigation_id": "nav_002",
    "timestamp": "2026-07-16T12:00:00Z"
  }
}
```

- **Ordering:** FIFO per navigation_id
- **Delivery:** Best-effort
- **Buffering:** 32 messages
- **Throttle:** Max 5 per second per session (rapid redirect chains)

### 2.4 Download

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/browser/download_started",
  "params": {
    "session_id": "sess_01J3YF...",
    "download_id": "dl_001",
    "url": "https://example.com/file.zip",
    "mime_type": "application/zip",
    "total_bytes": 1048576,
    "timestamp": "2026-07-16T12:00:00Z"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/browser/download_progress",
  "params": {
    "download_id": "dl_001",
    "received_bytes": 524288,
    "total_bytes": 1048576,
    "progress": 0.5
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/browser/download_completed",
  "params": {
    "download_id": "dl_001",
    "path": "/tmp/browseros/downloads/file.zip",
    "size_bytes": 1048576,
    "duration_ms": 3400
  }
}
```

- **Ordering:** Monotonic per download_id (started → 0+ progress → completed/failed)
- **Delivery:** Best-effort for progress, reliable for started/completed
- **Throttle:** Max 2 progress notifications per second per download

### 2.5 DOM Mutation

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/dom/mutation",
  "params": {
    "session_id": "sess_01J3YF...",
    "frame_id": "frame_001",
    "timestamp": "2026-07-16T12:00:00Z",
    "added": [
      { "element_id": "elem_010", "tag": "div", "parent_id": "elem_005" }
    ],
    "removed": [
      { "element_id": "elem_008", "tag": "span" }
    ],
    "attributes_changed": [
      { "element_id": "elem_003", "name": "class", "value": "active" }
    ],
    "character_data_changed": [
      { "element_id": "elem_007", "new_value": "Updated text" }
    ]
  }
}
```

- **Ordering:** FIFO per session_id
- **Delivery:** Lossy under backpressure
- **Buffering:** 128 messages (high frequency expected)
- **Backpressure:** Drop oldest, increment dropped counter
- **Throttle:** Batch mutations within 50ms windows → single notification
- **Opt-in:** Client must subscribe explicitly via `dom/observe` tool; not sent by default

### 2.6 Network

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/network/request",
  "params": {
    "session_id": "sess_01J3YF...",
    "request_id": "req_001",
    "url": "https://api.example.com/data",
    "method": "GET",
    "headers": { "accept": "application/json" },
    "resource_type": "fetch",
    "timestamp": "2026-07-16T12:00:00Z"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/network/response",
  "params": {
    "session_id": "sess_01J3YF...",
    "request_id": "req_001",
    "url": "https://api.example.com/data",
    "status_code": 200,
    "headers": { "content-type": "application/json" },
    "size_bytes": 2048,
    "duration_ms": 340,
    "timestamp": "2026-07-16T12:00:00Z"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/network/request_failed",
  "params": {
    "request_id": "req_002",
    "url": "https://api.example.com/timeout",
    "error": "connection_timeout",
    "timestamp": "2026-07-16T12:00:00Z"
  }
}
```

- **Ordering:** Per request_id: request → 0+ response_headers → 0+ response_body → completed/failed
- **Delivery:** Best-effort
- **Buffering:** 256 messages (high frequency)
- **Throttle:** Max 50 per second per session, batch within 10ms windows
- **Opt-in:** Client must enable via `network/intercept` or explicit subscribe

### 2.7 Workflow

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/workflow/step_completed",
  "params": {
    "session_id": "sess_01J3YF...",
    "execution_id": "wf_001",
    "step_id": "3",
    "tool": "dom/click",
    "status": "success",
    "duration_ms": 450,
    "result_summary": "Clicked #submit-btn"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/workflow/completed",
  "params": {
    "session_id": "sess_01J3YF...",
    "execution_id": "wf_001",
    "status": "success",
    "total_steps": 5,
    "completed_steps": 5,
    "failed_steps": 0,
    "total_duration_ms": 12300
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/workflow/failed",
  "params": {
    "session_id": "sess_01J3YF...",
    "execution_id": "wf_001",
    "failed_step": "4",
    "error": "Element not found: #submit-btn",
    "completed_steps": 3,
    "total_duration_ms": 8900
  }
}
```

- **Ordering:** Sequential per execution_id (step_completed 1 → 2 → ... → completed/failed)
- **Delivery:** Reliable (HighPriority)
- **Buffering:** 64 messages
- **Backpressure:** Block (max 1s) then cache to session state and flush on reconnect

### 2.8 Session

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/session/created",
  "params": {
    "session_id": "sess_01J3YF...",
    "browser_type": "chromium",
    "created_at": "2026-07-16T12:00:00Z"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/session/closed",
  "params": {
    "session_id": "sess_01J3YF...",
    "reason": "client_request",
    "duration_secs": 600,
    "tool_calls": 42,
    "timestamp": "2026-07-16T12:10:00Z"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/session/expired",
  "params": {
    "session_id": "sess_01J3YF...",
    "reason": "idle_timeout",
    "idle_seconds": 310,
    "timestamp": "2026-07-16T12:15:00Z"
  }
}
```

- **Ordering:** Exactly one `created` per session, at most one `closed` or `expired`
- **Delivery:** Reliable (HighPriority)

### 2.9 Tool Execution

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/tool/started",
  "params": {
    "session_id": "sess_01J3YF...",
    "request_id": "req_010",
    "tool": "browser/navigate",
    "params_summary": "{url: https://...}",
    "started_at": "2026-07-16T12:00:00Z"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/tool/completed",
  "params": {
    "request_id": "req_010",
    "tool": "browser/navigate",
    "duration_ms": 1200,
    "status": "success",
    "result_summary": "Navigated to example.com"
  }
}
```

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/tool/cancelled",
  "params": {
    "request_id": "req_010",
    "tool": "browser/navigate",
    "duration_ms": 800,
    "reason": "client_request"
  }
}
```

- **Ordering:** Exactly one `started`, at most one `completed` or `cancelled` or `failed` per request_id
- **Delivery:** Best-effort (informational)
- **Buffering:** 64 messages

### 2.10 Telemetry

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/server/metrics",
  "params": {
    "timestamp": "2026-07-16T12:00:00Z",
    "interval_secs": 60,
    "metrics": {
      "active_sessions": 3,
      "tools_called": 150,
      "tools_failed": 2,
      "avg_latency_ms": 45,
      "p99_latency_ms": 320,
      "notifications_sent": 1200,
      "notifications_dropped": 5,
      "memory_mb": 156
    }
  }
}
```

- **Cadence:** Every 60 seconds
- **Delivery:** Best-effort
- **Buffering:** 4 messages (stale data, no point buffering many)
- **Throttle:** Fixed interval, no per-client adjustment

### 2.11 Diagnostics

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/diagnostics/log",
  "params": {
    "session_id": "sess_01J3YF...",
    "level": "warn",
    "target": "browseros-mcp::tools::browser",
    "message": "Navigation timeout after 30s",
    "timestamp": "2026-07-16T12:00:00Z"
  }
}
```

- **Only in debug mode** (--debug flag on server startup)
- **Delivery:** Reliable in debug mode
- **Throttle:** Max 100 per second (ring buffer with oldest drop)
- **Buffering:** 256 messages

---

## 3. Delivery Guarantees Summary

| Notification Type | Ordering | Delivery | Backpressure | Throttle |
|-------------------|----------|----------|--------------|----------|
| progress | Monotonic | Best-effort | Drop oldest | 10/s per exec |
| browser/created | Exactly-once | Reliable | Block 100ms | None |
| browser/crashed | At-most-once | Reliable | Block 100ms | None |
| browser/navigated | FIFO | Best-effort | Drop oldest | 5/s per session |
| download/* | Monotonic per ID | Best-effort / Reliable | Drop oldest | 2/s per download |
| dom/mutation | FIFO per session | Lossy | Drop oldest | Batch 50ms |
| network/* | FIFO per request | Best-effort | Drop oldest | 50/s, batch 10ms |
| workflow/* | Sequential | Reliable | Block 1s → cache | None |
| session/* | Exactly-once | Reliable | Block 100ms | None |
| tool/* | Per-request | Best-effort | Drop oldest | None |
| server/metrics | N/A (timer) | Best-effort | Drop oldest | Fixed 60s |
| diagnostics/* | FIFO | Reliable (debug) | Ring buffer | 100/s |

---

## 4. Subscriber Management

```rust
pub struct SubscriberInfo {
    id: SubscriberId,
    session_id_filter: Option<SessionId>,
    notification_types: NotificationMask,
    sender: mpsc::Sender<McpNotification>,
    created_at: chrono::DateTime<chrono::Utc>,
}
```

- **SubscriberId:** UUID v7, generated per MCP client connection
- **SessionIdFilter:** Subscribe to notifications for a specific session only
- **NotificationMask:** Bitmask of interested notification types
- **Sender:** Bounded mpsc channel

### 4.1 Default Subscriptions

When an MCP client initializes and creates a session, it automatically receives:
- `session/*` notifications for its own sessions
- `browser/*` notifications for its own sessions
- `workflow/*` notifications for its own workflows
- `progress` notifications for its own tool calls

All other notifications require explicit opt-in (e.g., subscribing via `dom/observe`).

---

*This document defines the complete Notification Model. It contains no Rust code, no Cargo.toml, and no placeholders.*
