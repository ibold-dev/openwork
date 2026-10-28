'use client';

import Link from 'next/link';
import { useParams } from 'next/navigation';
import { useCallback, useEffect, useMemo, useState } from 'react';

import { AgreementStatus, MilestoneStatus } from '@/components/status-pill';
import { useWallet } from '@/components/wallet-provider';
import {
  acceptTerms,
  cancelDraft,
  decodeCoSign,
  encodeCoSign,
  fundMilestone,
  mutualRefund,
  raiseDispute,
  releaseMilestone,
  submitDeliverable,
} from '@/lib/actions';
import {
  associatedTokenAddress,
  buildWireTransaction,
  coSignWireTransaction,
  fetchAgreement,
  fetchTokenBalance,
  sendInstructions,
  submitWireTransaction,
  vaultAddress,
  type AgreementView,
} from '@/lib/chain';
import { CHAIN, CLUSTER, KNOWN_MINTS } from '@/lib/config';
import {
  bytesToHexString,
  formatAmount,
  formatTimestamp,
  shortenAddress,
} from '@/lib/format';
import { appendReceipt, loadTerms, trackAgreement } from '@/lib/storage';
import { canonicalizeTerms, hashDeliverable, type TermsInput } from '@openwork/sdk';
import type { Address } from '@solana/kit';
import { getTransactionDecoder } from '@solana/kit';
import { encodeTermsFragment, sha256Hex, type SharedTerms } from '@/lib/terms-share';

interface CoSignState {
  readonly wire: Uint8Array;
  readonly signers: readonly string[];
  readonly signed: readonly string[];
}

export default function AgreementPage() {
  const params = useParams<{ address: string }>();
  const address = (
    Array.isArray(params.address) ? params.address[0] : params.address
  ) as Address;
  const { account, wallet, open } = useWallet();
  const viewer = account?.address ?? null;

  const [view, setView] = useState<AgreementView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [submitFor, setSubmitFor] = useState<string | null>(null);
  const [deliverableUri, setDeliverableUri] = useState('');
  const [deliverableNote, setDeliverableNote] = useState('');
  const [coSign, setCoSign] = useState<CoSignState | null>(null);
  const [coSignLink, setCoSignLink] = useState<string | null>(null);
  const [sharedTerms, setSharedTerms] = useState<SharedTerms | null>(null);
  const [clientBalance, setClientBalance] = useState<bigint | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(address)) {
      setError('That is not a valid agreement address.');
      setLoading(false);
      return;
    }
    try {
      const next = await fetchAgreement(address);
      if (!next) {
        setError('No agreement exists at this address on ' + CLUSTER + '.');
      } else {
        setView(next);
        setError(null);
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not reach the RPC');
    } finally {
      setLoading(false);
    }
  }, [address]);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 20_000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (!view || !viewer) return;
    trackAgreement(address, viewer);
  }, [view, viewer, address]);

  // Hydrate terms from the URL fragment, then from this browser's cache. A
  // fragment is only trusted when its hash equals the on-chain commitment.
  useEffect(() => {
    if (!view) return;
    let cancelled = false;
    const onChainHash = bytesToHexString(view.account.termsHash);

    async function hydrate() {
      const hash = window.location.hash.replace(/^#/, '');
      const fragment = hash ? new URLSearchParams(hash).get('terms') : null;
      if (fragment) {
        const { decodeTermsFragment } = await import('@/lib/terms-share');
        const shared = decodeTermsFragment(fragment);
        if (shared) {
          const digest = await sha256Hex(shared.canonical);
          if (!cancelled && digest === onChainHash) {
            setSharedTerms(shared);
            return;
          }
        }
      }

      const stored = loadTerms(onChainHash);
      if (!cancelled && stored) {
        setSharedTerms({
          canonical: canonicalizeTerms({
            title: stored.title,
            description: stored.description,
            client: stored.client as Address,
            freelancer: stored.freelancer as Address,
            mint: stored.mint as Address,
            milestones: stored.milestones.map((milestone) => ({
              index: milestone.index,
              title: milestone.title,
              description: milestone.description,
              amount: BigInt(milestone.amount),
            })),
          }),
          parsed: {
            version: 1,
            title: stored.title,
            description: stored.description,
            client: stored.client as Address,
            freelancer: stored.freelancer as Address,
            mint: stored.mint as Address,
            milestones: stored.milestones.map((milestone) => ({
              index: milestone.index,
              title: milestone.title,
              description: milestone.description,
              amount: BigInt(milestone.amount),
            })),
          } as SharedTerms['parsed'],
        });
      }
    }

    void hydrate();
    return () => {
      cancelled = true;
    };
  }, [view]);

  // Decode a co-signature handoff from the fragment.
  useEffect(() => {
    if (!view) return;
    const hash = window.location.hash.replace(/^#/, '');
    const encoded = hash ? new URLSearchParams(hash).get('cosign') : null;
    if (!encoded) return;
    const wire = decodeCoSign(encoded);
    if (!wire) {
      setNotice('The co-signature link is malformed.');
      return;
    }
    try {
      const decoded = getTransactionDecoder().decode(wire);
      const entries = Object.entries(decoded.signatures);
      const signed = entries
        .filter(([, signature]) => signature?.some((byte) => byte !== 0))
        .map(([signer]) => signer);
      setCoSign({ wire, signers: entries.map(([signer]) => signer), signed });
    } catch {
      setNotice('The co-signature transaction could not be decoded.');
    }
  }, [view]);

  const refs = useMemo(() => {
    if (!view) return null;
    return {
      agreement: address,
      client: view.account.client as Address,
      freelancer: view.account.freelancer as Address,
      mint: view.account.mint as Address,
      vault: null as Address | null,
      vaultAuthority: null as Address | null,
    };
  }, [view, address]);

  const [vault, setVault] = useState<Address | null>(null);

  useEffect(() => {
    if (!view) return;
    void vaultAddress(address, view.account.mint as Address).then(setVault);
  }, [view, address]);

  useEffect(() => {
    if (!view || !viewer) return;
    void associatedTokenAddress(viewer as Address, view.account.mint as Address)
      .then(fetchTokenBalance)
      .then(setClientBalance)
      .catch(() => setClientBalance(null));
  }, [view, viewer]);

  const role = useMemo(() => {
    if (!view || !viewer) return 'observer' as const;
    if (view.account.client === viewer) return 'client' as const;
    if (view.account.freelancer === viewer) return 'freelancer' as const;
    return 'observer' as const;
  }, [view, viewer]);

  const mintInfo = view
    ? (KNOWN_MINTS[view.account.mint] ?? { symbol: 'tokens', decimals: 6 })
    : { symbol: 'tokens', decimals: 6 };

  const termsHashHex = view ? bytesToHexString(view.account.termsHash) : '';

  const shareLink = useMemo(() => {
    if (!sharedTerms || typeof window === 'undefined') return null;
    return `${window.location.origin}/app/agreements/${address}#terms=${encodeTermsFragment(sharedTerms.canonical)}`;
  }, [sharedTerms, address]);

  function copy(value: string, label: string) {
    void navigator.clipboard
      .writeText(value)
      .then(() => {
        setCopied(label);
        window.setTimeout(() => setCopied(null), 1600);
      })
      .catch(() => setNotice('Copy failed. Select the text manually.'));
  }

  async function run(
    label: string,
    kind: Parameters<typeof appendReceipt>[0]['kind'],
    build: () => Promise<import('@solana/kit').Instruction[]>,
  ) {
    if (!wallet || !account) {
      open();
      return;
    }
    setBusy(label);
    setError(null);
    setNotice(null);
    try {
      const instructions = await build();
      const result = await sendInstructions({ wallet, account, instructions });
      appendReceipt({
        signature: result.signature,
        agreement: address,
        kind,
        at: Date.now(),
        viewer: account.address,
      });
      setNotice(`${label} confirmed.`);
      setConfirming(null);
      setSubmitFor(null);
      await refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : `${label} failed`);
    } finally {
      setBusy(null);
    }
  }

  async function startCoSign(milestoneAddress: Address) {
    if (!view || !wallet || !account || !vault) return;
    setBusy('Preparing refund');
    setError(null);
    try {
      const clientAta = await associatedTokenAddress(
        view.account.client as Address,
        view.account.mint as Address,
      );
      const instructions = mutualRefund({
        refs: {
          agreement: address,
          client: view.account.client as Address,
          freelancer: view.account.freelancer as Address,
          mint: view.account.mint as Address,
          vault,
          vaultAuthority: await vaultAuthorityFor(view),
        },
        milestone: milestoneAddress,
        clientTokenAccount: clientAta,
      });
      const wire = await buildWireTransaction(account.address as Address, instructions);
      const signed = await coSignWireTransaction({
        wallet,
        account,
        wireTransaction: wire,
      });
      const link = `${window.location.origin}/app/agreements/${address}#cosign=${encodeCoSign(signed)}`;
      setCoSignLink(link);
      copy(link, 'co-sign link');
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not build the refund');
    } finally {
      setBusy(null);
    }
  }

  async function finishCoSign() {
    if (!wallet || !account || !coSign) return;
    setBusy('Submitting refund');
    setError(null);
    try {
      const signed = await coSignWireTransaction({
        wallet,
        account,
        wireTransaction: coSign.wire,
      });
      const signature = await submitWireTransaction(signed);
      appendReceipt({
        signature,
        agreement: address,
        kind: 'refund',
        at: Date.now(),
        viewer: account.address,
      });
      setNotice('Mutual refund submitted.');
      setCoSign(null);
      window.history.replaceState(null, '', window.location.pathname);
      await refresh();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Could not submit the refund');
    } finally {
      setBusy(null);
    }
  }

  async function submitWork(milestone: Address, index: number) {
    if (!view) return;
    if (!deliverableUri.trim() && !deliverableNote.trim()) {
      setError('Add a link or a note describing the deliverable.');
      return;
    }
    await run('Submit milestone', 'submit', async () => {
      const hash = await hashDeliverable({
        agreement: address,
        milestoneIndex: index,
        uri: deliverableUri.trim(),
        note: deliverableNote.trim(),
      });
      return submitDeliverable({
        refs: {
          agreement: address,
          client: view.account.client as Address,
          freelancer: view.account.freelancer as Address,
          mint: view.account.mint as Address,
          vault: vault!,
          vaultAuthority: await vaultAuthorityFor(view),
        },
        milestone,
        deliverableHash: hash,
      });
    });
    setDeliverableUri('');
    setDeliverableNote('');
  }

  if (loading && !view) {
    return (
      <div className="shell" style={{ paddingBlock: '3rem' }}>
        <div className="stack">
          <div className="skeleton" style={{ height: 28, width: 320 }} />
          <div className="skeleton" style={{ height: 120 }} />
          <div className="skeleton" style={{ height: 200 }} />
        </div>
      </div>
    );
  }

  if (!view) {
    return (
      <div className="shell">
        <div className="empty" style={{ marginBlock: '4rem' }}>
          <p className="eyebrow" style={{ justifyContent: 'center' }}>
            Agreement
          </p>
          <h1 className="h3">Nothing at this address</h1>
          <p className="help measure">
            {error ?? 'The account does not exist on this cluster.'}
          </p>
          <Link href="/app" className="btn">
            Back to the ledger
          </Link>
        </div>
      </div>
    );
  }

  const total = view.account.totalAmount;
  const released = view.account.releasedAmount;
  const locked = view.milestones
    .filter((milestone) =>
      ['funded', 'submitted', 'disputed'].includes(milestone.account.state),
    )
    .reduce((sum, milestone) => sum + milestone.account.amount, 0n);

  const title = sharedTerms?.parsed.title ?? 'Untitled agreement';

  return (
    <div className="shell">
      <div className="dash-head">
        <div className="stack" style={{ gap: '0.35rem', minWidth: 0 }}>
          <Link href="/app" className="tiny mono faint">
            ← ledger
          </Link>
          <h1 className="h1" style={{ overflowWrap: 'anywhere' }}>
            {title}
          </h1>
          <div className="contract-meta" style={{ marginTop: 0 }}>
            <span>{shortenAddress(address, 8)}</span>
            <span>role: {role}</span>
            <span>created {formatTimestamp(view.account.createdAt)}</span>
          </div>
        </div>
        <div className="cluster" style={{ justifyContent: 'flex-end' }}>
          <AgreementStatus state={view.account.state} />
          <span className="pill">{CHAIN}</span>
        </div>
      </div>

      {notice ? (
        <div className="callout small" role="status" style={{ marginBottom: '1rem' }}>
          {notice}
        </div>
      ) : null}
      {error ? (
        <div
          className="callout callout-bad small"
          role="alert"
          style={{ marginBottom: '1rem' }}
        >
          {error}
        </div>
      ) : null}

      {coSign ? (
        <div className="card card-pad stack" style={{ marginBottom: '1rem' }}>
          <div className="cluster" style={{ justifyContent: 'space-between' }}>
            <h2 className="h3">Mutual refund · co-signature</h2>
            <span className="pill pill-warn">2 signatures</span>
          </div>
          <p className="help">
            This transaction refunds a disputed milestone. It is valid only with both
            signatures. Already signed:{' '}
            {coSign.signed.map((signer) => shortenAddress(signer, 4)).join(', ') ||
              'none'}
            .
          </p>
          <div className="cluster">
            <button
              type="button"
              className="btn"
              onClick={() => void finishCoSign()}
              disabled={busy !== null}
            >
              {busy ?? 'Add my signature and submit'}
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => {
                setCoSign(null);
                window.history.replaceState(null, '', window.location.pathname);
              }}
            >
              Dismiss
            </button>
          </div>
          <p className="help">
            Your wallet must sign a transaction where it is not the fee payer. If it
            refuses, the other party can still submit after signing with a wallet that
            allows co-signing.
          </p>
        </div>
      ) : null}

      {coSignLink ? (
        <div className="card card-pad stack" style={{ marginBottom: '1rem' }}>
          <h2 className="h3">Your signature is attached</h2>
          <p className="help">
            Send this link to the other party. It contains the transaction, not a key.
            They add their signature and submit; the fee is paid by your wallet, which
            already signed.
          </p>
          <input
            className="input mono"
            readOnly
            value={coSignLink}
            onFocus={(event) => event.target.select()}
          />
          <div className="cluster">
            <button
              type="button"
              className="btn btn-secondary"
              onClick={() => copy(coSignLink, 'co-sign link')}
            >
              {copied === 'co-sign link' ? 'Copied' : 'Copy link'}
            </button>
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => setCoSignLink(null)}
            >
              Done
            </button>
          </div>
        </div>
      ) : null}

      <div className="detail-grid">
        <div className="stack">
          <div className="card card-pad">
            <div className="stat-row" style={{ border: 0 }}>
              <div className="stat" style={{ background: 'transparent' }}>
                <span className="stat-label">Total</span>
                <span className="stat-value">
                  {formatAmount(total, mintInfo.decimals)}
                </span>
                <span className="help">{mintInfo.symbol}</span>
              </div>
              <div className="stat" style={{ background: 'transparent' }}>
                <span className="stat-label">Released</span>
                <span className="stat-value">
                  {formatAmount(released, mintInfo.decimals)}
                </span>
                <span className="help">
                  {view.account.settledCount} milestone
                  {view.account.settledCount === 1 ? '' : 's'} settled
                </span>
              </div>
              <div className="stat" style={{ background: 'transparent' }}>
                <span className="stat-label">In vault</span>
                <span className="stat-value gradient-text">
                  {formatAmount(view.vaultBalance ?? locked, mintInfo.decimals)}
                </span>
                <span className="help">read from the token account</span>
              </div>
              <div className="stat" style={{ background: 'transparent' }}>
                <span className="stat-label">Refunded</span>
                <span className="stat-value">{view.account.refundedCount}</span>
                <span className="help">requires both signatures</span>
              </div>
            </div>
          </div>

          <div className="card card-pad stack">
            <div className="cluster" style={{ justifyContent: 'space-between' }}>
              <h2 className="h3">Milestones</h2>
              <button
                type="button"
                className="btn btn-ghost btn-sm"
                onClick={() => void refresh()}
                disabled={busy !== null}
              >
                Refresh
              </button>
            </div>

            {view.account.state === 'draft' && role === 'freelancer' ? (
              <div className="callout">
                <strong className="small">Review the terms before accepting</strong>
                {sharedTerms ? (
                  <>
                    <p className="help">
                      The hash of the terms below matches the hash committed on chain.
                      Accepting locks the scope and the milestone amounts.
                    </p>
                    <button
                      type="button"
                      className="btn btn-sm"
                      onClick={() =>
                        void run('Accept terms', 'accept', async () =>
                          acceptTerms(
                            {
                              agreement: address,
                              client: view.account.client as Address,
                              freelancer: view.account.freelancer as Address,
                              mint: view.account.mint as Address,
                              vault: vault!,
                              vaultAuthority: await vaultAuthorityFor(view),
                            },
                            view.account.termsHash,
                          ),
                        )
                      }
                      disabled={busy !== null || view.account.milestoneCount === 0}
                    >
                      {busy === 'Accept terms' ? 'Accepting…' : 'Accept these terms'}
                    </button>
                  </>
                ) : (
                  <p className="help">
                    Open the link the client shared with you. It carries the exact terms;
                    without them the hash cannot be reproduced and the program will reject
                    an acceptance.
                  </p>
                )}
              </div>
            ) : null}

            {view.account.state === 'draft' && role === 'client' ? (
              <div className="callout">
                <strong className="small">Waiting for the freelancer</strong>
                <p className="help">
                  Share the agreement link so they can review the hashed terms and accept.{' '}
                  {shareLink
                    ? 'The link already includes the terms.'
                    : 'Open this page with the terms fragment to generate a shareable link.'}
                </p>
                {shareLink ? (
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    onClick={() => copy(shareLink, 'agreement link')}
                  >
                    {copied === 'agreement link' ? 'Copied' : 'Copy agreement link'}
                  </button>
                ) : null}
              </div>
            ) : null}

            <div className="milestones">
              {view.milestones.map((milestone) => {
                const state = milestone.account.state;
                const isConfirming = confirming === milestone.address;
                const isSubmitting = submitFor === milestone.address;
                return (
                  <div className="milestone" key={milestone.address} data-state={state}>
                    <div className="milestone-node">
                      <span className="milestone-dot">
                        {String(milestone.account.index + 1).padStart(2, '0')}
                      </span>
                      <span className="milestone-line" aria-hidden="true" />
                    </div>

                    <div className="milestone-body">
                      <div className="milestone-head">
                        <div>
                          <div className="milestone-name">
                            {sharedTerms?.parsed.milestones.find(
                              (item) => item.index === milestone.account.index,
                            )?.title ?? `Milestone ${milestone.account.index + 1}`}
                          </div>
                          {sharedTerms?.parsed.milestones.find(
                            (item) => item.index === milestone.account.index,
                          )?.description ? (
                            <div className="milestone-note">
                              {
                                sharedTerms.parsed.milestones.find(
                                  (item) => item.index === milestone.account.index,
                                )!.description
                              }
                            </div>
                          ) : null}
                        </div>
                        <div className="cluster" style={{ gap: '0.5rem' }}>
                          <span className="ledger-amount">
                            {formatAmount(milestone.account.amount, mintInfo.decimals)}
                          </span>
                          <MilestoneStatus state={state} />
                        </div>
                      </div>

                      <div className="milestone-note">
                        {state === 'pending'
                          ? 'Not funded yet.'
                          : state === 'funded'
                            ? `Funded ${formatTimestamp(milestone.account.fundedAt)}`
                            : state === 'submitted'
                              ? `Submitted ${formatTimestamp(milestone.account.submittedAt)}`
                              : state === 'released'
                                ? `Released ${formatTimestamp(milestone.account.settledAt)}`
                                : state === 'refunded'
                                  ? `Refunded ${formatTimestamp(milestone.account.settledAt)}`
                                  : 'Disputed — release is frozen until both parties sign a refund.'}
                      </div>

                      {milestone.account.deliverableHash.some((byte) => byte !== 0) ? (
                        <div className="receipt-meta">
                          deliverable{' '}
                          {shortenAddress(
                            bytesToHexString(milestone.account.deliverableHash),
                            10,
                          )}
                        </div>
                      ) : null}

                      <div className="milestone-actions">
                        {state === 'pending' && role === 'client' ? (
                          isConfirming ? (
                            <>
                              <button
                                type="button"
                                className="btn btn-sm"
                                onClick={() =>
                                  void run(
                                    `Fund milestone ${milestone.account.index + 1}`,
                                    'fund',
                                    async () => {
                                      const clientAta = await associatedTokenAddress(
                                        view.account.client as Address,
                                        view.account.mint as Address,
                                      );
                                      return fundMilestone({
                                        refs: {
                                          agreement: address,
                                          client: view.account.client as Address,
                                          freelancer: view.account.freelancer as Address,
                                          mint: view.account.mint as Address,
                                          vault: vault!,
                                          vaultAuthority: await vaultAuthorityFor(view),
                                        },
                                        milestone: milestone.address,
                                        clientTokenAccount: clientAta,
                                      });
                                    },
                                  )
                                }
                                disabled={busy !== null || !vault}
                              >
                                {busy?.startsWith('Fund')
                                  ? 'Funding…'
                                  : 'Confirm funding'}
                              </button>
                              <button
                                type="button"
                                className="btn btn-ghost btn-sm"
                                onClick={() => setConfirming(null)}
                              >
                                Cancel
                              </button>
                            </>
                          ) : (
                            <button
                              type="button"
                              className="btn btn-sm"
                              onClick={() => setConfirming(milestone.address)}
                              disabled={view.account.state !== 'active'}
                            >
                              Fund{' '}
                              {formatAmount(milestone.account.amount, mintInfo.decimals)}{' '}
                              {mintInfo.symbol}
                            </button>
                          )
                        ) : null}

                        {state === 'pending' && role === 'client' ? (
                          <span className="help">
                            balance{' '}
                            {clientBalance === null
                              ? 'no token account'
                              : `${formatAmount(clientBalance, mintInfo.decimals)} ${mintInfo.symbol}`}
                          </span>
                        ) : null}

                        {state === 'funded' && role === 'freelancer' ? (
                          <button
                            type="button"
                            className="btn btn-secondary btn-sm"
                            onClick={() =>
                              setSubmitFor(isSubmitting ? null : milestone.address)
                            }
                          >
                            {isSubmitting ? 'Close' : 'Submit work'}
                          </button>
                        ) : null}

                        {state === 'submitted' && role === 'client' ? (
                          <>
                            {isConfirming ? (
                              <>
                                <button
                                  type="button"
                                  className="btn btn-sm"
                                  onClick={() =>
                                    void run('Release milestone', 'release', async () =>
                                      releaseMilestone({
                                        refs: {
                                          agreement: address,
                                          client: view.account.client as Address,
                                          freelancer: view.account.freelancer as Address,
                                          mint: view.account.mint as Address,
                                          vault: vault!,
                                          vaultAuthority: await vaultAuthorityFor(view),
                                        },
                                        milestone: milestone.address,
                                        freelancerTokenAccount:
                                          await associatedTokenAddress(
                                            view.account.freelancer as Address,
                                            view.account.mint as Address,
                                          ),
                                      }),
                                    )
                                  }
                                  disabled={busy !== null || !vault}
                                >
                                  {busy === 'Release milestone'
                                    ? 'Releasing…'
                                    : 'Confirm release'}
                                </button>
                                <button
                                  type="button"
                                  className="btn btn-ghost btn-sm"
                                  onClick={() => setConfirming(null)}
                                >
                                  Cancel
                                </button>
                              </>
                            ) : (
                              <button
                                type="button"
                                className="btn btn-accent btn-sm"
                                onClick={() => setConfirming(milestone.address)}
                              >
                                Approve &amp; release
                              </button>
                            )}
                          </>
                        ) : null}

                        {state === 'submitted' && role === 'freelancer' ? (
                          <span className="help">Waiting for the client to approve.</span>
                        ) : null}

                        {(state === 'funded' || state === 'submitted') &&
                        (role === 'client' || role === 'freelancer') ? (
                          <button
                            type="button"
                            className="btn btn-ghost btn-sm"
                            onClick={() =>
                              void run('Raise dispute', 'dispute', async () =>
                                raiseDispute({
                                  refs: {
                                    agreement: address,
                                    client: view.account.client as Address,
                                    freelancer: view.account.freelancer as Address,
                                    mint: view.account.mint as Address,
                                    vault: vault!,
                                    vaultAuthority: await vaultAuthorityFor(view),
                                  },
                                  milestone: milestone.address,
                                  signer: account!.address as Address,
                                }),
                              )
                            }
                            disabled={busy !== null}
                          >
                            {busy === 'Raise dispute' ? 'Raising…' : 'Raise dispute'}
                          </button>
                        ) : null}

                        {state === 'disputed' &&
                        (role === 'client' || role === 'freelancer') ? (
                          <button
                            type="button"
                            className="btn btn-secondary btn-sm"
                            onClick={() => void startCoSign(milestone.address)}
                            disabled={busy !== null || !vault}
                          >
                            {busy === 'Preparing refund'
                              ? 'Preparing…'
                              : 'Start mutual refund'}
                          </button>
                        ) : null}
                      </div>

                      {isSubmitting ? (
                        <div
                          className="sunken card-pad stack"
                          style={{ marginTop: '0.5rem' }}
                        >
                          <div className="field">
                            <label className="label" htmlFor={`uri-${milestone.address}`}>
                              Deliverable link
                              <span className="label-hint">optional</span>
                            </label>
                            <input
                              id={`uri-${milestone.address}`}
                              className="input"
                              value={deliverableUri}
                              onChange={(event) => setDeliverableUri(event.target.value)}
                              placeholder="https://…"
                            />
                          </div>
                          <div className="field">
                            <label
                              className="label"
                              htmlFor={`note-${milestone.address}`}
                            >
                              Note
                              <span className="label-hint">
                                hashed, not stored on-chain
                              </span>
                            </label>
                            <input
                              id={`note-${milestone.address}`}
                              className="input"
                              value={deliverableNote}
                              onChange={(event) => setDeliverableNote(event.target.value)}
                              placeholder="What changed in this milestone"
                            />
                          </div>
                          <div className="cluster">
                            <button
                              type="button"
                              className="btn btn-sm"
                              onClick={() =>
                                void submitWork(
                                  milestone.address,
                                  milestone.account.index,
                                )
                              }
                              disabled={busy !== null}
                            >
                              {busy === 'Submit milestone'
                                ? 'Submitting…'
                                : 'Sign & submit'}
                            </button>
                            <button
                              type="button"
                              className="btn btn-ghost btn-sm"
                              onClick={() => setSubmitFor(null)}
                            >
                              Cancel
                            </button>
                          </div>
                          <p className="help">
                            Only the hash goes on chain. Anyone with the link and note can
                            verify it later.
                          </p>
                        </div>
                      ) : null}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>

          <div className="card card-pad stack">
            <h2 className="h3">Activity</h2>
            <div className="stack" style={{ gap: 0 }}>
              <TimelineRow label="Agreement created" at={view.account.createdAt} tx />
              {view.account.acceptedAt > 0n ? (
                <TimelineRow label="Terms accepted" at={view.account.acceptedAt} />
              ) : null}
              {view.milestones.flatMap((milestone) => {
                const rows: React.ReactNode[] = [];
                if (milestone.account.fundedAt > 0n) {
                  rows.push(
                    <TimelineRow
                      key={`f-${milestone.address}`}
                      label={`Milestone ${milestone.account.index + 1} funded`}
                      at={milestone.account.fundedAt}
                    />,
                  );
                }
                if (milestone.account.submittedAt > 0n) {
                  rows.push(
                    <TimelineRow
                      key={`s-${milestone.address}`}
                      label={`Milestone ${milestone.account.index + 1} submitted`}
                      at={milestone.account.submittedAt}
                    />,
                  );
                }
                if (milestone.account.settledAt > 0n) {
                  rows.push(
                    <TimelineRow
                      key={`d-${milestone.address}`}
                      label={`Milestone ${milestone.account.index + 1} ${milestone.account.state}`}
                      at={milestone.account.settledAt}
                    />,
                  );
                }
                return rows;
              })}
            </div>
            <p className="help">
              Timestamps come from the on-chain accounts. The program clock is
              authoritative.
            </p>
          </div>
        </div>

        <div className="stack">
          <div className="card card-pad stack">
            <h2 className="h3">Parties</h2>
            <div className="kv">
              <div className="kv-row">
                <span className="kv-key">Client</span>
                <span className="kv-val">
                  {view.account.client === viewer
                    ? 'you'
                    : shortenAddress(view.account.client, 6)}
                </span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Freelancer</span>
                <span className="kv-val">
                  {view.account.freelancer === viewer
                    ? 'you'
                    : shortenAddress(view.account.freelancer, 6)}
                </span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Mint</span>
                <span className="kv-val">{mintInfo.symbol}</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Vault</span>
                <span className="kv-val">{vault ? shortenAddress(vault, 6) : '—'}</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Terms hash</span>
                <span className="kv-val" title={termsHashHex}>
                  {shortenAddress(termsHashHex, 8)}
                </span>
              </div>
            </div>
            <button
              type="button"
              className="btn btn-secondary btn-sm"
              onClick={() => copy(termsHashHex, 'terms hash')}
            >
              {copied === 'terms hash' ? 'Copied' : 'Copy terms hash'}
            </button>
          </div>

          <div className="card card-pad stack">
            <h2 className="h3">Terms</h2>
            {sharedTerms ? (
              <>
                <p className="small">
                  {sharedTerms.parsed.description || 'No scope note.'}
                </p>
                <div className="kv">
                  {sharedTerms.parsed.milestones.map((milestone) => (
                    <div className="kv-row" key={milestone.index}>
                      <span className="kv-key">
                        {milestone.index + 1}. {milestone.title}
                      </span>
                      <span className="kv-val">
                        {formatAmount(milestone.amount, mintInfo.decimals)}
                      </span>
                    </div>
                  ))}
                </div>
                {shareLink ? (
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    onClick={() => copy(shareLink, 'agreement link')}
                  >
                    {copied === 'agreement link' ? 'Copied' : 'Copy agreement link'}
                  </button>
                ) : null}
              </>
            ) : (
              <p className="help">
                The words behind this agreement are not in this browser. Open the link the
                other party shared — it carries the terms, and this page checks them
                against the on-chain hash before showing them.
              </p>
            )}
          </div>

          {role === 'client' && view.account.state === 'active' ? (
            <div className="card card-pad stack">
              <h2 className="h3">Fund from another chain</h2>
              <p className="help">
                Get a quote, deposit on the origin chain, then fund the milestone from
                your wallet. OpenWork never holds the bridged funds.
              </p>
              <Link
                href={`/app/fund?agreement=${address}`}
                className="btn btn-secondary btn-sm"
              >
                Open funding flow
              </Link>
            </div>
          ) : null}

          {role === 'client' && view.account.state === 'draft' ? (
            <div className="card card-pad stack">
              <h2 className="h3">Cancel draft</h2>
              <p className="help">
                A draft can be cancelled by the client alone. No funds have moved at this
                point.
              </p>
              <button
                type="button"
                className="btn btn-secondary btn-sm"
                onClick={() =>
                  void run('Cancel draft', 'cancel', async () =>
                    cancelDraft({
                      agreement: address,
                      client: view.account.client as Address,
                      freelancer: view.account.freelancer as Address,
                      mint: view.account.mint as Address,
                      vault: vault!,
                      vaultAuthority: await vaultAuthorityFor(view),
                    }),
                  )
                }
                disabled={busy !== null || view.account.milestoneCount > 0}
              >
                {busy === 'Cancel draft' ? 'Cancelling…' : 'Cancel agreement'}
              </button>
            </div>
          ) : null}

          <div className="card card-pad stack">
            <h2 className="h3">Verify independently</h2>
            <p className="help">
              Everything on this page is derived from the two accounts below. Check them
              in any explorer.
            </p>
            <a
              className="btn btn-ghost btn-sm"
              href={`https://explorer.solana.com/address/${address}?cluster=${CLUSTER}`}
              target="_blank"
              rel="noreferrer"
            >
              Agreement account ↗
            </a>
            {vault ? (
              <a
                className="btn btn-ghost btn-sm"
                href={`https://explorer.solana.com/address/${vault}?cluster=${CLUSTER}`}
                target="_blank"
                rel="noreferrer"
              >
                Vault token account ↗
              </a>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}

async function vaultAuthorityFor(view: AgreementView): Promise<Address> {
  const { findVaultAuthorityPda } = await import('@openwork/sdk');
  return findVaultAuthorityPda(view.address);
}

function TimelineRow({ label, at, tx }: { label: string; at: bigint; tx?: boolean }) {
  return (
    <div className="receipt">
      <div>
        <div className="receipt-kind">{label}</div>
        <div className="receipt-meta">{formatTimestamp(at)}</div>
      </div>
      <span className="tiny mono faint">{tx ? 'on-chain' : ''}</span>
    </div>
  );
}
