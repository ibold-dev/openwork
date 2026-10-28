# User flows

Two people use OpenWork: a **client** who pays and a **freelancer** who delivers.
There is no account, no email and no password. A wallet is the account.

## 1. Create an agreement (client)

`/app/new` is a three-step wizard:

1. **Terms** — counterparty wallet, settlement mint, title, description.
2. **Milestones** — between 1 and 12 rows, each with a title, description and
   amount. Amounts are converted to base units with the mint's decimals; the UI
   never uses floating point for money.
3. **Review** — the canonical terms are shown, hashed, and the client signs one
   transaction that creates the agreement and its milestones.

The wizard derives the agreement PDA from `(client, nonce)` locally, where the
nonce is generated per draft. If the API is reachable and there is no session
yet, the wizard first asks for a SIWS message signature. That is optional: it is
not a transaction and refusing it only means the agreement is not listed in the
index. The same nonce and hash are then sent to the indexer, so both agree on the
same address. If the indexer is offline, the agreement still exists on chain.

After creation the client gets a share link:

```
/app/agreements/<agreement>#terms=<base64url(canonical terms)>
```

The fragment is the terms transport. It never reaches a server, and the page
recomputes `sha256` and compares it with the on-chain `termsHash` before it will
enable acceptance. A stripped or edited fragment disables the accept button
instead of silently accepting different terms.

## 2. Accept (freelancer)

The freelancer opens the link, connects a wallet, and sees:

- the full terms, read from the fragment and verified against the chain;
- the agreement state and every milestone;
- an **Accept terms** button, enabled only when the connected wallet is the
  recorded freelancer and the hash matches.

Acceptance is a single signed transaction (`accept_agreement`) carrying the same
32-byte hash. A different hash is rejected by the program.

If the link has no fragment, the page says so and cannot enable acceptance; there
is no "accept anyway" path.

## 3. Fund (client)

Each milestone is funded separately.

1. The client opens the agreement and presses **Fund milestone N**.
2. The wallet simulates, then signs `fund_milestone`.
3. Tokens move from the client's associated token account into the agreement
   vault; the program checks the vault balance delta equals the milestone amount.

Funding is idempotent from the user's perspective: a second attempt fails at
simulation because the milestone is no longer `Pending`. The UI surfaces the
program error rather than pretending it worked.

The client must already hold the settlement mint. On Devnet that is the test mint
`tUSDC`; there is no faucet in the app.

## 4. Submit (freelancer)

The freelancer presses **Submit milestone**, enters a deliverable URI and an
optional note, and signs `submit_milestone`. The browser hashes a canonical
deliverable object (agreement, index, URI, note) and stores only the hash on
chain. The URI and note are sent to the indexer when there is a session; the
chain proves _that_ a deliverable was committed, not _what_ it said.

Submitting does not move money and does not imply approval.

## 5. Release (client)

The client presses **Confirm release** and signs `approve_and_release`. The
program:

- checks the milestone is `Submitted`;
- checks the destination token account's authority is the freelancer;
- transfers exactly the recorded amount;
- marks the milestone `Released` and increments the agreement's released total.

There is no partial release in v1. If the work is incomplete, the client does not
release; the parties can agree a mutual refund instead.

## 6. Dispute and mutual refund

Either party can press **Raise dispute** on a funded or submitted milestone. That
freezes `approve_and_release` permanently for that milestone. There is no
administrator to appeal to.

The only exit is a **mutual refund**, which needs both signatures. Because a
browser cannot make two wallets sign one transaction at once, OpenWork uses a
co-signature handoff:

1. The initiator presses **Add my signature**. The wire transaction is signed
   with a zeroed fee-payer slot and encoded into a link:
   `/app/agreements/<agreement>#cosign=<base64url(wire tx)>`.
2. The other party opens the link, reviews the refund, adds their signature and
   submits. They pay the network fee.
3. If a wallet refuses to sign a transaction where it is not the fee payer, the
   UI says so instead of pretending the handoff succeeded. In that case the
   parties can use a wallet that supports co-signing, or a script against the
   published program.

A refund returns the exact milestone amount to the client's token account and
marks the milestone `Refunded`.

## 7. Dashboard

`/app` lists agreements the connected wallet is party to.

- With the API running, **Sync titles** starts a SIWS session and the page adds
  the agreements that wallet is a party to from the index, including ones first
  opened on another device. Indexed titles are shown when present.
- The page still reads every agreement account from the chain. A stale index
  cannot change a balance, a milestone state or a role; the chain wins.
- Without a session or without the API, the page still works from chain state:
  paste or open an agreement address and read its accounts directly. Nothing
  about funding or releasing depends on the index.
- With a session and an indexed agreement, each confirmed action claims its
  signature with `POST /v1/settlements`, and a submitted deliverable also posts
  its URI and note. Each claim stays `unverified` until the worker re-reads the
  transaction; a failed claim never becomes a chain fact.

## 8. Cross-chain funding

`/app/fund` is a **simulation** on this deployment.

1. The client signs in to the indexer (message signature, not a transaction).
2. Chooses an origin asset and amount, and requests a quote.
3. The mock adapter returns a quote-shaped response flagged `simulated: true`,
   with a deposit address explicitly described as not real.
4. Status stops at `escrow_funding_pending`; the mock never claims value arrived.

The page states all of this in the UI, not only in the docs.

Even with the real adapter, the flow is **two steps**: routed value lands in the
client's own wallet, then the client signs a separate `fund_milestone`. OpenWork
never routes value to a third party, and the API rejects a quote whose recipient
is not the authenticated wallet.

Not verified in this build:

- a real 1Click quote (no partner credentials);
- an Aurora-origin route (no `aurora` chain in the 1Click token catalogue);
- any Mainnet settlement.

## 9. Failure behavior

| Situation                              | What the UI does                                        |
| -------------------------------------- | ------------------------------------------------------- |
| Wallet rejects the signature           | No transaction is sent; the error is shown verbatim     |
| Simulation fails                       | The program error is shown and nothing is submitted     |
| API unreachable                        | Switches to chain-only mode; value movements still work |
| RPC unreachable                        | Error with a retry; no optimistic state change          |
| Indexer out of sync                    | Chain account state wins; the badge shows index in use  |
| Link without terms fragment            | Accept is disabled and the page explains why            |
| Wallet cannot co-sign as non-fee-payer | Explained; no false success message                     |
