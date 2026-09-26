#!/bin/bash
# Compatibility entry point; the shared builder also runs natively in PowerShell.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
exec bun ./build-wasm.mjs "$@"
