use std::sync::atomic::Ordering;

use crate::error::LlmError;
use crate::mcp::errors::McpError;
use crate::mcp::McpAdapter;
use crate::provider::LlProvider;
use crate::types::{
    LlContent, LlFinishReason, LlRole, ProviderCapability, ProviderEmbedRequest,
    ProviderEmbedResponse, ProviderHealthResult, ProviderMessage, ProviderRequest,
    ProviderResponse, ProviderStream,
};

impl LlProvider for McpAdapter {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> Vec<ProviderCapability> {
        vec![
            ProviderCapability::Chat,
            ProviderCapability::ToolUse,
            ProviderCapability::FunctionCalling,
        ]
    }

    fn models(&self) -> Vec<String> {
        vec![self.id.clone()]
    }

    fn chat(&self, request: ProviderRequest) -> Result<ProviderResponse, LlmError> {
        self.ensure_spawned()
            .map_err(|e| LlmError::ProviderError(format!("spawn failed: {e}")))?;

        if !self.initialized.load(Ordering::Acquire) {
            self.initialize()
                .map_err(|e| LlmError::ProviderUnavailable(format!("deferred init failed: {e}")))?;
        }

        let tool_call = request.messages.iter().rev().find_map(|msg| {
            if let LlContent::ToolCall {
                name, arguments, ..
            } = &msg.content
            {
                Some((name.clone(), arguments.clone()))
            } else {
                None
            }
        });

        let (tool_name, tool_args) = tool_call.ok_or_else(|| {
            McpError::InvalidRequest("no ToolCall message found in request".into())
        })?;

        let timeout = std::cmp::max(request.timeout_ms, 5_000);
        let result = self
            .with_client(|client| {
                client.call_tool(
                    &tool_name,
                    serde_json::Value::Object(tool_args.into_iter().collect()),
                    timeout,
                )
            })
            .map_err(|e| LlmError::ProviderError(format!("tool call failed: {e}")))?;

        let output = result
            .content
            .iter()
            .map(|c| c.text.clone())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ProviderResponse {
            message: ProviderMessage {
                role: LlRole::Tool,
                content: LlContent::ToolResult {
                    call_id: String::new(),
                    output,
                },
            },
            finish_reason: LlFinishReason::Stop,
            input_tokens: 0,
            output_tokens: 0,
            model: self.id.clone(),
        })
    }

    fn chat_stream(&self, _request: ProviderRequest) -> Result<ProviderStream, LlmError> {
        Err(LlmError::StreamingUnsupported)
    }

    fn embed(&self, _request: ProviderEmbedRequest) -> Result<ProviderEmbedResponse, LlmError> {
        Err(LlmError::CapabilityNotSupported("embedding".into()))
    }

    fn health(&self) -> ProviderHealthResult {
        McpAdapter::health(self)
    }
}
