# Valkey Manager

Native desktop client for Valkey, built with Rust, egui/eframe, and `fred`.

## Run

Install the Rust toolchain and the system development libraries required by egui's
windowing and OpenGL backends, then run from the repository root:

```sh
cargo run
```

The current native UI supports saved standalone connections, password storage in the
operating system keychain, a glob-based browser (up to 500 keys per scan), key type
and TTL inspection, bounded previews for strings, lists, hashes, sets, and sorted
sets, string value editing, safe rename, confirmed deletion, an asynchronous command
console, and a basic server monitor (`PING`, version, uptime, clients, memory,
processed commands, and database key count). Network work does not block the UI.

For authenticated servers, enter the URL (for example `rediss://user@host:6380`),
enter the password in the masked password field, and save the profile. The profile
file stores the endpoint without its password; the secret stays in the system
keychain.

## Current migration status

The original Tauri/Vue implementation is preserved in the repository while its
functionality is migrated to the native egui app. The current egui slice provides
standalone connection profiles, keychain-backed credentials, and basic key browsing
and string operations, console, and monitoring. Collection editing and Sentinel/
Cluster support remain to be migrated.
