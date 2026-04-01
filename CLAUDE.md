# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Helix is a modal text editor inspired by Kakoune/Neovim, written in Rust. It's a terminal-based editor with built-in language server support, tree-sitter syntax highlighting, and multiple selections.

## Development Commands

### Building and Running
- Build: `cargo build`
- Run development version: `cargo run` (faster to compile than release)
- Run with log file: `cargo run -- --log foo.log`
- Build release: `cargo build --release`
- **Faster compile times**: Consider using [mold](https://github.com/rui314/mold) linker (mentioned in CONTRIBUTING.md)

### Testing
- Unit tests: `cargo test --workspace`
- Integration tests: `cargo integration-test` (requires `--features integration` flag)
- Single test: `cargo test --workspace --test <test_name>`
- Set log level for integration tests: `HELIX_LOG_LEVEL=debug cargo integration-test`

### Linting and Formatting
- Format check: `cargo fmt --all --check`
- Format: `cargo fmt --all`
- Clippy: `cargo clippy --workspace --all-targets -- -D warnings`
- Documentation: `cargo doc --no-deps --workspace --document-private-items`

### Development Tasks (xtask)
- Generate documentation: `cargo xtask docgen` (generates files for mdbook)
- Check tree-sitter queries: `cargo xtask query-check [languages]`
- Check theme files: `cargo xtask theme-check [themes]`
- View help: `cargo xtask`

### Documentation
- Preview book: `mdbook serve book` then visit http://localhost:3000

## Architecture

Helix follows a modular crate architecture:

- **helix-core**: Functional editing primitives (Rope data structure, selections, transactions)
- **helix-view**: UI abstractions for backends (Documents, Views, Editor state)
- **helix-term**: Terminal UI frontend (commands, keymaps, application loop)
- **helix-tui**: TUI primitives (forked from tui-rs)
- **helix-lsp**: Language server client
- **helix-lsp-types**: LSP type definitions
- **helix-dap**: Debug Adapter Protocol client
- **helix-dap-types**: DAP type definitions
- **helix-event**: Event system with hooks and debouncing
- **helix-loader**: Loading external resources (grammars, themes)
- **helix-vcs**: Version control integration
- **helix-parsec**: Parser combinator library
- **helix-stdx**: Standard library extensions

### Key Design Principles
1. **Functional core**: `helix-core` operations return new copies rather than modifying in place
2. **Transaction-based**: Changes are made via `Transaction`s that can be inverted for undo
3. **Multi-view**: Multiple views can display the same document
4. **Rope-based**: Text uses `ropey` for efficient editing operations
5. **Tree-sitter integration**: Syntax highlighting and code analysis via tree-sitter

### Important Files
- `helix-term/src/commands.rs`: All editor commands
- `helix-term/src/keymap.rs`: Keybindings
- `helix-term/tests/test/helpers.rs`: Integration test helpers
- `docs/architecture.md`: High-level architecture overview
- `docs/CONTRIBUTING.md`: Contribution guidelines

## Configuration and Dependencies

- **MSRV**: 1.87 (Minimum Stable Rust Version, follows Firefox's policy)
- **MSRV update locations**: When increasing MSRV, update three locations:
  1. `workspace.package.rust-version` in root `Cargo.toml`
  2. `env.MSRV` in `.github/workflows/build.yml`
  3. `toolchain.channel` in `rust-toolchain.toml`
- **Toolchain**: Stable Rust with rustfmt, clippy components (specified in `rust-toolchain.toml`)
- **rust-analyzer**: If not using Nix dev shell, may need `rustup component add rust-analyzer` (toolchain selects MSRV but doesn't download matching rust-analyzer automatically)
- **Cargo aliases**: Defined in `.cargo/config.toml`:
  - `integration-test`: `test --features integration --profile integration --workspace --test integration`
  - `xtask`: `run --package xtask --`
- **Primary dependencies**: tree-sitter, ropey, tokio, parking_lot
- **Workspace**: Defined in root `Cargo.toml` with multiple member crates
- **Build profiles**: `release` (optimized), `opt` (more aggressive optimization), `integration` (for integration tests)

## Development Guidelines

From `.specify/memory/constitution.md`:
1. **Rust-First & MSRV Policy**: Follow Rust best practices and Firefox's MSRV policy
2. **Modular Architecture**: Maintain clear separation between crates
3. **Integration Testing**: Non-negotiable requirement for code contributions
4. **Functional Core Design**: Core editing primitives must be functional
5. **Performance & Simplicity**: Prioritize performance; complexity must be justified

## Cursor Rules

Cursor AI rules are in `.cursor/rules/specify-rules.mdc`. Key points:
- Use Rust (MSRV per Firefox policy)
- Commands: `cargo test; cargo clippy`
- Follow standard Rust conventions

**Note**: `.cursor/commands/` contains speckit command templates for development workflow (specification, planning, implementation tasks).

## Logging and Debugging

- Use `log::info!`, `warn!`, or `error!` macros for debug output
- Enable logs: `hx -v <file>` for info level, more `v`s for higher verbosity
- View logs: `:log-open` command in Helix or `tail -f foo.log` externally

## Testing Notes

- Integration tests use `helix-term/tests/test/helpers.rs`
- MacOS may need `ulimit -n 10240` to avoid "Too many open files" errors
- Always write integration tests for new functionality

## Common Workflow

1. Make changes
2. Run `cargo test --workspace`
3. Run `cargo integration-test` 
4. Run `cargo fmt --all --check`
5. Run `cargo clippy --workspace --all-targets -- -D warnings`
6. If modifying documentation: `cargo xtask docgen` and commit changes