#!/usr/bin/env bash
set -euo pipefail

# Synthetic account fixtures must never migrate their tokens into the OS keychain.
project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$project_root"
export AGENT_BROWSER_NO_KEYCHAIN=1
exec cargo test -p browser_memory --tests "$@"
