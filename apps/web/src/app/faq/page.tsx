import type { Metadata } from 'next';
import Link from 'next/link';

import { CLUSTER, DEFAULT_MINT_SYMBOL, PROGRAM_ID } from '@/lib/config';

export const metadata: Metadata = {
  title: 'FAQ — OpenWork',
  description:
    'Answers about escrow custody, fees, disputes, cross-chain funding on Devnet and what happens if OpenWork disappears.',
};

const QUESTIONS = [
  {
    q: 'Who holds the money while a milestone is in progress?',
    a: 'A token account owned by a program-derived address (PDA) for that agreement. The escrow program is the only thing that can move value out of it. OpenWork has no key, no balance and no server-side signer.',
  },
  {
    q: 'What does OpenWork charge?',
    a: 'Nothing. The program has no fee instruction, so there is no way to take a cut even if we wanted to. You pay Solana network fees and account rent; rent is returned when accounts are closed.',
  },
  {
    q: 'What happens if the client disappears?',
    a: 'The freelancer can still submit work, but only the client can approve and release. If the client is unreachable, the two parties can agree a mutual refund, which needs both signatures. A milestone that is neither approved nor refunded stays in the vault — the program has no timeout that would let anyone take it.',
  },
  {
    q: 'What happens if the freelancer disappears?',
    a: 'The client can raise a dispute on any funded milestone, which freezes release. The client can also fund later milestones or simply stop. Refunding requires the freelancer to co-sign, so the money stays visible in the vault until both parties act.',
  },
  {
    q: 'Why is a dispute not resolvable by an administrator?',
    a: 'Because an administrator with the power to resolve disputes is an administrator with the power to steal. v1 chooses a weaker promise — a transparent freeze — over a stronger-sounding one that would require custody. The limitation is stated on the security page rather than hidden.',
  },
  {
    q: 'Is the cross-chain funding real?',
    a: 'Not on this deployment. Quotes come from a local mock adapter that mirrors the NEAR 1Click API shapes. Every quote is flagged simulated, the deposit address is explicitly marked as not real, and the mock never reports that value arrived. The real adapter exists behind a mainnet-only flag.',
  },
  {
    q: 'Why do I have to fund escrow in two steps?',
    a: 'Routed funds land in your own wallet. Funding a milestone is a second transaction you sign, moving the exact milestone amount into the agreement vault. This is deliberate: it means a bridge failure can never be mistaken for escrow funding.',
  },
  {
    q: `What token is used on ${CLUSTER}?`,
    a: `A test SPL mint created for this deployment, shown as ${DEFAULT_MINT_SYMBOL} in the UI. It is not Circle-issued USDC and has no value. On Mainnet the mint would be an allowlisted canonical token configured at deploy time.`,
  },
  {
    q: 'Do I need an account?',
    a: 'No. A Solana wallet is the account. The metadata API only stores titles and quotes so the dashboard can show them; the chain holds the authoritative state and the app works without the API for every value movement.',
  },
  {
    q: 'What happens if the OpenWork website is down?',
    a: 'Nothing is trapped. The agreement and vault are ordinary Solana accounts. Any wallet or script can build approve_and_release or mutual_cancel_and_refund against the deployed program. The program ID and instruction layouts are published in the repository.',
  },
  {
    q: 'Has the program been audited?',
    a: 'No. It has a test suite covering state transitions, authorization, wrong-signer, wrong-mint, wrong-destination, duplicate-release and refund paths, and it is small enough to review. That is not the same as an audit, and we do not claim one.',
  },
  {
    q: 'Can the program be upgraded?',
    a: 'Yes, in v1 the deploy authority can upgrade it. That is disclosed on the security page. Before any Mainnet deployment the authority should move to a multisig with a published upgrade policy.',
  },
];

export default function FaqPage() {
  return (
    <div className="shell section-tight">
      <div className="stack" style={{ maxWidth: '44rem' }}>
        <p className="eyebrow">FAQ</p>
        <h1 className="display" style={{ fontSize: 'clamp(2.4rem, 5vw, 3.4rem)' }}>
          Straight answers,
          <br />
          <span className="gradient-text">including the awkward ones.</span>
        </h1>
        <p className="lede">
          If a claim here is not enforced by the program, it says so.
        </p>
      </div>

      <div className="stack" style={{ marginTop: '2.5rem', maxWidth: '52rem' }}>
        {QUESTIONS.map((item, index) => (
          <details className="details" key={item.q}>
            <summary>
              <span className="mono tiny faint" style={{ marginRight: '0.75rem' }}>
                {String(index + 1).padStart(2, '0')}
              </span>
              {item.q}
            </summary>
            <div className="details-body">
              <p className="help" style={{ maxWidth: '62ch' }}>
                {item.a}
              </p>
            </div>
          </details>
        ))}
      </div>

      <div className="card card-pad split" style={{ marginTop: '3rem' }}>
        <div className="stack">
          <h2 className="h3">Read the code, not the copy</h2>
          <p className="help measure">
            The escrow program is {PROGRAM_ID.slice(0, 4)}…{PROGRAM_ID.slice(-4)} on{' '}
            {CLUSTER}. The repository contains the Anchor source, the instruction layouts
            and the tests that exercise every guard described here.
          </p>
        </div>
        <div className="cluster" style={{ justifyContent: 'flex-start' }}>
          <Link href="/security" className="btn btn-secondary">
            Security model
          </Link>
          <Link href="/app/new" className="btn">
            Start an agreement
          </Link>
        </div>
      </div>
    </div>
  );
}
