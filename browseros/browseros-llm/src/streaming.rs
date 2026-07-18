//! Streaming — synchronous stream handle for LLM streaming responses.

use crate::error::LlmError;
use crate::types::{LlFinishReason, LlToolCallDelta, LlUsage};

/// Handle to a streaming response.
pub struct LlStreamHandle {
    pub model: String,
    pub provider: String,
    receiver: std::sync::mpsc::Receiver<LlStreamEvent>,
}

impl std::fmt::Debug for LlStreamHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlStreamHandle")
            .field("model", &self.model)
            .field("provider", &self.provider)
            .finish()
    }
}

/// Events from a streaming response.
#[derive(Debug, Clone)]
pub enum LlStreamEvent {
    Chunk(LlStreamChunk),
    Done(LlUsage),
    Error(LlmError),
}

/// A single streaming chunk.
///
/// Chunks are delivered in monotonically increasing index order.
/// Index 0 is always the first chunk. Gaps (non-contiguous indices)
/// are possible when a chunk's processing is deferred; consumers
/// should not assume strict sequential contiguity.
#[derive(Debug, Clone)]
pub struct LlStreamChunk {
    pub content: String,
    pub finish_reason: Option<LlFinishReason>,
    pub tool_calls: Vec<LlToolCallDelta>,
    /// 0-based monotonically increasing index. Non-guaranteed contiguous.
    pub index: usize,
}

impl LlStreamHandle {
    /// Create a new stream handle.
    pub fn new(
        model: String,
        provider: String,
        receiver: std::sync::mpsc::Receiver<LlStreamEvent>,
    ) -> Self {
        Self {
            model,
            provider,
            receiver,
        }
    }

    /// Blocking receive — returns next event or disconnected.
    pub fn recv(&self) -> Result<LlStreamEvent, LlmError> {
        self.receiver
            .recv()
            .map_err(|_| LlmError::TransportError("stream disconnected".into()))
    }

    /// Non-blocking try_receive.
    pub fn try_recv(&self) -> Result<Option<LlStreamEvent>, LlmError> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(_) => Err(LlmError::TransportError("stream disconnected".into())),
        }
    }

    /// Iterator interface.
    pub fn iter(&self) -> LlStreamIterator<'_> {
        LlStreamIterator { handle: self }
    }
}

/// Iterator over stream events.
pub struct LlStreamIterator<'a> {
    handle: &'a LlStreamHandle,
}

impl<'a> Iterator for LlStreamIterator<'a> {
    type Item = Result<LlStreamEvent, LlmError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.handle.recv() {
            Ok(LlStreamEvent::Done(_)) => None,
            Ok(event) => Some(Ok(event)),
            Err(e) => Some(Err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::LlUsage;

    fn make_chunk(content: &str) -> LlStreamEvent {
        LlStreamEvent::Chunk(LlStreamChunk {
            content: content.to_string(),
            finish_reason: None,
            tool_calls: vec![],
            index: 0,
        })
    }

    fn make_done() -> LlStreamEvent {
        LlStreamEvent::Done(LlUsage {
            input_tokens: 10,
            output_tokens: 20,
            total_tokens: 30,
            cost_estimate_cents: 0.05,
            currency: "USD".into(),
        })
    }

    fn make_error() -> LlStreamEvent {
        LlStreamEvent::Error(LlmError::ProviderError("test error".into()))
    }

    #[test]
    fn handle_new_and_debug() {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        let _ = tx.send(make_chunk("hello"));
        assert_eq!(handle.model, "gpt-4");
        assert_eq!(handle.provider, "openai");
        let dbg = format!("{:?}", &handle);
        assert!(dbg.contains("gpt-4"));
        assert!(dbg.contains("openai"));
    }

    #[test]
    fn recv_receives_chunk() {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        tx.send(make_chunk("hello")).unwrap();
        let event = handle.recv().unwrap();
        match event {
            LlStreamEvent::Chunk(c) => assert_eq!(c.content, "hello"),
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn recv_receives_done() {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        tx.send(make_done()).unwrap();
        match handle.recv().unwrap() {
            LlStreamEvent::Done(u) => assert_eq!(u.input_tokens, 10),
            _ => panic!("expected Done"),
        }
    }

    #[test]
    fn recv_disconnected_returns_transport_error() {
        let (_tx, rx) = std::sync::mpsc::channel::<LlStreamEvent>();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        drop(_tx);
        match handle.recv() {
            Err(LlmError::TransportError(_)) => {}
            _ => panic!("expected TransportError"),
        }
    }

    #[test]
    fn try_recv_returns_none_when_empty() {
        let (_tx, rx) = std::sync::mpsc::channel::<LlStreamEvent>();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        assert!(handle.try_recv().unwrap().is_none());
    }

    #[test]
    fn try_recv_returns_some_when_available() {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        tx.send(make_chunk("hello")).unwrap();
        let event = handle.try_recv().unwrap().unwrap();
        match event {
            LlStreamEvent::Chunk(c) => assert_eq!(c.content, "hello"),
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn try_recv_disconnected_returns_transport_error() {
        let (tx, rx) = std::sync::mpsc::channel::<LlStreamEvent>();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        drop(tx);
        match handle.try_recv() {
            Err(LlmError::TransportError(_)) => {}
            other => panic!("expected TransportError, got {:?}", other),
        }
    }

    #[test]
    fn iter_yields_chunks_and_stops_at_done() {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        tx.send(make_chunk("a")).unwrap();
        tx.send(make_chunk("b")).unwrap();
        tx.send(make_done()).unwrap();

        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert_eq!(events.len(), 2);
        match &events[0] {
            LlStreamEvent::Chunk(c) => assert_eq!(c.content, "a"),
            _ => panic!("expected Chunk"),
        }
        match &events[1] {
            LlStreamEvent::Chunk(c) => assert_eq!(c.content, "b"),
            _ => panic!("expected Chunk"),
        }
    }

    #[test]
    fn iter_stops_at_done_no_more_events() {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        tx.send(make_chunk("a")).unwrap();
        tx.send(make_done()).unwrap();
        tx.send(make_chunk("b")).unwrap();

        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn iter_yields_error_variant_and_continues() {
        let (tx, rx) = std::sync::mpsc::channel();
        let handle = LlStreamHandle::new("gpt-4".into(), "openai".into(), rx);
        tx.send(make_chunk("a")).unwrap();
        tx.send(make_error()).unwrap();
        tx.send(make_chunk("b")).unwrap();
        tx.send(make_done()).unwrap();

        let events: Vec<LlStreamEvent> = handle.iter().filter_map(|r| r.ok()).collect();
        assert_eq!(events.len(), 3);
        match &events[0] {
            LlStreamEvent::Chunk(c) => assert_eq!(c.content, "a"),
            _ => panic!("expected Chunk at [0]"),
        }
        match &events[1] {
            LlStreamEvent::Error(e) => assert!(e.to_string().contains("test error")),
            _ => panic!("expected Error at [1]"),
        }
        match &events[2] {
            LlStreamEvent::Chunk(c) => assert_eq!(c.content, "b"),
            _ => panic!("expected Chunk at [2]"),
        }
    }

    #[test]
    fn handle_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<LlStreamHandle>();
    }

    #[test]
    fn stream_chunk_clone() {
        let chunk = LlStreamChunk {
            content: "hello".into(),
            finish_reason: None,
            tool_calls: vec![],
            index: 0,
        };
        let cloned = chunk.clone();
        assert_eq!(chunk.content, cloned.content);
        assert_eq!(chunk.index, cloned.index);
    }

    #[test]
    fn stream_chunk_clone_independence() {
        let mut chunk = LlStreamChunk {
            content: "hello".into(),
            finish_reason: None,
            tool_calls: vec![],
            index: 0,
        };
        let cloned = chunk.clone();
        chunk.content = "modified".into();
        assert_eq!(cloned.content, "hello");
    }

    #[test]
    fn stream_event_clone_chunk() {
        let event = LlStreamEvent::Chunk(LlStreamChunk {
            content: "test".into(),
            finish_reason: None,
            tool_calls: vec![],
            index: 0,
        });
        let cloned = event.clone();
        match (&event, &cloned) {
            (LlStreamEvent::Chunk(a), LlStreamEvent::Chunk(b)) => {
                assert_eq!(a.content, b.content);
            }
            _ => panic!("expected Chunk variants"),
        }
    }

    #[test]
    fn stream_event_clone_done() {
        let usage = LlUsage {
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            cost_estimate_cents: 0.01,
            currency: "USD".into(),
        };
        let event = LlStreamEvent::Done(usage);
        let cloned = event.clone();
        match (&event, &cloned) {
            (LlStreamEvent::Done(a), LlStreamEvent::Done(b)) => {
                assert_eq!(a.total_tokens, b.total_tokens);
            }
            _ => panic!("expected Done variants"),
        }
    }

    #[test]
    fn stream_event_clone_error() {
        let event = LlStreamEvent::Error(LlmError::ProviderError("test".into()));
        let cloned = event.clone();
        match (&event, &cloned) {
            (LlStreamEvent::Error(a), _) => {
                assert!(a.to_string().contains("test"));
            }
            _ => panic!("expected Error variants"),
        }
    }

    #[test]
    fn stream_event_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<LlStreamEvent>();
        assert_send::<LlStreamChunk>();
    }
}
