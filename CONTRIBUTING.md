# Contributing to BrowserOS

## Code of Conduct

Be respectful, inclusive, and constructive. Harassment and discriminatory behavior will not be tolerated.

## Getting Started

1. Ensure you have Rust nightly installed
2. Clone the repository
3. Run `cargo build --workspace` to verify the build
4. Run `cargo test --workspace` to run all tests

## Development Workflow

- All code changes require tests
- Maintain >80% coverage on new code
- Run `cargo fmt --check` and `cargo clippy --workspace -- -D warnings` before committing
- Keep the API documented with doc comments on all public items

## Crate Conventions

- `browseros/` contains only Rust source code — all documentation lives in `.vibe/`
- Each crate has a single responsibility (see `Cargo.toml` workspace members)
- Public APIs are frozen per workspace architecture documents

## Testing

- Unit tests live next to the code they test (same file, `#[cfg(test)]` module)
- Integration tests live in `tests/` directories within each crate
- Some tests require a running Chrome instance (`smoke_browser_launch`)

## Pull Requests

1. Fork the repository
2. Create a feature branch
3. Make your changes with tests
4. Run quality gates: `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace`
5. Submit a pull request
