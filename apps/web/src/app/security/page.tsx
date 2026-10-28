import type { Metadata } from 'next';
import Link from 'next/link';

import { CLUSTER, PLATFORM_FEE_BPS, PROGRAM_ID } from '@/lib/config';
import { shortenAddress } from '@/lib/format';

export const metadata: Metadata = {
  title: 'Security model — OpenWork',
  description:
    'How OpenWork holds funds: a program-owned vault, no admin keys, no platform fee, and a dispute exit that requires both signatures.',
};

const INVARIANTS = [
  {
    id: 'custody',
    title: 'OpenWork never holds a key',
    body: 'The browser talks to an RPC and the wallet signs. There is no server-side signer, no deposit address controlled by OpenWork and no balance to withdraw. The optional metadata API indexes titles and quotes; it has no authority over the vault.',
  },
  {
    id: 'vault',
    title: 'The vault is owned by a PDA',
    body: 'Each agreement has a program-derived vault authority. Its associated token account can only be debited by the escrow program, through approve_and_release or mutual_cancel_and_refund. There is no private key for that authority, so there is nothing to phish.',
  },
  {
    id: 'fee',
    title: 'No platform fee, no treasury',
    body: 'The program has no instruction that transfers value to a platform account. A zero fee is structural: there is no admin drain function to compromise, because there is no fee path at all.',
  },
  {
    id: 'dispute',
    title: 'A dispute freezes release',
    body: 'Either party can raise a dispute on a funded or submitted milestone. That freezes approve_and_release permanently for that milestone. The only instruction that moves the money afterwards is a mutual refund signed by both client and freelancer.',
  },
  {
    id: 'amounts',
    title: 'Amounts come from the account',
    body: 'Funding transfers the exact milestone amount into the vault and verifies the balance delta. Release pays the exact recorded amount to a token account whose authority is constrained to the freelancer named in the agreement. A substituted destination aborts the transaction.',
  },
  {
    id: 'once',
    title: 'Every milestone settles once',
    body: 'Milestone state moves Pending → Funded → Submitted → Released, or Pending → Funded → Disputed → Refunded. Both terminal transitions are guarded, so a replay cannot pay twice.',
  },
];

export default function SecurityPage() {
  return (
    <div className="shell section-tight">
      <div className="stack" style={{ maxWidth: '46rem' }}>
        <p className="eyebrow">Security model · {CLUSTER}</p>
        <h1 className="display" style={{ fontSize: 'clamp(2.4rem, 5vw, 3.6rem)' }}>
          Trust the accounts,
          <br />
          <span className="gradient-text">not the company.</span>
        </h1>
        <p className="lede">
          Every guarantee on this page is enforced by an account constraint in the escrow
          program. If the OpenWork website disappeared tomorrow, both parties could still
          release or refund using any Solana wallet.
        </p>
      </div>

      <div className="stat-row" style={{ marginTop: '2.5rem' }}>
        <div className="stat">
          <span className="stat-label">Program</span>
          <span className="stat-value mono" style={{ fontSize: '0.9rem' }}>
            {shortenAddress(PROGRAM_ID, 6)}
          </span>
          <span className="help">Devnet deployment</span>
        </div>
        <div className="stat">
          <span className="stat-label">Platform fee</span>
          <span className="stat-value">{(PLATFORM_FEE_BPS / 100).toFixed(2)}%</span>
          <span className="help">No fee instruction exists</span>
        </div>
        <div className="stat">
          <span className="stat-label">Upgrade authority</span>
          <span className="stat-value">Disclosed</span>
          <span className="help">Program is upgradeable in v1</span>
        </div>
        <div className="stat">
          <span className="stat-label">Admin withdrawal</span>
          <span className="stat-value">None</span>
          <span className="help">Not implemented, not planned</span>
        </div>
      </div>

      <section id="invariants" className="section rule-top">
        <div className="grid-2">
          {INVARIANTS.map((item) => (
            <article className="card card-pad stack" key={item.id} id={item.id}>
              <h2 className="h3">{item.title}</h2>
              <p className="help">{item.body}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="section rule-top">
        <div className="split">
          <div className="stack">
            <p className="eyebrow">Disputes</p>
            <h2 className="h2">Frozen is not confiscated.</h2>
            <p className="lede">
              A dispute stops the clock. The money stays in the vault, visible to anyone,
              until the client and freelancer both sign a refund. Neither party can move
              it alone, and neither can OpenWork.
            </p>
            <p className="help measure">
              v1 has no arbitration. If the two parties cannot agree, the funds remain in
              escrow — that is the honest limitation, and it is why the refund requires
              two signatures rather than an administrator.
            </p>
            <Link href="/faq" className="btn btn-secondary">
              Read the FAQ
            </Link>
          </div>

          <div className="card card-pad stack">
            <p className="eyebrow">Trust boundaries</p>
            <div className="kv">
              <div className="kv-row">
                <span className="kv-key">Wallet</span>
                <span className="kv-val">Signs every value movement</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Browser</span>
                <span className="kv-val">Composes instructions locally</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Metadata API</span>
                <span className="kv-val">Index only, no signing</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Escrow program</span>
                <span className="kv-val">Final authority over funds</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">RPC provider</span>
                <span className="kv-val">Can censor, cannot steal</span>
              </div>
            </div>
            <p className="help">
              An RPC can refuse to relay a transaction; it cannot alter one, because the
              wallet signs the compiled message. A different RPC restores service.
            </p>
          </div>
        </div>
      </section>

      <section className="section rule-top">
        <div className="stack" style={{ maxWidth: '48rem' }}>
          <p className="eyebrow">Limits of this deployment</p>
          <h2 className="h2">What is not protected yet</h2>
          <ul
            className="help"
            style={{ paddingLeft: '1.1rem', display: 'grid', gap: '0.55rem' }}
          >
            <li>
              The program is upgradeable by the deploying authority. A upgrade could
              change behaviour; the authority is published and should be moved to a
              multisig before any Mainnet use.
            </li>
            <li>
              Cross-chain funding is a Devnet simulation. No bridge is running, so no
              bridged value exists to protect.
            </li>
            <li>
              Terms are stored as a hash plus a URL fragment. Whoever holds the fragment
              can read the terms; it is not a confidentiality guarantee.
            </li>
            <li>
              No independent audit has been performed. The test suite is public and the
              code is small enough to read, but that is not an audit.
            </li>
          </ul>
        </div>
      </section>
    </div>
  );
}
