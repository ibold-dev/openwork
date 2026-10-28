#!/usr/bin/env bash
# Runs the real Devnet end-to-end smoke test.
#
# This spends Devnet SOL, creates a fresh SPL mint and a fresh agreement, and
# writes docs/devnet-smoke.json with every signature. It never touches Mainnet
# and never handles anything of value.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

RPC_URL="${OPENWORK_DEVNET_RPC_URL:-https://api.devnet.solana.com}"
KEYPAIR="${OPENWORK_DEVNET_CLIENT_KEYPAIR:-$HOME/.config/solana/id.json}"

if [[ ! -f "$KEYPAIR" ]]; then
  echo "error: client keypair not found at $KEYPAIR" >&2
  echo "set OPENWORK_DEVNET_CLIENT_KEYPAIR to a funded Devnet keypair" >&2
  exit 1
fi

BALANCE="$(solana balance -u "$RPC_URL" --keypair "$KEYPAIR" --output json |
  sed -E 's/.*"lamports":([0-9]+).*/\1/')"
if [[ -z "$BALANCE" || "$BALANCE" == "0" ]]; then
  echo "error: $KEYPAIR has no SOL on $RPC_URL" >&2
  exit 1
fi
echo "==> client balance: $BALANCE lamports"

echo "==> running Devnet lifecycle (this creates real Devnet accounts)"
OPENWORK_DEVNET_SMOKE=1 \
  OPENWORK_DEVNET_RPC_URL="$RPC_URL" \
  OPENWORK_DEVNET_CLIENT_KEYPAIR="$KEYPAIR" \
  pnpm --filter @openwork/sdk test:devnet

echo
echo "==> receipt written to docs/devnet-smoke.json"
