#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(git rev-parse --show-toplevel)"
compose_file="$repo_root/compose.yaml"
compose_project="valkey-manager-e2e"
valkey_port="${VALKEY_E2E_PORT:-16480}"

cleanup() {
    docker compose --project-name "$compose_project" --project-directory "$repo_root" \
        -f "$compose_file" down --remove-orphans
}
trap cleanup EXIT

VALKEY_PORT="$valkey_port" docker compose --project-name "$compose_project" \
    --project-directory "$repo_root" -f "$compose_file" up --detach --wait valkey
export VALKEY_TEST_URL="redis://127.0.0.1:$valkey_port"

cargo test --manifest-path "$repo_root/Cargo.toml" --locked \
    --test valkey_integration -- --ignored --nocapture
cargo test --manifest-path "$repo_root/Cargo.toml" --locked \
    --test valkey_performance_e2e -- --ignored --nocapture
