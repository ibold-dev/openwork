# Deployment

Two environments are supported: **localnet** for development and **Devnet** for
the public demo. Mainnet is _documented but not enabled_; the checklist at the
end is a gate, not a formality.

## Prerequisites

| Tool       | Version | Why                             |
| ---------- | ------- | ------------------------------- |
| Rust       | 1.89.0  | pinned in `rust-toolchain.toml` |
| Solana CLI | 2.3.x   | deploy, airdrop, validator      |
| Anchor CLI | 1.1.2   | build and IDL                   |
| Node       | ≥ 20.18 | web and SDK                     |
| pnpm       | 10.27   | workspace                       |
| Postgres   | 16      | metadata API                    |

> The Solana CLI defaults to **Mainnet**. Always pass `-u devnet` or set
> `solana config set -u devnet` before any account command.

## 1. Program

```sh
anchor build
anchor test                     # optional: LiteSVM program tests
solana program deploy target/deploy/openwork_escrow.so \
  --program-id target/deploy/openwork_escrow-keypair.json \
  -u devnet
```

The program keypair is generated once by `anchor keys sync` and is **gitignored**.
Losing it means deploying to a new program ID and re-indexing agreements; it is
not a fund-loss event, because vaults are derived from the program ID.

Record the program ID in `Anchor.toml`, `apps/web/src/lib/config.ts` and
`apps/api/.env`.

### Upgrade authority

`anchor build` deploys an upgradeable program owned by the deploying key. The
current Devnet authority is disclosed in [deployments.md](deployments.md).

For anything beyond a demo:

```sh
# Move the authority to a multisig you control.
solana program set-upgrade-authority <PROGRAM_ID> \
  --new-upgrade-authority <MULTISIG> -u <cluster>
```

There is no timelock in v1. A multisig without a published upgrade policy is only
marginally better than a single key.

## 2. Settlement mint

Devnet uses a test SPL mint created by the smoke run. To create a fresh one:

```sh
spl-token create-token --decimals 6 -u devnet
spl-token create-account <MINT> -u devnet
spl-token mint <MINT> 1000 -u devnet
```

Set `NEXT_PUBLIC_DEFAULT_MINT` and `NEXT_PUBLIC_DEFAULT_MINT_SYMBOL` in
`apps/web/.env.local`. **Never call a test mint USDC** in UI copy; the app labels
it `tUSDC` for this reason.

On Mainnet the mint must be a canonical, allowlisted token. The program does not
whitelist mints itself: the client chooses the mint at agreement creation, and
the UI decides which mints to offer. That is a product-level control, not an
on-chain one, and should be revisited before Mainnet.

## 3. Web app

### Local

```sh
cp apps/web/.env.example apps/web/.env.local
pnpm install
pnpm --filter @openwork/web dev
```

### Vercel or any Node host

- Build command: `pnpm --filter @openwork/web build`
- Output: Next.js (standalone is enabled; use the platform's Next preset)
- Environment: every variable in `apps/web/.env.example`
- `OPENWORK_API_URL` must point at the API; the Next server proxies
  `/api/proxy/*` to it so the session cookie stays first-party.

Public variables are inlined at **build** time. Changing
`NEXT_PUBLIC_SOLANA_RPC_URL` requires a rebuild.

### Docker

```sh
docker build -f infra/docker/web.Dockerfile \
  --build-arg OPENWORK_API_URL=https://api.example.com \
  -t openwork-web .
```

## 4. API and worker

The API requires Postgres. Migrations are embedded and run on startup.

```sh
createdb openwork
cp apps/api/.env.example apps/api/.env
# edit DATABASE_URL and origins
cargo run -p openwork-api --bin api
cargo run -p openwork-api --bin worker   # separate process
```

Production notes:

- Set `API_PUBLIC_ORIGIN` and `CORS_ALLOWED_ORIGINS` to the same HTTPS origin;
  the process refuses to start otherwise.
- `COOKIE_SECURE=1` behind HTTPS.
- Run at least one worker; without it, claims stay `pending`.
- The rate limiter is per-process. Put a shared limiter in front if you run more
  than one API replica.
- Health: `/health/live` (process) and `/health/ready` (database + RPC).

### Docker Compose

```sh
docker compose -f infra/compose/docker-compose.yml up --build
```

Brings up Postgres, API, worker and web on `localhost:3000` / `localhost:8080`,
configured for Devnet and the mock intents adapter.

## 5. Cross-chain adapter

`INTENTS_MODE=mock` is the only supported setting for this build. The real
adapter requires:

- `INTENTS_MODE=mainnet`
- `ONECLICK_API_KEY`
- a **Mainnet** cluster configuration

It refuses to serve quotes on Devnet, and there is no silent fallback. See
[cross-chain notes](user-flows.md#cross-chain-funding) for what was and was not
verified.

## 6. Mainnet gate

Do not deploy to Mainnet until every item is true:

- [ ] Independent audit of the program and of the SDK encoders.
- [ ] Upgrade authority is a multisig with a documented, timelocked policy.
- [ ] Canonical mint allowlisted in the UI and verified.
- [ ] Dispute policy decided (v1 freezes forever; is that acceptable?).
- [ ] Real cross-chain route verified with 1Click support and credentials.
- [ ] Monitoring for vault balances, failed settlements and stuck workers.
- [ ] Legal review of escrow and money-transmission obligations in each market.
- [ ] A rehearsed incident runbook, including how to pause _without_ an admin
      drain (v1 has no pause; freezing means stopping the frontend, not the
      program).

Until then, the honest deployment is Devnet, and the UI says so.

## 7. Reproducing the committed Devnet receipts

```sh
./scripts/devnet-smoke.sh
```

It creates a new mint, agreement and milestone, runs the full lifecycle and
rewrites `docs/devnet-smoke.json`. The client keypair comes from
`OPENWORK_DEVNET_CLIENT_KEYPAIR` (default `~/.config/solana/id.json`); the
freelancer is fresh and funded by transfer, so no faucet is needed.
