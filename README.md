# OpenWork

**Milestone escrow for independent work, settled on Solana.**

A client funds a milestone into a program-owned vault. The freelancer submits a
deliverable. The client approves, and the program pays the exact funded amount
to the freelancer's token account. OpenWork never holds a key, never holds a
balance, and takes no fee.

The escrow lives in an Anchor program. The web app composes transactions locally
and asks the wallet to sign; the API is an optional index for titles, quotes and
confirmations. If this website disappeared, both parties could still settle the
agreement with any Solana wallet.

- Program (Devnet): [`GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR`](https://explorer.solana.com/address/GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR?cluster=devnet)
- End-to-end Devnet receipts: [`docs/deployments.md`](docs/deployments.md)
- Settlement mint: `ENmQKqTu9FaWJtmwk3nXiNT3GxX6F4k7rZe8t2bqUYT8` (`tUSDC`, no value)

## Status

| Area                | State                                                                                     |
| ------------------- | ----------------------------------------------------------------------------------------- |
| Escrow program      | Deployed to Devnet; 16 LiteSVM tests                                                      |
| SDK lifecycle       | 13 tests against a local validator with real SPL tokens                                   |
| Devnet smoke        | Real create → accept → fund → submit → release, signatures committed                      |
| Web app             | Builds and runs; 8 routes; wallet-standard signing, no adapter packages                   |
| Metadata API        | Unit, HTTP and reconciliation tests green; OpenAPI 3.0; needs Postgres to run             |
| Cross-chain funding | **Devnet simulation only** — the real 1Click adapter is mainnet-gated and unverified here |
| Hosted public URL   | Not provided; deploy your own with the instructions below                                 |
| Audit               | **None.** The tests are public; that is not an audit                                      |

## How it works

```
client wallet ──sign──┐
                      ├──▶ escrow program ──▶ agreement PDA (state)
freelancer wallet ────┘         │                milestone PDAs
                                │                vault authority PDA
                                ▼                      │
                        SPL token vault ◀──────────────┘
                                │
                       release / mutual refund
                                ▼
                     freelancer or client token account
```

1. The client creates a draft agreement with the freelancer, settlement mint and
   a `sha256` of the canonical terms.
2. Milestones are added in order, each with an exact amount.
3. The freelancer accepts by signing the same terms hash. A different hash is
   rejected on chain.
4. The client funds a milestone; tokens move into an associated token account
   owned by the agreement's vault-authority PDA.
5. The freelancer submits a deliverable hash.
6. The client approves: the program transfers exactly the recorded amount to the
   freelancer's token account and marks the milestone released.

A dispute freezes release. The only way value leaves a disputed milestone is a
mutual refund signed by **both** parties. There is no admin path, no fee path
and no timeout that lets one side take the money.

## Repository layout

```
programs/openwork-escrow/   Anchor program: state, 9 instructions, events, errors
packages/sdk/               Hand-written TypeScript client, IDL, unit + integration tests
apps/web/                   Next.js 16 app: landing, ledger, wizard, agreement, funding
apps/api/                   Rust/Axum metadata + reconciliation API, OpenAPI, worker
crates/domain/              Shared amount, terms and PDA logic (Rust)
crates/aurora-adapter/      NEAR 1Click adapter plus the labelled mock adapter
migrations/                 Postgres schema
scripts/                    Validator harness, Devnet smoke, IDL drift check
docs/                       Architecture, security, deployment, flows, API
```

## Quick start

Prerequisites: Node 20.18+, pnpm 10, Rust 1.89, Solana CLI 2.x, Anchor 1.1.2.

```sh
pnpm install
anchor build

# Program tests (in-process SVM)
cargo test -p openwork-escrow

# SDK unit tests
pnpm --filter @openwork/sdk test

# Web app
pnpm --filter @openwork/web dev     # http://localhost:3000
```

The web app is usable without the API: it reads agreements directly from the
RPC and shows a "chain-only" badge. To run the full stack you also need
Postgres 16+:

```sh
createdb openwork
cp apps/api/.env.example apps/api/.env    # set DATABASE_URL
cargo run -p openwork-api --bin api       # http://127.0.0.1:8080
cargo run -p openwork-api --bin worker    # confirms claimed signatures
```

Full-stack integration tests use a throwaway validator and need no network:

```sh
./scripts/test-integration.sh
```

## Verification

`make verify` runs the gate CI runs:

```sh
node scripts/check-idl.mjs                  # IDL vs SDK discriminators
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm -r typecheck
pnpm -r build
```

The Devnet smoke test is separate because it spends Devnet SOL and creates public
accounts:

```sh
./scripts/devnet-smoke.sh
```

It writes `docs/devnet-smoke.json`; the committed receipts include the program
deploy and every lifecycle step.

## Security model in one paragraph

Funds sit in a token account whose authority is a PDA derived from the agreement.
Only `approve_and_release` (client signature) or `mutual_cancel_and_refund`
(client **and** freelancer signatures) can debit it. The program has no fee
instruction, no admin instruction and no upgrade bypass beyond the Solana program
upgrade authority, which is disclosed. Management of that authority is the
single largest operational risk in v1.

Read [`docs/security.md`](docs/security.md) for the invariants and the known
limits, including the absence of arbitration.

## Cross-chain funding (honest version)

The funding screen is a **Devnet simulation**. The mock adapter mirrors the NEAR
1Click request/response shapes, marks every quote `simulated: true`, and stops at
`escrow_funding_pending`; it never claims a bridge delivered value. The real
adapter code exists behind `INTENTS_MODE=mainnet` and was not exercised against
Mainnet in this build (no partner credentials, no supported Aurora-origin route).
See [`docs/user-flows.md`](docs/user-flows.md#cross-chain-funding).

Even with a real adapter the flow is two-step: routed value lands in the client's
own wallet, then the client signs a separate `fund_milestone` transaction. A
bridge failure can therefore never be mistaken for escrow funding.

## Deployment

Devnet is already deployed; see [`docs/deployment.md`](docs/deployment.md) for
reproducing it, the Mainnet checklist and the multisig requirement.

## License

MIT. See [`LICENSE`](LICENSE).
