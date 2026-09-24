# Valkey Manager

Native desktop client for Valkey, built with Rust, egui/eframe, and `fred`.

## Run

Install the Rust toolchain and the system development libraries required by egui's
windowing and OpenGL backends, then run from the repository root:

```sh
cargo run
```

## Install a release

- **Linux:** download the `.AppImage`, make it executable with `chmod +x`, then run it.
- **macOS:** open the `.dmg` and drag Valkey Manager to Applications. The `.app.zip` is an alternative.

The macOS app is currently unsigned and not notarized. If Gatekeeper says it is damaged, use the app-specific quarantine workaround and signing setup in [macOS Gatekeeper and signing](docs/macos-gatekeeper.md).

The current native UI supports saved standalone connections, password storage in the
operating system keychain, a glob-based browser (up to 500 keys per scan), key type
and TTL inspection, bounded previews for strings, lists, hashes, sets, and sorted
sets, string value editing, collection member/field updates, safe rename, confirmed
deletion, an asynchronous command console, and a basic server monitor (`PING`,
version, uptime, clients, memory, processed commands, and database key count).
Network work does not block the UI.

For authenticated servers, enter the URL (for example `rediss://user@host:6380`),
enter the password in the masked password field, and save the profile. The profile
file stores the endpoint without its password; the secret stays in the system
keychain.

## Application architecture

Valkey Manager is a native Rust/egui desktop application; the active application does
not use Tauri, a webview, or a JavaScript frontend. Current connections support
standalone Valkey endpoints. Sentinel, Cluster, and Pub/Sub support remain future work.
