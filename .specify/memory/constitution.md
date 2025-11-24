<!--
Sync Impact Report:
Version: 0.0.0 → 1.0.0 (Initial creation)
Modified Principles: None (initial creation)
Added Sections: Core Principles, Technology Stack, Development Workflow, Governance
Removed Sections: None
Templates requiring updates:
  ✅ .specify/templates/plan-template.md (Constitution Check section exists)
  ✅ .specify/templates/spec-template.md (no direct constitution references)
  ✅ .specify/templates/tasks-template.md (no direct constitution references)
Follow-up TODOs: None
-->

# Helix Constitution

## Core Principles

### I. Rust-First & MSRV Policy

Helix MUST be written in Rust and follow Rust best practices. The Minimum Stable Rust Version (MSRV) policy MUST align with Firefox's MSRV policy. When increasing MSRV, update three locations: `workspace.package.rust-version` in root `Cargo.toml`, `env.MSRV` in `.github/workflows/build.yml`, and `toolchain.channel` in `rust-toolchain.toml`. Rationale: Ensures compatibility with downstream distributions and maintains consistency with established Rust ecosystem standards.

### II. Modular Architecture

Helix MUST maintain clear separation between crates: `helix-core` (functional editing primitives), `helix-view` (UI abstractions), `helix-term` (terminal UI), `helix-lsp` (language server client), and supporting crates. Each crate MUST have a well-defined purpose and minimal dependencies. Rationale: Enables independent development, testing, and potential reuse of components.

### III. Integration Testing (NON-NEGOTIABLE)

All code contributions MUST include integration tests where applicable. Integration tests MUST be runnable via `cargo integration-test` and MUST use helpers from `helix-term/tests/test/helpers.rs`. Unit tests MUST be runnable via `cargo test --workspace`. Rationale: Ensures editor functionality works correctly in realistic scenarios and prevents regressions.

### IV. Functional Core Design

Core editing primitives in `helix-core` MUST be functional: operations MUST return new copies rather than modifying data in place. Transactions MUST be invertible for undo support. Rationale: Enables safe snapshotting, undo/redo, and predictable state management.

### V. Performance & Simplicity

Code MUST prioritize performance where it impacts user experience (rendering, editing operations). Complexity MUST be justified. Start simple and follow YAGNI (You Aren't Gonna Need It) principles. Rationale: Helix is a performance-critical editor; unnecessary complexity degrades maintainability and user experience.

## Technology Stack

**Language**: Rust (MSRV per Firefox policy)  
**Primary Dependencies**: tree-sitter (syntax highlighting), ropey (rope data structure), LSP/DAP clients  
**Testing**: `cargo test --workspace` (unit/doc tests), `cargo integration-test` (integration tests)  
**Build System**: Cargo with workspace configuration  
**Target Platform**: Terminal-based editor (Linux, macOS, Windows)

## Development Workflow

**Testing Requirements**: 
- Unit tests MUST pass: `cargo test --workspace`
- Integration tests MUST pass: `cargo integration-test`
- Code contributors are STRONGLY ENCOURAGED to write integration tests

**Code Review**: All PRs MUST verify compliance with constitution principles. Complexity violations MUST be justified in PR descriptions.

**Documentation**: Architecture documentation MUST be maintained in `docs/architecture.md`. API documentation generated via `cargo doc --open`.

## Governance

This constitution supersedes all other development practices. Amendments require:
1. Documentation of the change rationale
2. Update to version number (semantic versioning: MAJOR for incompatible changes, MINOR for additions, PATCH for clarifications)
3. Update to `LAST_AMENDED_DATE`
4. Propagation to dependent templates and documentation

All PRs and code reviews MUST verify compliance with constitution principles. Complexity MUST be justified. Use `docs/CONTRIBUTING.md` for runtime development guidance.

**Version**: 1.0.0 | **Ratified**: 2025-11-14 | **Last Amended**: 2025-11-14
