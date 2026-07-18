use std::path::Path;
use std::sync::Arc;

use browseros_mcp::{McpServerBuilder, SystemHealthTool, SystemVersionTool};
use browseros_runtime::RuntimeContext;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        println!("browseros-mcp-server");
        println!();
        println!("USAGE:");
        println!("  browseros_mcp_server [config-path]");
        println!();
        println!("ARGUMENTS:");
        println!("  config-path    Path to YAML config file (optional)");
        println!();
        println!("The server reads JSON-RPC messages from stdin and writes");
        println!("responses to stdout. Logs are written to stderr.");
        return;
    }

    let config_path = args.get(1).map(Path::new);

    let mut builder = RuntimeContext::builder().with_stderr_logging();
    if let Some(path) = config_path {
        builder = builder.config_file(path.to_path_buf());
    }
    let runtime = builder.build().unwrap_or_else(|e| {
        eprintln!("FATAL: failed to initialize runtime: {e}");
        std::process::exit(1);
    });

    let server = McpServerBuilder::new()
        .with_runtime(Arc::new(runtime))
        .with_tool("system/health", Box::new(SystemHealthTool::new()))
        .with_tool("system/version", Box::new(SystemVersionTool::new()))
        .build()
        .unwrap_or_else(|e| {
            eprintln!("FATAL: failed to build MCP server: {e}");
            std::process::exit(1);
        });

    eprintln!("INFO: browseros-mcp server starting");

    if let Err(e) = server.start() {
        eprintln!("FATAL: server error: {e}");
        std::process::exit(1);
    }

    eprintln!("INFO: browseros-mcp server shut down");
}
