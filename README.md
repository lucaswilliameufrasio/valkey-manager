# Valkey Manager

Native desktop client for Valkey, built with Rust, egui/eframe, and `fred`.

## Run

Install the Rust toolchain and the system development libraries required by egui's
windowing and OpenGL backends, then run from the repository root:

```sh
cargo run
```

The current native UI connects to a standalone Valkey endpoint using a Redis URL
(for example `redis://127.0.0.1:6379`) and scans keys using a glob pattern. Connection
work runs asynchronously so network operations do not block the UI.

## Current migration status

The original Tauri/Vue implementation is preserved in the repository while its
functionality is migrated to the native egui app. The current egui slice provides
standalone connection and key browsing; connection profiles, secure credential
storage, key inspection/editing, commands, monitoring, and Sentinel/Cluster support
remain to be migrated.
