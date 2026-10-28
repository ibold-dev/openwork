'use client';

import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useEffect, useMemo, useState } from 'react';

import { useWallet } from '@/components/wallet-provider';
import { createContract } from '@/lib/api';
import { buildDraftInstructions, fetchMintDecimals, sendInstructions } from '@/lib/chain';
import { DEFAULT_MINT, DEFAULT_MINT_SYMBOL, KNOWN_MINTS } from '@/lib/config';
import { formatAmount, parseAmount, shortenAddress } from '@/lib/format';
import {
  appendReceipt,
  nextAgreementNonce,
  saveTerms,
  trackAgreement,
} from '@/lib/storage';
import { MAX_MILESTONES, hashTerms, type TermsInput } from '@openwork/sdk';
import type { Address } from '@solana/kit';

interface MilestoneDraft {
  readonly key: string;
  title: string;
  description: string;
  amount: string;
}

type Step = 0 | 1 | 2;

const STEPS = ['Terms', 'Milestones', 'Review'] as const;

function emptyMilestone(index: number): MilestoneDraft {
  return {
    key: `m-${index}-${Math.random().toString(36).slice(2, 7)}`,
    title: '',
    description: '',
    amount: '',
  };
}

export default function NewAgreementPage() {
  const router = useRouter();
  const { account, wallet, open } = useWallet();

  const [step, setStep] = useState<Step>(0);
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [freelancer, setFreelancer] = useState('');
  const [mint, setMint] = useState<string>(DEFAULT_MINT);
  const [mintDecimals, setMintDecimals] = useState(
    KNOWN_MINTS[DEFAULT_MINT]?.decimals ?? 6,
  );
  const [milestones, setMilestones] = useState<MilestoneDraft[]>([emptyMilestone(0)]);
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [progress, setProgress] = useState<string | null>(null);

  const symbol = KNOWN_MINTS[mint]?.symbol ?? DEFAULT_MINT_SYMBOL;

  useEffect(() => {
    let cancelled = false;
    if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(mint)) return;
    void fetchMintDecimals(mint as Address)
      .then((decimals) => {
        if (!cancelled) setMintDecimals(decimals);
      })
      .catch(() => {
        if (!cancelled) setMintDecimals(6);
      });
    return () => {
      cancelled = true;
    };
  }, [mint]);

  const parsed = useMemo(() => {
    return milestones.map((milestone) => ({
      ...milestone,
      baseUnits: parseAmount(milestone.amount, mintDecimals),
    }));
  }, [milestones, mintDecimals]);

  const total = parsed.reduce((sum, milestone) => sum + (milestone.baseUnits ?? 0n), 0n);

  const termsProblem = useMemo(() => {
    if (title.trim().length < 4) return 'Give the agreement a title (4+ characters).';
    if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(freelancer)) {
      return 'Enter the freelancer wallet address.';
    }
    if (account && freelancer === account.address) {
      return 'The freelancer must be a different wallet from yours.';
    }
    if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(mint)) {
      return 'Enter a valid settlement mint.';
    }
    return null;
  }, [title, freelancer, mint, account]);

  const milestoneProblem = useMemo(() => {
    if (milestones.length === 0) return 'Add at least one milestone.';
    for (const [index, milestone] of parsed.entries()) {
      if (milestone.title.trim().length === 0) {
        return `Milestone ${index + 1} needs a title.`;
      }
      if (milestone.baseUnits === null || milestone.baseUnits <= 0n) {
        return `Milestone ${index + 1} needs an amount greater than zero.`;
      }
    }
    return null;
  }, [milestones, parsed]);

  function updateMilestone(key: string, patch: Partial<MilestoneDraft>) {
    setMilestones((current) =>
      current.map((milestone) =>
        milestone.key === key ? { ...milestone, ...patch } : milestone,
      ),
    );
  }

  function addMilestone() {
    setMilestones((current) =>
      current.length >= MAX_MILESTONES
        ? current
        : [...current, emptyMilestone(current.length)],
    );
  }

  function removeMilestone(key: string) {
    setMilestones((current) =>
      current.length === 1 ? current : current.filter((item) => item.key !== key),
    );
  }

  function goTo(next: Step) {
    setError(null);
    if (next > 0 && termsProblem) {
      setError(termsProblem);
      setStep(0);
      return;
    }
    if (next > 1 && milestoneProblem) {
      setError(milestoneProblem);
      setStep(1);
      return;
    }
    setStep(next);
  }

  async function submit() {
    if (!account || !wallet) {
      open();
      return;
    }
    if (termsProblem) {
      setError(termsProblem);
      setStep(0);
      return;
    }
    if (milestoneProblem) {
      setError(milestoneProblem);
      setStep(1);
      return;
    }

    setSubmitting(true);
    setError(null);

    const terms: TermsInput = {
      title: title.trim(),
      description: description.trim(),
      client: account.address as Address,
      freelancer: freelancer as Address,
      mint: mint as Address,
      milestones: parsed.map((milestone, index) => ({
        index,
        title: milestone.title.trim(),
        description: milestone.description.trim(),
        amount: milestone.baseUnits!,
      })),
    };

    try {
      setProgress('Hashing terms');
      const termsHash = await hashTerms(terms);
      const nonce = nextAgreementNonce();

      setProgress('Building transactions');
      const { agreement, transactions } = await buildDraftInstructions({
        client: terms.client,
        freelancer: terms.freelancer,
        mint: terms.mint,
        nonce,
        termsHash,
        milestones: terms.milestones,
      });

      setProgress('Waiting for wallet signature (1 of 2)');
      const created = await sendInstructions({
        wallet,
        account,
        instructions: transactions[0]!,
      });

      setProgress('Waiting for wallet signature (2 of 2)');
      const milestonesTx = await sendInstructions({
        wallet,
        account,
        instructions: transactions[1]!,
      });

      const hexHash = Array.from(termsHash)
        .map((byte) => byte.toString(16).padStart(2, '0'))
        .join('');

      saveTerms(hexHash, {
        title: terms.title,
        description: terms.description,
        client: terms.client,
        freelancer: terms.freelancer,
        mint: terms.mint,
        milestones: terms.milestones.map((milestone) => ({
          index: milestone.index,
          title: milestone.title,
          description: milestone.description,
          amount: milestone.amount.toString(),
        })),
      });
      trackAgreement(agreement, account.address);
      appendReceipt({
        signature: created.signature,
        agreement,
        kind: 'create',
        at: Date.now(),
        viewer: account.address,
      });
      appendReceipt({
        signature: milestonesTx.signature,
        agreement,
        kind: 'create',
        at: Date.now(),
        viewer: account.address,
      });

      // Metadata is a convenience. The chain already holds the authoritative
      // state, so a failed index write must not fail the agreement.
      void createContract({
        title: terms.title,
        description: terms.description,
        freelancer: terms.freelancer,
        mint: terms.mint,
        agreementNonce: nonce.toString(),
        termsHash: hexHash,
        milestones: terms.milestones.map((milestone) => ({
          title: milestone.title,
          description: milestone.description,
          amount: milestone.amount.toString(),
        })),
      }).catch(() => undefined);

      router.push(`/app/agreements/${agreement}`);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not create the agreement');
      setSubmitting(false);
      setProgress(null);
    }
  }

  if (!account) {
    return (
      <div className="wizard">
        <div className="empty">
          <p className="eyebrow" style={{ justifyContent: 'center' }}>
            New agreement
          </p>
          <h1 className="h3">Connect the client wallet</h1>
          <p className="help measure">
            The client is the wallet that creates and funds the agreement. It signs;
            OpenWork never sees the key.
          </p>
          <button type="button" className="btn" onClick={open}>
            Connect wallet
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="wizard">
      <div
        className="cluster"
        style={{ justifyContent: 'space-between', marginBottom: '1.25rem' }}
      >
        <div className="stack" style={{ gap: '0.25rem' }}>
          <p className="eyebrow">New agreement</p>
          <h1 className="h1">Draft the terms</h1>
        </div>
        <Link href="/app" className="btn btn-ghost btn-sm">
          Cancel
        </Link>
      </div>

      <div className="steps">
        {STEPS.map((label, index) => (
          <span
            key={label}
            className="step-chip"
            data-active={step === index}
            data-done={step > index}
          >
            <span className="mono">{`0${index + 1}`}</span>
            {label}
          </span>
        ))}
      </div>

      {error ? (
        <div
          className="callout callout-bad small"
          role="alert"
          style={{ marginBottom: '1rem' }}
        >
          {error}
        </div>
      ) : null}

      {step === 0 ? (
        <div className="card card-pad stack" style={{ gap: '1.1rem' }}>
          <div className="field">
            <label className="label" htmlFor="title">
              Title
              <span className="label-hint">public to both parties</span>
            </label>
            <input
              id="title"
              className="input"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Landing page rebuild"
              maxLength={120}
            />
          </div>

          <div className="field">
            <label className="label" htmlFor="description">
              Scope
              <span className="label-hint">hashed, not stored on-chain</span>
            </label>
            <textarea
              id="description"
              className="textarea"
              value={description}
              onChange={(event) => setDescription(event.target.value)}
              placeholder="What is being delivered, and what counts as done?"
              maxLength={2000}
            />
          </div>

          <div className="field">
            <label className="label" htmlFor="freelancer">
              Freelancer wallet
            </label>
            <input
              id="freelancer"
              className="input mono"
              value={freelancer}
              onChange={(event) => setFreelancer(event.target.value.trim())}
              placeholder="9WzD…AWWM"
              spellCheck={false}
            />
            <span className="help">
              Only this wallet can submit work or receive a release. It is part of the
              hashed terms.
            </span>
          </div>

          <div className="field">
            <label className="label" htmlFor="mint">
              Settlement mint
              <span className="label-hint">SPL token</span>
            </label>
            <input
              id="mint"
              className="input mono"
              value={mint}
              onChange={(event) => setMint(event.target.value.trim())}
              spellCheck={false}
            />
            <span className="help">
              {symbol} · {mintDecimals} decimals. Both parties need a token account in
              this mint.
            </span>
          </div>

          <div className="cluster" style={{ justifyContent: 'flex-end' }}>
            <button type="button" className="btn" onClick={() => goTo(1)}>
              Continue
            </button>
          </div>
        </div>
      ) : null}

      {step === 1 ? (
        <div className="stack">
          <div className="cluster" style={{ justifyContent: 'space-between' }}>
            <h2 className="h3">Milestones</h2>
            <span className="pill">
              {milestones.length} / {MAX_MILESTONES}
            </span>
          </div>

          {milestones.map((milestone, index) => (
            <div className="milestone-editor" key={milestone.key}>
              <div className="milestone-editor-head">
                <span className="step-num">
                  Milestone {String(index + 1).padStart(2, '0')}
                </span>
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() => removeMilestone(milestone.key)}
                  disabled={milestones.length === 1}
                >
                  Remove
                </button>
              </div>

              <div className="field">
                <label className="label" htmlFor={`title-${milestone.key}`}>
                  Title
                </label>
                <input
                  id={`title-${milestone.key}`}
                  className="input"
                  value={milestone.title}
                  onChange={(event) =>
                    updateMilestone(milestone.key, { title: event.target.value })
                  }
                  placeholder="Design system"
                  maxLength={80}
                />
              </div>

              <div className="field">
                <label className="label" htmlFor={`desc-${milestone.key}`}>
                  Deliverable
                </label>
                <input
                  id={`desc-${milestone.key}`}
                  className="input"
                  value={milestone.description}
                  onChange={(event) =>
                    updateMilestone(milestone.key, { description: event.target.value })
                  }
                  placeholder="Figma file with tokens and components"
                  maxLength={160}
                />
              </div>

              <div className="field">
                <label className="label" htmlFor={`amount-${milestone.key}`}>
                  Amount
                  <span className="label-hint">{symbol}</span>
                </label>
                <input
                  id={`amount-${milestone.key}`}
                  className="input mono"
                  inputMode="decimal"
                  value={milestone.amount}
                  onChange={(event) =>
                    updateMilestone(milestone.key, { amount: event.target.value })
                  }
                  placeholder="1200.00"
                />
              </div>
            </div>
          ))}

          <button
            type="button"
            className="btn btn-secondary"
            onClick={addMilestone}
            disabled={milestones.length >= MAX_MILESTONES}
          >
            Add milestone
          </button>

          <div className="cluster" style={{ justifyContent: 'space-between' }}>
            <button type="button" className="btn btn-ghost" onClick={() => setStep(0)}>
              Back
            </button>
            <button type="button" className="btn" onClick={() => goTo(2)}>
              Review
            </button>
          </div>
        </div>
      ) : null}

      {step === 2 ? (
        <div className="stack">
          <div className="card card-pad stack">
            <div className="cluster" style={{ justifyContent: 'space-between' }}>
              <h2 className="h3">{title}</h2>
              <span className="pill pill-accent">
                {formatAmount(total, mintDecimals)} {symbol}
              </span>
            </div>
            {description ? <p className="small muted">{description}</p> : null}

            <table className="table">
              <thead>
                <tr>
                  <th>#</th>
                  <th>Milestone</th>
                  <th className="num">Amount</th>
                </tr>
              </thead>
              <tbody>
                {parsed.map((milestone, index) => (
                  <tr key={milestone.key}>
                    <td className="mono tiny">{String(index + 1).padStart(2, '0')}</td>
                    <td>
                      <div>{milestone.title}</div>
                      {milestone.description ? (
                        <div className="help">{milestone.description}</div>
                      ) : null}
                    </td>
                    <td className="num">
                      {formatAmount(milestone.baseUnits ?? 0n, mintDecimals)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>

            <div className="kv">
              <div className="kv-row">
                <span className="kv-key">Client</span>
                <span className="kv-val">{shortenAddress(account.address, 6)}</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Freelancer</span>
                <span className="kv-val">{shortenAddress(freelancer, 6)}</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Mint</span>
                <span className="kv-val">{shortenAddress(mint, 6)}</span>
              </div>
            </div>

            <p className="help">
              Review signs twice: once to create the agreement, once to add the
              milestones. After that the freelancer accepts with the same terms hash,
              which locks the scope.
            </p>
          </div>

          {progress ? (
            <div className="callout small" role="status">
              <span className="cluster">
                <span className="spin" aria-hidden="true" />
                {progress}
              </span>
            </div>
          ) : null}

          <div className="cluster" style={{ justifyContent: 'space-between' }}>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => setStep(1)}
              disabled={submitting}
            >
              Back
            </button>
            <button
              type="button"
              className="btn btn-lg"
              onClick={() => void submit()}
              disabled={submitting}
            >
              {submitting ? 'Creating…' : 'Create agreement'}
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
