#!/usr/bin/env bash
# Runs the SDK integration suite against a throwaway local validator.
#
# The escrow program is loaded from the compiled `.so`; the validator's ledger
# lives in a temp directory and is deleted on exit. No cluster credentials and
# no Devnet/Mainnet connection are involved.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PROGRAM_ID="GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR"
PROGRAM_SO="target/deploy/openwork_escrow.so"
RPC_PORT="${OPENWORK_TEST_PORT:-8899}"
FAUCET_PORT=$((RPC_PORT + 1))
RPC_URL="http://127.0.0.1:${RPC_PORT}"

if [[ ! -f "$PROGRAM_SO" ]]; then
  echo "==> building program"
  NO_DNA=1 anchor build
fi

if [[ ! -f "$PROGRAM_SO" ]]; then
  echo "error: $PROGRAM_SO not found after build" >&2
  exit 1
fi

LEDGER_DIR="$(mktemp -d "${TMPDIR:-/tmp}/openwork-ledger-XXXXXX")"
LOG_FILE="${LEDGER_DIR}/validator.log"
VALIDATOR_PID=""

cleanup() {
  if [[ -n "$VALIDATOR_PID" ]] && kill -0 "$VALIDATOR_PID" 2>/dev/null; then
    kill "$VALIDATOR_PID" 2>/dev/null || true
    wait "$VALIDATOR_PID" 2>/dev/null || true
  fi
  rm -rf "$LEDGER_DIR"
}
trap cleanup EXIT

echo "==> starting local validator on ${RPC_URL}"
solana-test-validator \
  --reset \
  --quiet \
  --ledger "$LEDGER_DIR/ledger" \
  --rpc-port "$RPC_PORT" \
  --faucet-port "$FAUCET_PORT" \
  --bpf-program "$PROGRAM_ID" "$PROGRAM_SO" \
  >"$LOG_FILE" 2>&1 &
VALIDATOR_PID=$!

for _ in $(seq 1 120); do
  if curl -sf -X POST "$RPC_URL" \
    -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' >/dev/null 2>&1; then
    READY=1
    break
  fi
  if ! kill -0 "$VALIDATOR_PID" 2>/dev/null; then
    echo "error: validator exited during startup" >&2
    cat "$LOG_FILE" >&2
    exit 1
  fi
  sleep 0.5
done

if [[ "${READY:-0}" != "1" ]]; then
  echo "error: validator did not become ready" >&2
  cat "$LOG_FILE" >&2
  exit 1
fi

echo "==> running integration suite"
OPENWORK_TEST_RPC_URL="$RPC_URL" \
  pnpm --filter @openwork/sdk exec vitest run tests/lifecycle.integration.test.ts
