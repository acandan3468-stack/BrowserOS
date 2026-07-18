// Phase 6.2+ may add a real cross-platform signal handler here.
// Phase 6.1 shutdown is driven by:
//   - stdin EOF (McpServer::start returns when reader returns TransportError)
//   - "shutdown" JSON-RPC request (sets shutdown_requested flag)
//   - "exit" JSON-RPC notification (sets shutdown_requested flag)
// Ctrl+C terminates the OS process without guaranteed cleanup.
// This is acceptable for Phase 6.1; real signal handling is tracked in Phase 6.2.
