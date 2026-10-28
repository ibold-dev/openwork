# Security policy

OpenWork escrows funds on Solana. If you believe you have found a way to move
escrowed value without the signatures the program requires, or to bypass a state
guard, report it privately.

**Do not open a public issue.**

## Reporting

Use GitHub private vulnerability reporting:
**Security → Report a vulnerability** on this repository.

Please include:

- the affected component (program, SDK encoder, web transaction builder, API),
- the commit or deployed program ID,
- a minimal reproduction: instruction, accounts, transaction or test,
- impact: loss of funds, authorization bypass, denial of service, privacy,
- whether the issue reproduces on the Devnet program
  `GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR`.

## Scope

In scope:

- the escrow program (`programs/openwork-escrow`),
- the SDK's instruction encoders, decoders and terms hashing,
- the web app's transaction composition and terms verification,
- the API's authentication, authorization and reconciliation logic.

Out of scope:

- the Devnet keypair published in `docs/deployments.md` (it is a public test key),
- the test mint (`tUSDC`) having no value,
- phishing or wallet-extension compromise,
- denial of service through a public RPC,
- the known limitations listed in [`docs/security.md`](docs/security.md), unless
  you have found a way to exceed what is disclosed there.

## What to expect

- Acknowledgement of the report.
- An assessment against the invariants in `docs/security.md`.
- A fix and a coordinated disclosure date if the report is valid.

There is no bounty program. Good-faith research that respects this policy will
not be pursued legally.

## Safe harbor

Do not test against Mainnet, do not touch other users' agreements, and do not
exfiltrate data. Use a local validator or Devnet with your own accounts.
