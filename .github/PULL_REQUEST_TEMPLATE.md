## What changed

<!-- One paragraph. Link the issue if there is one. -->

## Why

## Verification

<!-- Paste the commands you ran, not just "tests pass". -->

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `pnpm -r typecheck`
- [ ] `pnpm -r build`
- [ ] `node scripts/check-idl.mjs`
- [ ] `./scripts/test-integration.sh` (if the program or SDK changed)

## Funds-safety checklist

- [ ] No new instruction can move escrowed value without the party the program
      already requires.
- [ ] No admin key, fee path or upgrade bypass was added.
- [ ] Program account layout changes include a migration plan (v1 has no
      versioned migration; a layout change means a new deployment).
- [ ] Public copy still distinguishes Devnet simulation from real routing.
