# Deployments

Every value on this page was produced by a command in this repository against
Solana **Devnet**. Nothing here is Mainnet, and none of the tokens have value.

## Program

| Item              | Value                                                                                      |
| ----------------- | ------------------------------------------------------------------------------------------ |
| Cluster           | Devnet (`https://api.devnet.solana.com`)                                                   |
| Program ID        | `GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR`                                              |
| Deploy signature  | `3dx8kbwmFth8DVNgUuqcj38D8QSHARyb48DVxyL5v2Uc8odKAvvBvPyHb5QGm5oQGAxA3NSaLGjSNXofw7nx4u6F` |
| Upgrade authority | `5ok8FfjBgwDyfAFWYPx3yQ3Ty7s962XPthsKypNVgTup` (CLI default keypair, **not** a multisig)   |
| Source            | `programs/openwork-escrow`                                                                 |
| Binary hash       | see `anchor build` output; the `.so` is reproducible from the pinned toolchain             |

Explorer: <https://explorer.solana.com/address/GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR?cluster=devnet>

The upgrade authority is an honest single key. Moving it to a multisig is a
prerequisite for any Mainnet deployment; see [deployment.md](./deployment.md).

## Settlement mint

| Item             | Value                                          |
| ---------------- | ---------------------------------------------- |
| Mint             | `ENmQKqTu9FaWJtmwk3nXiNT3GxX6F4k7rZe8t2bqUYT8` |
| Symbol in the UI | `tUSDC`                                        |
| Decimals         | 6                                              |
| Supply           | 1,000.000000                                   |
| Mint authority   | `5ok8FfjBgwDyfAFWYPx3yQ3Ty7s962XPthsKypNVgTup` |
| Freeze authority | none                                           |

This is a plain SPL mint created by the smoke run. It is **not** Circle-issued
USDC and is not redeemable for anything.

## End-to-end smoke run

`packages/sdk/tests/devnet-smoke.integration.test.ts` creates a fresh agreement,
funds a milestone, submits a deliverable and releases the payment. The receipt
is committed at [`docs/devnet-smoke.json`](./devnet-smoke.json).

| Step                  | Signature                                                                                  |
| --------------------- | ------------------------------------------------------------------------------------------ |
| Fund freelancer (SOL) | `3N9azSuhArroe5qfX4pfappwxvXEytMEvYu6fqtEXgcduAk1uEQ42FiZ7KapxxRzKFxfVkt4w6RBuhcqtPSBzReZ` |
| Create mint           | `5Nfwh4wSHLdJrm6iNEqb51Y99GAssMYCkGLxeyPHpvGsxFXqz9hDFNB4GoFDEKjdPaWrxwqAg9PRGtscMZt7TNMy` |
| Create token accounts | `2DXSEaT4P723zXzQ2WSnMnjEGvcQic7iaSCnYBXwWSUyXkpXttTGTVnzhyvwtnCxLrFL95NQCGGm9RjLKchzk1TR` |
| Mint test tokens      | `5btfvDF3m828EizGB3uoG3LPLURv9UNKxxWc4Qv24tgzWKQp25AGS1NryJBrYeP8SUu8REUQRYrzoz89Y9qUNKcQ` |
| `create_agreement`    | `5bJq56osM2NtLExNbttc4JQz8rKvR6AJuPBepEJSRDVqY8gc2yNU2j97Hy4V8gdbwcPuaSAUtkAVfiVKksHrGPsv` |
| `create_milestone`    | `4kqsJkhFmto5cZ23Dbc6dD5pLTDSv9K2MsN8gXjQqWjRkyD7pBR7khwf2tccPiHP9VWubLzYCtjGJMUStSJPpPSp` |
| `accept_agreement`    | `2Bd2Tz81BxpsUTsLCxdbV4edWmVK7XYLMHLGubJGver8H4LjxqUc8TLZZpCoxfWMdGR5RhUQx1xGPo2QuEExWjSd` |
| `fund_milestone`      | `67KCeGw431UXZj4DL8RTJfGWvvZVhs4r28nAMMsghVAntC8xME7JXR29fb99doRW4GXRGSHgwydpHm2PPTw3RgeE` |
| `submit_milestone`    | `4JkqG4VEhwhAVcUyyyhJqTtHN3VYbS8APiKidfqujmLQdBmJVVKuyL9YDFTEL92LZUJJg3YPMTRGV1BMTQnrLnNC` |
| `approve_and_release` | `5KnvGAWpdsWSiT7dWpryhwpE2ZXXF7GMNy4vD5JTKDVzmeSmUkuk4DkqtopxDEo72CaDuZrD6NDdAe7Jire9GDH4` |

Accounts for that run:

| Account             | Address                                        |
| ------------------- | ---------------------------------------------- |
| Agreement PDA       | `32uv2z7tARSew89h1g7rGRv4KQNokJnWiPdrR8ghG6WK` |
| Milestone PDA       | `BX6Xq9Zaq5Wan19wgJn3T38ZjDCg1dfCFyJEGvS3uo4o` |
| Vault authority     | `BghR732henUwj8daPgXZggszggt7gHLuh9mMgJG4ngus` |
| Vault token account | `5m4qZCzNaZcguoD8fpSjUYunyKex6rusjU1roJdLYMrD` |
| Client              | `5ok8FfjBgwDyfAFWYPx3yQ3Ty7s962XPthsKypNVgTup` |
| Freelancer          | `2n7C96aUcA8gtPjnD47iR6hZns2EoJDx771pAwmP9XPu` |

The run asserted, on chain:

1. agreement state moved `draft → active`;
2. the milestone moved `pending → funded → submitted → released`;
3. the client's token balance fell by exactly `250.000000`;
4. the vault held exactly `250.000000` between funding and release, then `0`;
5. the freelancer's token balance rose by exactly `250.000000`;
6. `settledCount == 1` and `releasedAmount == 250.000000`.

## Reproducing

```sh
# Program (requires Anchor 1.1.2 and the pinned Rust toolchain)
anchor build
solana program deploy target/deploy/openwork_escrow.so \
  --program-id target/deploy/openwork_escrow-keypair.json -u devnet

# Smoke run (spends Devnet SOL; creates a new mint and agreement)
cd packages/sdk
OPENWORK_DEVNET_SMOKE=1 pnpm test:devnet
```

The smoke test reads the client keypair from
`OPENWORK_DEVNET_CLIENT_KEYPAIR` (default `~/.config/solana/id.json`), never logs
it, and writes `docs/devnet-smoke.json`. The freelancer is a fresh keypair
funded with a small SOL transfer, so no faucet is required.

## API reconciliation against Postgres

The metadata API was run against a throwaway PostgreSQL 17 cluster together with
the reconciliation worker:

1. migrations applied on startup (`/health/ready` reported `database: ok`, `rpc: ok`);
2. SIWS login with `5ok8FfjBgwDyfAFWYPx3yQ3Ty7s962XPthsKypNVgTup` succeeded;
3. the smoke agreement was indexed with the browser-supplied nonce and hash and
   derived the same PDA, `32uv2z7tARSew89h1g7rGRv4KQNokJnWiPdrR8ghG6WK`;
4. an `approve_and_release` claim for
   `5KnvGAWpdsWSiT7dWpryhwpE2ZXXF7GMNy4vD5JTKDVzmeSmUkuk4DkqtopxDEo72CaDuZrD6NDdAe7Jire9GDH4`
   moved from `unverified` to `verified` at slot `516054375`;
5. the worker recorded `MilestoneReleased` plus the four earlier lifecycle events
   and populated `contracts.chain_state = completed`,
   `chain_settled_count = 1`, `chain_released_amount = 250000000`.

That run also exposed and fixed a deserialization bug: `TransactionMeta` was
missing `#[serde(rename_all = "camelCase")]`, so `logMessages` never arrived and
every claim failed as `no_events_for_contract`. A regression test now decodes a
real `getTransaction` meta shape.

## Local integration environment

`scripts/test-integration.sh` starts `solana-test-validator`, loads the compiled
program and SPL Token, and runs the 13-test lifecycle suite. Those tests never
touch Devnet or Mainnet.
