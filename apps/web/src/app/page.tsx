import Link from 'next/link';
import type { Metadata } from 'next';

import { LedgerVisual } from '@/components/ledger-visual';
import { Reveal } from '@/components/reveal';
import { CLUSTER, PLATFORM_FEE_BPS } from '@/lib/config';

export const metadata: Metadata = {
  title: 'OpenWork — milestone escrow for independent work',
  description:
    'Client funds sit in a Solana program, not in OpenWork. Each milestone is released on approval; a disputed milestone can only move by mutual refund.',
};

const STEPS = [
  {
    num: '01',
    title: 'Write the terms',
    body: 'Both wallets, the settlement mint and every milestone amount are hashed into the agreement. The 32-byte hash is the only thing the program stores about your words, so nothing can be edited after the fact.',
  },
  {
    num: '02',
    title: 'Fund one milestone',
    body: 'The client moves the first milestone into a token account owned by the agreement’s vault authority. OpenWork cannot move it — the program only releases it to the freelancer named in the agreement.',
  },
  {
    num: '03',
    title: 'Approve, get paid',
    body: 'The freelancer submits a deliverable hash, the client approves, and the vault pays the exact milestone amount. No invoice chase, no platform balance to withdraw from.',
  },
];

const INVARIANTS = [
  {
    label: 'Vault authority',
    value: 'PDA, no key',
    detail:
      'The vault is owned by a program-derived address. There is no private key to steal and no admin function to drain it.',
  },
  {
    label: 'Platform fee',
    value: '0 bps',
    detail:
      'The program has no fee instruction. Making the fee zero is a structural decision, not a setting.',
  },
  {
    label: 'Dispute exit',
    value: '2 signatures',
    detail:
      'A dispute freezes release. The only way funds move again is a refund signed by both parties.',
  },
  {
    label: 'Custody',
    value: 'None',
    detail:
      'OpenWork never holds a key, a balance or a signed transaction. The browser talks straight to the RPC.',
  },
];

export default function HomePage() {
  return (
    <>
      <section className="hero">
        <div className="shell hero-grid">
          <div className="hero-copy">
            <p className="eyebrow">Milestone escrow · {CLUSTER}</p>

            <h1 className="display">
              Work gets paid
              <br />
              <span className="gradient-text">when it lands.</span>
            </h1>

            <p className="lede">
              OpenWork holds the client’s money in a Solana program and releases it one
              milestone at a time. Neither side has to trust the other, and neither side
              has to trust us.
            </p>

            <div className="hero-actions">
              <Link href="/app/new" className="btn btn-lg">
                Start an agreement
              </Link>
              <Link href="/app" className="btn btn-secondary btn-lg">
                Open the ledger
              </Link>
            </div>

            <div className="hero-meta">
              <span>
                <span className="pill-dot" style={{ background: 'var(--good)' }} />
                Program live on {CLUSTER}
              </span>
              <span>{PLATFORM_FEE_BPS / 100}% platform fee</span>
              <span>No admin keys</span>
              <span>Non-custodial</span>
            </div>
          </div>

          <Reveal y={22}>
            <LedgerVisual />
          </Reveal>
        </div>
      </section>

      <section className="shell section-tight" aria-label="Escrow guarantees">
        <div className="stat-row">
          {INVARIANTS.map((item) => (
            <div className="stat" key={item.label}>
              <span className="stat-label">{item.label}</span>
              <span className="stat-value">{item.value}</span>
              <span className="help">{item.detail}</span>
            </div>
          ))}
        </div>
      </section>

      <section id="how" className="section rule-top">
        <div className="shell">
          <Reveal>
            <div className="split" style={{ marginBottom: '2.5rem' }}>
              <div className="stack">
                <p className="eyebrow">How it works</p>
                <h2 className="h2">Three moves. The chain keeps the receipts.</h2>
              </div>
              <p className="lede">
                Every state change is an instruction with a named signer. There is no
                support inbox that can reverse a release, because there is no function
                that could.
              </p>
            </div>
          </Reveal>

          <div className="grid-3">
            {STEPS.map((step, index) => (
              <Reveal key={step.num} delay={index * 0.08}>
                <article className="card step">
                  <span className="step-num">{step.num}</span>
                  <h3>{step.title}</h3>
                  <p>{step.body}</p>
                </article>
              </Reveal>
            ))}
          </div>
        </div>
      </section>

      <section className="section rule-top">
        <div className="shell split">
          <Reveal>
            <div className="stack">
              <p className="eyebrow">The money path</p>
              <h2 className="h2">
                Funds move once, in one direction, with one signature.
              </h2>
              <p className="lede">
                Funding is a transfer into the agreement vault. Release is a signed
                transfer out to the freelancer’s token account, for the exact amount
                committed at creation. Refunds are the mirror image and require both
                parties.
              </p>
              <p className="help measure">
                The client can never be paid by the escrow, and the freelancer can never
                withdraw more than the milestone amount. Both constraints are account
                constraints in the program, not validation in our UI.
              </p>
            </div>
          </Reveal>

          <Reveal delay={0.1}>
            <div className="card card-pad stack">
              <p className="eyebrow" style={{ marginBottom: '0.25rem' }}>
                Release instruction
              </p>
              <pre className="code" aria-label="Approve and release accounts">
                {`approve_and_release
  client               signer, writable
  agreement            writable
  milestone            writable
  freelancer           readonly
  mint                 readonly
  freelancer_token     writable   ← authority = freelancer
  vault                writable   ← authority = vault PDA
  vault_authority      readonly
  token_program        readonly`}
              </pre>
              <p className="help">
                The destination token account is constrained to be owned by the freelancer
                recorded in the agreement. Paying anyone else aborts the transaction.
              </p>
            </div>
          </Reveal>
        </div>
      </section>

      <section id="fees" className="section rule-top">
        <div className="shell">
          <Reveal>
            <div className="split split-reverse">
              <div className="card card-pad">
                <table className="table">
                  <thead>
                    <tr>
                      <th>Cost</th>
                      <th className="num">Amount</th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr>
                      <td>OpenWork platform fee</td>
                      <td className="num gradient-text">0.00%</td>
                    </tr>
                    <tr>
                      <td>Escrow creation</td>
                      <td className="num">network fee only</td>
                    </tr>
                    <tr>
                      <td>Milestone funding</td>
                      <td className="num">network fee only</td>
                    </tr>
                    <tr>
                      <td>Release or refund</td>
                      <td className="num">network fee only</td>
                    </tr>
                  </tbody>
                </table>
                <p className="help" style={{ marginTop: '1rem' }}>
                  Rent for the agreement and milestone accounts is returned when the
                  accounts are closed. Until then it stays locked in the program, not with
                  OpenWork.
                </p>
              </div>

              <div className="stack">
                <p className="eyebrow">Fees</p>
                <h2 className="h2">
                  A fee is a feature you can remove. So we removed it.
                </h2>
                <p className="lede">
                  The escrow program has nine instructions and none of them transfer value
                  to a platform account. That keeps the security story short: there is no
                  treasury to compromise.
                </p>
                <p className="help measure">
                  Cross-chain funding adds a bridge provider’s spread and fees. OpenWork
                  does not take a cut of those either.
                </p>
              </div>
            </div>
          </Reveal>
        </div>
      </section>

      <section className="section rule-top">
        <div className="shell">
          <Reveal>
            <div className="split">
              <div className="stack">
                <p className="eyebrow">Cross-chain funding</p>
                <h2 className="h2">Fund from another chain without leaving the page.</h2>
                <p className="lede">
                  A client can swap into the settlement mint with a two-step deposit flow:
                  request a quote, send the origin asset, and the provider settles into
                  the client’s wallet. The client then funds the milestone from their own
                  wallet — OpenWork never holds the bridged funds.
                </p>
                <div className="callout callout-warn">
                  <strong className="small">Devnet simulation</strong>
                  <span className="help">
                    The demo runs a mock intent adapter. Quote and status shapes match the
                    1Click API, but no bridge is involved and no funds move between
                    chains. The real adapter is compiled behind a mainnet-only flag and
                    refuses to start in mock mode.
                  </span>
                </div>
              </div>

              <div className="card card-pad stack">
                <div className="cluster" style={{ justifyContent: 'space-between' }}>
                  <span className="mono small">POST /v0/quote</span>
                  <span className="pill pill-warn">
                    <span className="pill-dot" />
                    simulation
                  </span>
                </div>
                <pre className="code">
                  {`{
  "dry": true,
  "swapType": "EXACT_INPUT",
  "originAsset": "nep141:base-eth.omft.near",
  "destinationAsset": "sol",
  "amount": "50000000",
  "recipient": "4Nd1…DB4T",
  "recipientType": "DESTINATION_CHAIN",
  "refundTo": "4Nd1…DB4T",
  "depositType": "ORIGIN_CHAIN",
  "slippageTolerance": 100
}`}
                </pre>
                <p className="help">
                  Quote responses are persisted with a `simulated` flag so no screen can
                  present a simulated deposit address as a real one.
                </p>
                <Link href="/app/fund" className="btn btn-secondary">
                  Try the funding flow
                </Link>
              </div>
            </div>
          </Reveal>
        </div>
      </section>

      <section className="section rule-top">
        <div className="shell">
          <div className="card card-pad split" style={{ alignItems: 'center' }}>
            <div className="stack">
              <p className="eyebrow">Start today</p>
              <h2 className="h2">
                Your next client can fund the first milestone in two minutes.
              </h2>
              <p className="help measure">
                You need a Solana wallet and a token balance. No account, no KYC, no
                deposit into an OpenWork balance.
              </p>
            </div>
            <div className="cluster" style={{ justifyContent: 'flex-start' }}>
              <Link href="/app/new" className="btn btn-lg">
                Create an agreement
              </Link>
              <Link href="/security" className="btn btn-secondary btn-lg">
                Read the security model
              </Link>
            </div>
          </div>
        </div>
      </section>
    </>
  );
}
