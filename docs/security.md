# Security

OpenWork's promise is narrow on purpose: **the program holds the money, and no
single party — including OpenWork — can move it alone.**

This document describes what is enforced, what is not, and how to report a
problem. It is written to be read alongside
[`programs/openwork-escrow`](../programs/openwork-escrow), not instead of it.

## 1. Custody

- Funds are held in an SPL token account owned by a PDA derived from the
  agreement: `["vault-authority", agreement]`.
- The program is the only signer for that PDA. There is no private key to phish.
- The web app builds transactions locally and the wallet signs them. The API has
  no keypair and no signer code.
- OpenWork operates no deposit address and no treasury. The program contains no
  instruction that pays a platform account and no fee parameter.

## 2. Authorization matrix

| Transition                        | Required signature(s)     | Enforced by                                                                   |
| --------------------------------- | ------------------------- | ----------------------------------------------------------------------------- |
| Create agreement / add milestones | client                    | `has_one = client`                                                            |
| Accept terms                      | freelancer                | `has_one = freelancer` + hash equality                                        |
| Cancel a draft                    | client                    | `has_one = client`, state `Draft`                                             |
| Fund a milestone                  | client                    | `has_one = client`, state `Active`, milestone `Pending`                       |
| Submit a deliverable              | freelancer                | `has_one = freelancer`, milestone `Funded`                                    |
| Release payment                   | client                    | `has_one = client`, milestone `Submitted`, destination authority = freelancer |
| Raise a dispute                   | client or freelancer      | signer is one of the two parties, milestone `Funded`/`Submitted`              |
| Refund                            | client **and** freelancer | both `Signer` accounts, milestone not already terminal                        |

No instruction accepts an arbitrary recipient for value. `approve_and_release`
checks the destination token account's authority against the freelancer recorded
in the agreement, so redirecting a payment aborts the transaction.

## 3. Invariants and how they are tested

| Invariant                                      | Test coverage                                    |
| ---------------------------------------------- | ------------------------------------------------ |
| Agreement and milestone sizes match the SDK    | Rust layout tests + SDK size assertions          |
| PDA derivation is deterministic                | Rust + SDK fixture tests                         |
| Only the client can fund                       | Rust and validator tests with the wrong signer   |
| A foreign mint cannot fund or release          | Validator test with a second mint                |
| A release to the client's token account fails  | Validator test with a substituted destination    |
| A milestone cannot be funded or released twice | Rust + validator state tests                     |
| Terms hash mismatch blocks acceptance          | Rust + validator tests                           |
| A dispute freezes release                      | Validator test: release after dispute fails      |
| A refund needs both signatures                 | Validator test: client-only refund fails         |
| A refund returns the exact amount              | Validator test asserts client and vault balances |

Tests are in `programs/openwork-escrow/tests/escrow.rs` (16 tests, in-process
SVM), `packages/sdk/tests/lifecycle.integration.test.ts` (13 tests against a local
validator with SPL Token), and `apps/api/tests/api.rs` (HTTP and reconciliation).

## 4. Known limitations

These are real and are not hidden in marketing copy.

1. **No arbitration.** v1 cannot resolve a disagreement. A disputed milestone is
   frozen until both parties sign a refund. If one party refuses forever, the
   funds stay in the vault. That is worse for the aggrieved party than an
   arbitrator and better than an administrator who can steal; the trade-off is
   deliberate.
2. **Upgradeable program.** The Devnet program's upgrade authority is the single
   key `5ok8FfjBgwDyfAFWYPx3yQ3Ty7s962XPthsKypNVgTup`. Whoever holds it can
   replace the program logic. Any Mainnet deployment must move it to a multisig
   with a published policy and, ideally, a timelock.
3. **No audit.** No third party has reviewed this code. The test suite is public
   and the program is small, but neither is an audit.
4. **Terms are not confidential.** Only a hash is on chain. The full terms travel
   in a URL fragment; anyone with the link can read them.
5. **No on-chain title or description.** Off-chain metadata can be lost. The
   amount, parties, hash and state cannot.
6. **RPC trust.** A malicious RPC can withhold or delay transactions and can show
   stale account data. It cannot forge a signature. Pointing at another RPC
   restores service.
7. **Devnet mint.** `tUSDC` is a test mint with no value and a single mint
   authority. It is not USDC.
8. **Mock cross-chain adapter.** Devnet funding quotes are simulated. Nothing
   bridges, and the mock never reports that value arrived. The real adapter was
   not exercised against Mainnet in this build.
9. **Session security.** The API uses an HttpOnly cookie plus a CSRF header and
   an origin allowlist. A compromised browser or wallet extension is outside the
   threat model.
10. **No rate-limit guarantee across replicas.** The API's limiter is in-process;
    a multi-replica deployment needs a shared limiter.

## 5. Threat model

| Adversary                | Goal                               | Outcome                                                                       |
| ------------------------ | ---------------------------------- | ----------------------------------------------------------------------------- |
| Freelancer               | Take funds without client approval | Cannot sign `approve_and_release`; refund needs the client too                |
| Client                   | Avoid paying after delivery        | Can refuse to approve; freelancer's only lever is the frozen state, disclosed |
| Either party             | Move a disputed milestone alone    | No instruction permits it                                                     |
| OpenWork operator        | Drain a vault                      | No key, no admin instruction, no fee path                                     |
| Stolen session cookie    | Act as a user in the API           | Can alter metadata; cannot move funds, because signing happens in the wallet  |
| Malicious RPC            | Steal or alter a transaction       | Cannot forge the wallet signature; can only censor                            |
| Upgrade authority holder | Replace program logic              | Can steal. Disclosed; must be a multisig on Mainnet                           |
| Phishing site            | Get a user to sign a transfer      | Out of scope for the program; the wallet prompt shows the real instruction    |

## 6. Operational security

- Secrets are never committed. `.env` files, keypairs and deploy artefacts are
  gitignored.
- The API sets `X-Content-Type-Options`, `Referrer-Policy`, `X-Frame-Options` and
  a restrictive `Permissions-Policy` at the web edge.
- Session and challenge records are stored hashed/expiring; challenges are
  single-use.
- The Devnet keypair in `docs/deployments.md` is a public test key. Never send it
  value on Mainnet.

## 7. Reporting a vulnerability

Do **not** open a public issue for a suspected vulnerability.

Use GitHub private vulnerability reporting on this repository
(`Security → Report a vulnerability`). Include:

- affected component and commit,
- a minimal reproduction or transaction,
- impact (funds, authorization, availability, privacy),
- whether the issue affects the Devnet program currently deployed.

You will get an acknowledgement and a remediation plan. There is no bounty
program. Public disclosure before a fix is not authorized.

## 8. Before Mainnet

Not a checklist that has been completed — a checklist that has **not** been:

- [ ] Independent audit of the program and SDK encoders.
- [ ] Upgrade authority moved to a multisig with a timelock and published policy.
- [ ] Canonical mint allowlisted and verified (not a test mint).
- [ ] Fee and treasury decision made explicitly; v1's zero-fee stance is
      structural, so adding a fee is a program upgrade and a new audit surface.
- [ ] Arbitration or timeout policy decided; v1 has neither.
- [ ] Real cross-chain route verified end to end with partner support.
- [ ] Incident runbook and monitoring for vault balances and failed settlements.
- [ ] Legal review of escrow, money transmission and terms of service.
