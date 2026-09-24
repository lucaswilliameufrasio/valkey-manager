# Contributor guidance

- This is a native Rust/egui desktop application. Keep the active app independent
  of Tauri, webviews, and JavaScript frontend tooling.
- Before adding or refactoring a capability, screen, Valkey operation, or service
  boundary, read `.agents/skills/system-module-architecture/SKILL.md` and its
  project-specific application in `docs/module-architecture.md`.
- Apply the architecture skill using Rust/egui equivalents: typed request/result
  models, field-level validation, async task state, explicit errors, and real
  Valkey integration tests.
- Before declaring Rust changes complete, run:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets --locked -- -D warnings`
  - `cargo test --workspace --locked`
  - `cargo build --workspace --locked`
