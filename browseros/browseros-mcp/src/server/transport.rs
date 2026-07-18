use std::io::{BufRead, BufReader};

use crate::error::McpServerError;

/// Reads newline-delimited JSON-RPC messages from stdin.
pub struct StdioReader {
    reader: BufReader<std::io::Stdin>,
}

impl StdioReader {
    pub fn new() -> Self {
        StdioReader {
            reader: BufReader::new(std::io::stdin()),
        }
    }

    /// Reads a single line from stdin.
    ///
    /// Returns `McpServerError::TransportError` on EOF or I/O error.
    pub fn read_line(&mut self) -> Result<String, McpServerError> {
        let mut line = String::new();
        let n = self
            .reader
            .read_line(&mut line)
            .map_err(|e| McpServerError::TransportError(e.to_string()))?;
        if n == 0 {
            return Err(McpServerError::TransportError("stdin closed".into()));
        }
        Ok(line.trim_end_matches(['\r', '\n']).to_string())
    }
}

impl Default for StdioReader {
    fn default() -> Self {
        Self::new()
    }
}
