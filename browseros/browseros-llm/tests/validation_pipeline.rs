// Bootstrap file: registers `tests/validation/` as a test target.
// Cargo only discovers *.rs files directly under `tests/`,
// so this file re-exports the validation subdirectory as a module.

#[path = "validation/mcp_validation.rs"]
mod mcp_validation;

#[path = "validation/mod.rs"]
mod validation;
