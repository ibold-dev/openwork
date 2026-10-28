# Architecture

OpenWork is four layers with one rule between them: **the program is the only
authority over funds**. Everything else either prepares a transaction for a
wallet to sign, or reads state that the program already wrote.

```
┌──────────────────────────── browser ────────────────────────────┐
│  Next.js app (apps/web)                                         │
│  Wallet Standard discovery ──▶ wallet signs ──▶ RPC             │
│  Terms canonicalization, PDA derivation, wire transactions      │
└───────────────┬─────────────────────────────────────────────────┘
                │ optional JSON (titles, quotes, confirmations)
                ▼
┌──────────────────────────── API (apps/api) ─────────────────────┐
│  Axum · SIWS sessions · Postgres · outbox worker                │
│  Indexes metadata, verifies claims with getSignatureStatuses,   │
│  decodes program accounts and Anchor events                     │
└───────────────┬─────────────────────────────────────────────────┘
                │ read-only RPC
                ▼
┌──────────────────────────── Solana ─────────────────────────────┐
│  openwork-escrow (Anchor)                                       │
│  Agreement PDAs · Milestone PDAs · vault-authority PDA          │
│  SPL Token vault · events                                       │
└─────────────────────────────────────────────────────────────────┘
```

The web app never needs the API to move value. With the API offline it reads
agreements from the RPC and performs every instruction; it only loses off-chain
titles and quote history.

## 1. Escrow program

Location: `programs/openwork-escrow` · Framework: Anchor 1.1.2 · Rust 1.89

### Accounts

| Account         | Seeds                                | Contents                                                                                                             |
| --------------- | ------------------------------------ | -------------------------------------------------------------------------------------------------------------------- |
| `Agreement`     | `["agreement", client, nonce_le]`    | schema version, bump, state, client, freelancer, mint, nonce, terms hash, counts, total/released amounts, timestamps |
| `Milestone`     | `["milestone", agreement, index_le]` | index, bump, state, agreement, amount, deliverable hash, timestamps                                                  |
| Vault authority | `["vault-authority", agreement]`     | PDA only; signs vault transfers via CPI                                                                              |
| Vault           | ATA of `(vault authority, mint)`     | SPL token account holding funded milestones                                                                          |

`Agreement` is 185 bytes and `Milestone` 108 bytes including the 8-byte Anchor
discriminator. The SDK asserts both sizes so a layout change cannot slip through
silently.

### Instructions

| Instruction                | Signer                    | Preconditions                                           | Effect                                                         |
| -------------------------- | ------------------------- | ------------------------------------------------------- | -------------------------------------------------------------- |
| `create_agreement`         | client                    | client ≠ freelancer, nonce unused                       | writes a `Draft` agreement with the terms hash                 |
| `create_milestone`         | client                    | draft, index == milestone_count, ≤ 12 total, amount > 0 | writes a `Pending` milestone and adds to the total             |
| `accept_agreement`         | freelancer                | draft, supplied hash equals stored hash                 | moves to `Active`, records `accepted_at`                       |
| `cancel_draft_agreement`   | client                    | draft only                                              | closes the draft; no funds exist yet                           |
| `fund_milestone`           | client                    | active, milestone `Pending`                             | transfers exactly `amount` into the vault and checks the delta |
| `submit_milestone`         | freelancer                | milestone `Funded`                                      | stores the deliverable hash, moves to `Submitted`              |
| `approve_and_release`      | client                    | milestone `Submitted`                                   | transfers exactly `amount` to the freelancer's token account   |
| `raise_dispute`            | client or freelancer      | milestone `Funded` or `Submitted`                       | moves to `Disputed`, freezing release                          |
| `mutual_cancel_and_refund` | client **and** freelancer | milestone `Funded`, `Submitted` or `Disputed`           | returns exactly `amount` to the client                         |

There is no instruction that pays a platform account, and none that lets a
single party move a disputed milestone.

### Invariants enforced by constraints

- **Mint binding.** Every token account is constrained to the agreement mint.
- **Destination binding.** `approve_and_release` requires the destination token
  account's authority to equal `agreement.freelancer`.
- **Vault binding.** Vault and vault authority are re-derived from the agreement
  and mint; a substituted vault fails the seeds/owner check.
- **One settlement per milestone.** `Released` and `Refunded` are terminal and
  guarded before the CPI.
- **Exact amounts.** Funding verifies the vault balance delta; release and refund
  use the amount recorded at funding time, not a caller-supplied number.
- **No arithmetic wrap.** Checked adds/subs throughout; the release profile also
  enables `overflow-checks`.
- **Terms binding.** Acceptance requires the same 32-byte hash committed at
  creation.

### Events

Every state transition emits an Anchor event: `AgreementCreated`,
`MilestoneCreated`, `AgreementAccepted`, `AgreementCancelled`, `MilestoneFunded`,
`MilestoneSubmitted`, `MilestoneReleased`, `MilestoneDisputed`,
`MilestoneRefunded`. The API records them as an audit trail; it never uses an
event as proof of settlement without the account snapshot agreeing.

## 2. SDK

Location: `packages/sdk`

The SDK is intentionally dependency-light: `@solana/kit` only. It contains no
code generation step.

- `instructions.ts` — instruction builders with the exact account order and
  Borsh payloads.
- `accounts.ts` — decoders for `Agreement`, `Milestone` and RPC account data.
- `discriminators.ts` — hardcoded Anchor discriminators, re-derived and checked
  by `scripts/check-idl.mjs` and the unit tests.
- `terms.ts` — canonical terms JSON and SHA-256, plus deliverable hashing.
- `program.ts` — program ID, seeds, PDA helpers and size constants.
- `errors.ts` — program error code names.

Canonical terms are a single-line JSON object with fixed key order and decimal
string amounts, versioned as `1`. The Rust API implements the identical encoding;
a cross-language fixture pins the hash so the two implementations cannot drift.

## 3. Web app

Location: `apps/web` · Next.js 16 App Router · React 19

### Wallet integration

`src/lib/wallet.ts` speaks the Wallet Standard that wallets already register. No
per-wallet adapter packages are bundled. A transaction is compiled with Kit,
encoded to wire bytes, handed to `solana:signTransaction`, then submitted as
base64. Sign-in uses `solana:signMessage` over the SIWS text, which cannot move
funds.

### Chain layer

`src/lib/chain.ts` builds and submits wire transactions. Two details matter:

- the compiled transaction includes a zeroed 64-byte signature slot for the fee
  payer, because wallets expect a fully-formed transaction, not a message;
- co-signing rebuilds the same wire transaction and asks a second wallet to add
  its signature without becoming the fee payer.

### Terms sharing without a server

The URL fragment of an agreement link carries the exact canonical terms as
base64url (`#terms=…`). The page recomputes `sha256` and compares it with the
on-chain `termsHash` before enabling "accept". Fragments are never sent to a
server, so the terms stay out of request logs. Whoever holds the link can read
the terms; this is not a confidentiality guarantee.

### Degradation

`src/lib/api.ts` probes `/api/proxy/health/live`. If the API is absent, the app
shows a chain-only badge and skips indexing. Every value movement still works.

## 4. API

Location: `apps/api` · Axum 0.8 · SQLx · Postgres 16

### Authentication

Sign-In-With-Solana: `POST /v1/auth/challenge` stores a nonce, the wallet signs
the exact SIWS message, `POST /v1/auth/verify` verifies the ed25519 signature,
consumes the challenge and sets an HttpOnly session cookie. Unsafe methods also
require an `Origin` on the allowlist and the `x-openwork-csrf` token from
`GET /v1/me`. Session tokens are stored hashed.

### Metadata only

The API stores titles, descriptions, milestone metadata, deliverable claims and
quotes. It may not release, refund or hold funds: it has no keypair and no
instruction that could move value.

### Reconciliation

A settlement claim (`POST /v1/settlements`) is stored as `pending` and enqueued in
`outbox_jobs`. The worker:

1. leases jobs with `FOR UPDATE SKIP LOCKED`;
2. calls `getSignatureStatuses` on the claimed signature;
3. fetches the transaction, parses program logs into Anchor events, and decodes
   the agreement and milestone accounts;
4. validates the event kind and actor against the claim;
5. writes `chain_events` (deduplicated by `(chain, signature, event_index)`) and
   updates the `chain_*` snapshot columns.

A claim that never confirms stays `pending`; it is never promoted by assumption.

### Idempotency

Unsafe requests may carry `Idempotency-Key`. The API stores a hash of the request
with the response and replays it for the same key and body. A different body with
the same key is a conflict.

### Cross-chain adapters

`crates/aurora-adapter` exposes one trait with two implementations:

- `MockIntentAdapter` — deterministic, offline, every quote flagged
  `simulated: true`, status stops at `escrow_funding_pending`.
- `OneClickAdapter` — real NEAR 1Click client, compiled but only reachable when
  the deployment is configured for Mainnet with an API key. There is no runtime
  fallback between the two, because a silent fallback would hand a Mainnet user a
  simulated deposit address.

## 5. Data model

Postgres tables: `users`, `login_challenges`, `wallet_sessions`, `contracts`,
`milestones`, `intent_quotes`, `intent_transfers`, `onchain_transactions`,
`chain_events`, `outbox_jobs`, `idempotency_keys`, `audit_events`,
`worker_checkpoints`. Amounts are decimal text, never floats. Every chain-bound
row carries its cluster so Devnet and Mainnet data cannot mix.

`onchain_transactions.verification_status` uses `unverified`, `verified`,
`failed` and `not_found`. Only the worker moves a row out of `unverified`, and it
does so from a confirmed `getTransaction` response, never from a client claim.

## 6. Trust boundaries

| Component         | Can it move funds?                  | Can it lie?                                                          |
| ----------------- | ----------------------------------- | -------------------------------------------------------------------- |
| Wallet            | Yes, only with the user's signature | No; the signature is the authorization                               |
| Browser           | No                                  | It can show wrong copy; the wallet prompt shows the real transaction |
| RPC               | No                                  | Yes, by withholding or serving stale data; a different RPC fixes it  |
| API               | No                                  | It can mislabel metadata, which reconciliation overwrites            |
| Program           | Yes, within its rules               | No; the rules are the deployed bytecode                              |
| Upgrade authority | Yes, by replacing the program       | Yes; this is the disclosed centralization risk                       |

## 7. Deliberate omissions

- No arbitration and no admin dispute resolution (see [security.md](security.md)).
- No platform fee, therefore no treasury and no fee accounting.
- No upgrade timelock. Mainnet requires moving the upgrade authority to a
  multisig first.
- No on-chain terms storage: only a hash, to keep accounts small and cheap.
- No confidential terms: a URL fragment is a sharing mechanism, not encryption.
