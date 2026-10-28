# Contributing

OpenWork moves other people's money. Contributions are judged first on whether
they keep the custody invariants intact, second on tests, third on style.

## Setup

```sh
pnpm install
anchor build
cp apps/api/.env.example apps/api/.env   # for the API
```

Toolchain versions are pinned: Rust 1.89.0 (`rust-toolchain.toml`), Anchor 1.1.2,
Node ≥ 20.18, pnpm 10.27.

## Before opening a pull request

```sh
make verify          # IDL drift, formatting, clippy, unit tests, typecheck
./scripts/test-integration.sh   # if the program or SDK changed
pnpm -r build
```

The Devnet smoke test is not part of the gate; run it only when you changed
something that affects real transactions, and never point it at Mainnet.

## Rules for program changes

- **No new custodian.** No instruction may let a single party, or OpenWork, move
  escrowed value.
- **No admin override.** Disputes are resolved by both signatures or not at all.
- **Exact amounts.** Never trust a caller-supplied amount for a transfer; use the
  amount recorded at funding.
- **Terminal states stay terminal.** A replay must not pay twice.
- **Layout changes need a plan.** The program has no in-place migration. Changing
  `Agreement` or `Milestone` means a new deployment, a new IDL and an SDK update
  in the same pull request.
- Update the IDL (`anchor build`) and `scripts/check-idl.mjs` expectations
  together. Drift fails CI.

## Rules for UI copy

- Say **simulated** where a quote is simulated, in the interface itself, not only
  in the docs.
- Never call the Devnet mint USDC.
- Never claim an audit, a partnership or a Mainnet deployment that has not
  happened.
- If a wallet cannot do something, say so; do not hide a failed signature behind
  a success state.

## Commits

Conventional commits, scoped by area:

```
feat(program): freeze release after a dispute
fix(web): verify the terms hash before enabling accept
docs(security): disclose the upgrade authority
test(sdk): cover wrong-destination release
```

Keep the program, SDK and web changes in separate commits when they can be
reviewed independently.

## Reporting bugs

Use the issue templates. For anything that could put funds at risk, follow
[SECURITY.md](SECURITY.md) instead of opening a public issue.
