# LLM Streaming Support

**Phase:** 2.1 — Design Freeze  
**Status:** Draft  

---

## 1. The Streaming Challenge

BrowserOS is synchronous. The DAG executor (`browseros-dag`) runs synchronously — there is no async runtime, no tokio, no async/await. LLM streaming (SSE, chunked HTTP) is inherently asynchronous.

### The Bidi Problem

```
Planner calls Gateway.chat_stream()
  │
  ├─ Gateway returns LlStreamHandle immediately
  ├─ Planner receives handle
  ├─ Planner MUST block until stream completes
  │   └─ If planner blocks, there's no benefit
  │
  └─ Conclusion: True streaming doesn't benefit the synchronous Planner
```

### Solution: Two-Level Architecture

```rust
// Level 1: Gateway produces LlStreamHandle (sync-compatible)
pub struct LlStreamHandle {
    receiver: crossbeam_channel::Receiver<LlStreamEvent>,
}

impl LlStreamHandle {
    // Blocking receive — compatible with sync executor
    pub fn recv(&self) -> Result<LlStreamEvent, LlmError>;

    // Non-blocking try_receive
    pub fn try_recv(&self) -> Result<Option<LlStreamEvent>, LlmError>;
}

// Level 2: Background thread handles SSE stream
// The adapter spawns a thread that:
// 1. Reads SSE events from reqwest response
// 2. Pushes LlStreamEvent to crossbeam channel
// 3. Closes channel when done
```

---

## 2. Stream Production (Inside Adapter)

```rust
impl LlProvider for OpenAiAdapter {
    fn chat_stream(&self, request: ProviderRequest) -> Result<ProviderStream, LlmError> {
        let (tx, rx) = crossbeam_channel::bounded::<ProviderStreamEvent>(256);

        let http_client = self.http_client.clone();
        let api_url = self.config.api_url.clone();
        let api_key = self.config.api_key.clone();

        // Spawn background thread for SSE reading
        std::thread::spawn(move || {
            let body = build_openai_request(&request, true);
            let response = match http_client
                .post(&api_url)
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&body)
                .send()
            {
                Ok(r) => r,
                Err(e) => { let _ = tx.send(ProviderStreamEvent::Error(e.into())); return; }
            };

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            // Read SSE events
            while let Some(chunk_result) = futures::executor::block_on(stream.next()) {
                match chunk_result {
                    Ok(chunk) => {
                        buffer.push_str(&String::from_utf8_lossy(&chunk));
                        for event in parse_sse_events(&buffer) {
                            match map_sse_to_stream_event(&event) {
                                Some(ProviderStreamEvent::Chunk { .. }) => {
                                    let _ = tx.send(ProviderStreamEvent::Chunk { .. });
                                }
                                Some(ProviderStreamEvent::Done { .. }) => {
                                    let _ = tx.send(ProviderStreamEvent::Done { .. });
                                    return;
                                }
                                None => {}
                            }
                        }
                        buffer.clear();
                    }
                    Err(e) => {
                        let _ = tx.send(ProviderStreamEvent::Error(e.into()));
                        return;
                    }
                }
            }
        });

        Ok(ProviderStream { receiver: rx })
    }
}
```

---

## 3. Stream Consumption (Gateway Level)

```rust
impl LlGateway {
    pub fn chat_stream(&self, request: LlRequest) -> Result<LlStreamHandle, LlmError> {
        let resolved = self.router.resolve(
            request.capability.as_deref().unwrap_or("chat"),
            &[],
        )?;

        // Translate LlRequest → ProviderRequest
        let provider_request = self.prepare_provider_request(&request, &resolved);

        // Cap context window
        let provider_request = self.enforce_context_window(provider_request, &resolved)?;

        // Check cache
        if let Some(cached) = self.cache.get(&request) {
            let (tx, rx) = crossbeam_channel::bounded(1);
            tx.send(LlStreamEvent::Chunk(LlStreamChunk {
                content: cached.message.content.to_string(),
                finish_reason: Some(cached.finish_reason),
                tool_calls: vec![],
                index: 0,
            }));
            tx.send(LlStreamEvent::Done(cached.usage));
            return Ok(LlStreamHandle {
                receiver: rx,
                model: resolved.model_id,
                provider: resolved.provider_id,
            });
        }

        // Get provider
        let provider = self.registry.get_provider(&resolved.provider_id)?;
        if !provider.capabilities().contains(&ProviderCapability::Streaming) {
            return Err(LlmError::StreamingUnsupported);
        }

        // Call provider's chat_stream
        let provider_stream = provider.chat_stream(provider_request)?;
        let (tx, rx) = crossbeam_channel::bounded(256);

        // Spawn aggregation thread
        let cost_tracker = self.cost.clone();
        let telemetry = self.telemetry.clone();
        let cache = self.cache.clone();
        let model_id = resolved.model_id.clone();
        let provider_id = resolved.provider_id.clone();

        std::thread::spawn(move || {
            let mut full_content = String::new();
            let mut usage = LlUsage::default();

            while let Ok(event) = provider_stream.receiver.recv() {
                match event {
                    ProviderStreamEvent::Chunk { content, finish_reason } => {
                        full_content.push_str(&content);
                        let _ = tx.send(LlStreamEvent::Chunk(LlStreamChunk {
                            content,
                            finish_reason,
                            tool_calls: vec![],
                            index: 0,
                        }));
                    }
                    ProviderStreamEvent::Done { input_tokens, output_tokens } => {
                        usage.input_tokens = input_tokens;
                        usage.output_tokens = output_tokens;
                        usage.total_tokens = input_tokens + output_tokens;
                        usage.cost_estimate_cents = cost_tracker.estimate(
                            &model_id, input_tokens, output_tokens
                        );
                        // Record cost
                        cost_tracker.record(&model_id, input_tokens, output_tokens);
                        // Emit telemetry
                        telemetry.record_chat(model_id, provider_id, input_tokens, output_tokens);
                        // Emit event
                        // event_bus.publish(LlmRequestCompleted { ... });
                        let _ = tx.send(LlStreamEvent::Done(usage));
                    }
                    ProviderStreamEvent::Error(e) => {
                        cost_tracker.record_failure(&model_id);
                        let _ = tx.send(LlStreamEvent::Error(e));
                    }
                }
            }
        });

        Ok(LlStreamHandle {
            receiver: rx,
            model: resolved.model_id,
            provider: resolved.provider_id,
        })
    }
}
```

---

## 4. Consumer Patterns

### Blocking Consumer (Planner Use Case)

```rust
// Planner collects full response from stream
let handle = gateway.chat_stream(request)?;

let mut full_response = LlResponse {
    message: LlMessage::assistant(""),
    finish_reason: LlFinishReason::Stop,
    usage: LlUsage::default(),
    model: handle.model.clone(),
    provider: handle.provider.clone(),
};

while let Ok(event) = handle.recv() {
    match event {
        LlStreamEvent::Chunk(chunk) => {
            full_response.append_content(&chunk.content);
            if let Some(reason) = chunk.finish_reason {
                full_response.finish_reason = reason;
            }
        }
        LlStreamEvent::Done(usage) => {
            full_response.usage = usage;
            break;
        }
        LlStreamEvent::Error(e) => return Err(e),
    }
}
```

### Non-Blocking Consumer (UI / Interactive)

```rust
let handle = gateway.chat_stream(request)?;

// Poll for events without blocking
loop {
    match handle.try_recv() {
        Ok(Some(LlStreamEvent::Chunk(chunk))) => {
            // Update UI with chunk.content
        }
        Ok(Some(LlStreamEvent::Done(usage))) => {
            // Mark complete
            break;
        }
        Ok(Some(LlStreamEvent::Error(e))) => {
            // Handle error
            break;
        }
        Ok(None) => {
            // No data yet, continue loop
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(e) => break,
    }
}
```

---

## 5. Timeout for Streams

```rust
// Gateway enforces streaming timeout
let timeout = Duration::from_secs(config.timeout.streaming_secs);
let timeout_instant = Instant::now() + timeout;

while let Ok(event) = handle.recv() {
    if Instant::now() > timeout_instant {
        return Err(LlmError::Timeout { elapsed_ms: timeout.as_millis() as u64 });
    }
    // Process event
}
```

---

## 6. Design Decisions

| Decision | Rationale |
|---|---|
| Streams use background threads | Sync runtime can't await; thread + channel is the simplest sync-compatible pattern |
| Channel capacity is bounded (256) | Backpressure — if consumer is slow, producer blocks on channel |
| No async runtime dependency | browseros-llm does not depend on tokio or async-std |
| Streaming is optional per provider | Adapter returns `StreamingUnsupported` if provider doesn't support it |
| Gateway aggregates stream into final response | Consumers can use either streaming or aggregated interface |
| Cache stores final response, not stream | Streaming requests are cached as completed responses |
