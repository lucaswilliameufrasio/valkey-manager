# Applying system-module-architecture to Valkey Manager

The project-wide architecture standard lives in
[`.agents/skills/system-module-architecture/SKILL.md`](../.agents/skills/system-module-architecture/SKILL.md).
Use it before adding a major screen, Valkey capability, or connection topology.
The skill is stack-neutral; apply its typed-contract and field-validation guidance
through Rust types, validation functions, and egui async state in this project.

## Current capability boundaries

| Capability | Primary intent | State and failures | Integration boundary |
| --- | --- | --- | --- |
| Connections | Create/select/connect/disconnect a standalone profile | disconnected, connecting, connected, disconnecting, failed; invalid URL, keychain unavailable, auth/TLS/network failure | `profile.rs` persists typed metadata in TOML and secrets in the OS keychain; Fred client owns protocol operations |
| Keyspace | Search and inspect bounded results, then operate on one selected key | scanning, selected, loading, mutating, failed; scan/TTL/type/read/write errors | Fred SCAN/TYPE/TTL and typed commands; previews are capped to avoid loading an entire database into UI state |
| Command console | Execute an explicitly entered Valkey command | idle, running, succeeded, failed/timed out | Fred custom-command boundary, shell-style argument parsing only; no shell is invoked |
| Server monitor | Inspect health and basic server statistics | empty, refreshing, loaded, failed | PING, INFO, and DBSIZE through Fred |

The current window uses a connection panel and separate Keys, Console, and Monitor
tabs. Keep those intents distinct as the UI grows. A raw command console is an
intentional power-user exception to normal domain commands; keep it isolated and
clearly indicate that commands can mutate remote data.

This is a single-user desktop app, so permissions are primarily the capabilities of
the selected Valkey ACL account rather than application roles. Destructive actions
still need confirmation because they affect remote data.

## Boundaries to preserve in future modules

- A selected connection scopes all key operations; do not let a command or event
  silently use a different profile after a switch.
- Keep Fred and RESP conversion in named Valkey services/adapters. UI code should
  pass typed requests and receive typed results instead of assembling protocol
  arguments throughout a large render function.
- Represent topology (standalone, Cluster, Sentinel) as explicit connection
  configuration. Cluster routing and Sentinel discovery belong to the connection
  capability; do not scatter topology checks across each editor.
- Model Pub/Sub and Streams as separate event/long-running workflows with explicit
  subscribe, unsubscribe, reconnect, cancellation, and error states.
- Keep destructive actions isolated behind confirmation. Prefer non-overwriting
  operations such as RENAMENX/NX when they match the intent.
- Keep preview limits and pagination in the service contract; do not fetch
  unbounded collections just because an egui ScrollArea can scroll.
- Test protocol adapters against a real local Valkey instance. Unit tests should
  cover parsing, validation, and state transitions without replacing protocol
  integration coverage.

## Current architecture debt

`src/lib.rs` currently contains window state, UI rendering, asynchronous task
orchestration, and several Valkey calls in one module. The existing `profile.rs`
is a separate configuration/keychain boundary. When adding a substantial feature,
extract one vertical capability at a time (for example `connections/`, `keys/`,
`console/`, `monitor/`) with its typed state, service contract, errors, and tests;
avoid a broad file move that changes behavior simultaneously.
