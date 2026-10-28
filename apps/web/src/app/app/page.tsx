'use client';

import Link from 'next/link';
import { useCallback, useEffect, useMemo, useState } from 'react';

import { AgreementStatus } from '@/components/status-pill';
import { useWallet } from '@/components/wallet-provider';
import { fetchAgreement, type AgreementView } from '@/lib/chain';
import { CLUSTER, DEFAULT_MINT, KNOWN_MINTS, PROGRAM_ID } from '@/lib/config';
import { formatAmount, relativeTime, shortenAddress } from '@/lib/format';
import { listReceipts, listTrackedAgreements, trackAgreement } from '@/lib/storage';
import type { Address } from '@solana/kit';

type Filter = 'all' | 'client' | 'freelancer';

interface LoadedAgreement {
  readonly address: string;
  readonly view: AgreementView | null;
  readonly error: string | null;
}

export default function DashboardPage() {
  const { account, open } = useWallet();
  const viewer = account?.address ?? null;

  const [rows, setRows] = useState<readonly LoadedAgreement[]>([]);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState<Filter>('all');
  const [lookup, setLookup] = useState('');
  const [lookupError, setLookupError] = useState<string | null>(null);

  const tracked = useMemo(() => (viewer ? listTrackedAgreements(viewer) : []), [viewer]);

  const load = useCallback(async () => {
    if (!viewer) return;
    if (tracked.length === 0) {
      setRows([]);
      return;
    }
    setLoading(true);
    const loaded = await Promise.all(
      tracked.map(async (entry): Promise<LoadedAgreement> => {
        try {
          const view = await fetchAgreement(entry.address as Address);
          return { address: entry.address, view, error: null };
        } catch (cause) {
          return {
            address: entry.address,
            view: null,
            error: cause instanceof Error ? cause.message : 'Could not read agreement',
          };
        }
      }),
    );
    setRows(loaded);
    setLoading(false);
  }, [tracked, viewer]);

  useEffect(() => {
    void load();
  }, [load]);

  const visible = rows.filter(({ view }) => {
    if (!view || !viewer) return true;
    if (filter === 'client') return view.account.client === viewer;
    if (filter === 'freelancer') return view.account.freelancer === viewer;
    return true;
  });

  const stats = useMemo(() => {
    let locked = 0n;
    let released = 0n;
    let open = 0;
    let disputed = 0;
    for (const { view } of rows) {
      if (!view) continue;
      if (view.account.state === 'active' || view.account.state === 'draft') open += 1;
      released += view.account.releasedAmount;
      for (const milestone of view.milestones) {
        if (
          milestone.account.state === 'funded' ||
          milestone.account.state === 'submitted'
        ) {
          locked += milestone.account.amount;
        }
        if (milestone.account.state === 'disputed') {
          locked += milestone.account.amount;
          disputed += 1;
        }
      }
    }
    return { locked, released, open, disputed };
  }, [rows]);

  const mintInfo = KNOWN_MINTS[DEFAULT_MINT] ?? { symbol: 'tokens', decimals: 6 };

  function submitLookup(event: React.FormEvent) {
    event.preventDefault();
    const value = lookup.trim();
    if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(value)) {
      setLookupError('That does not look like a Solana address.');
      return;
    }
    setLookupError(null);
    if (viewer) trackAgreement(value, viewer);
    window.location.href = `/app/agreements/${value}`;
  }

  if (!account || !viewer) {
    return (
      <div className="shell">
        <div className="empty" style={{ marginBlock: '4rem' }}>
          <p className="eyebrow">Dashboard</p>
          <h1 className="h3">Connect a wallet to see your agreements</h1>
          <p className="help measure">
            Your agreements are keyed to your wallet. OpenWork reads them from the chain;
            this browser only remembers which ones you have opened.
          </p>
          <button
            type="button"
            className="btn"
            onClick={open}
            style={{ marginTop: '0.5rem' }}
          >
            Connect wallet
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="shell">
      <div className="dash-head">
        <div className="stack" style={{ gap: '0.35rem' }}>
          <p className="eyebrow">Ledger · {shortenAddress(viewer, 5)}</p>
          <h1 className="h1">Agreements</h1>
        </div>
        <div className="cluster">
          <Link href="/app/new" className="btn">
            New agreement
          </Link>
          <Link href="/app/fund" className="btn btn-secondary">
            Cross-chain funding
          </Link>
        </div>
      </div>

      <div className="stat-row" style={{ marginBottom: '1.25rem' }}>
        <div className="stat">
          <span className="stat-label">Open agreements</span>
          <span className="stat-value">{stats.open}</span>
        </div>
        <div className="stat">
          <span className="stat-label">In escrow</span>
          <span className="stat-value gradient-text">
            {formatAmount(stats.locked, mintInfo.decimals)}
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">Released</span>
          <span className="stat-value">
            {formatAmount(stats.released, mintInfo.decimals)}
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">Disputes</span>
          <span className="stat-value">{stats.disputed}</span>
        </div>
      </div>

      <div className="dash-grid">
        <div className="stack">
          <div className="cluster" style={{ justifyContent: 'space-between' }}>
            <div className="cluster" role="tablist" aria-label="Filter agreements">
              {(
                [
                  ['all', 'All'],
                  ['client', 'As client'],
                  ['freelancer', 'As freelancer'],
                ] as const
              ).map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  role="tab"
                  aria-selected={filter === value}
                  className={filter === value ? 'btn btn-sm' : 'btn btn-ghost btn-sm'}
                  onClick={() => setFilter(value)}
                >
                  {label}
                </button>
              ))}
            </div>
            <button
              type="button"
              className="btn btn-ghost btn-sm"
              onClick={() => void load()}
              disabled={loading}
            >
              {loading ? <span className="spin" aria-label="Refreshing" /> : 'Refresh'}
            </button>
          </div>

          {loading && rows.length === 0 ? (
            <div className="contract-list" aria-busy="true">
              {[0, 1, 2].map((index) => (
                <div className="contract-row" key={index}>
                  <div className="stack" style={{ gap: '0.5rem' }}>
                    <div className="skeleton" style={{ height: 14, width: 220 }} />
                    <div className="skeleton" style={{ height: 10, width: 160 }} />
                  </div>
                  <div className="skeleton" style={{ height: 18, width: 70 }} />
                </div>
              ))}
            </div>
          ) : visible.length === 0 ? (
            <div className="empty">
              <p className="eyebrow" style={{ justifyContent: 'center' }}>
                No agreements yet
              </p>
              <p style={{ fontWeight: 500 }}>
                {tracked.length === 0
                  ? 'Nothing tracked in this browser'
                  : 'Nothing matches this filter'}
              </p>
              <p className="help measure">
                Create an agreement, or open one you were invited to by pasting its
                address below.
              </p>
              <Link href="/app/new" className="btn" style={{ marginTop: '0.35rem' }}>
                Start an agreement
              </Link>
            </div>
          ) : (
            <div className="contract-list">
              {visible.map(({ address, view, error }) => (
                <Link
                  key={address}
                  href={`/app/agreements/${address}`}
                  className="contract-row"
                >
                  <div style={{ minWidth: 0 }}>
                    <div className="contract-title">
                      {view
                        ? view.account.client === viewer
                          ? `With ${shortenAddress(view.account.freelancer, 5)}`
                          : `From ${shortenAddress(view.account.client, 5)}`
                        : 'Unreadable agreement'}
                    </div>
                    <div className="contract-meta">
                      <span>{shortenAddress(address, 6)}</span>
                      <span>
                        {view ? `${view.account.milestoneCount} milestones` : '—'}
                      </span>
                      <span>
                        {view
                          ? view.account.client === viewer
                            ? 'client'
                            : 'freelancer'
                          : 'unknown role'}
                      </span>
                      {view && view.account.acceptedAt > 0n ? (
                        <span>accepted {relativeTime(view.account.acceptedAt)}</span>
                      ) : null}
                      {error ? (
                        <span style={{ color: 'var(--bad)' }}>{error}</span>
                      ) : null}
                    </div>
                  </div>
                  <div style={{ display: 'grid', gap: 6, justifyItems: 'end' }}>
                    <span className="contract-amount">
                      {view
                        ? `${formatAmount(view.account.totalAmount, mintInfo.decimals)} ${mintInfo.symbol}`
                        : '—'}
                    </span>
                    {view ? <AgreementStatus state={view.account.state} /> : null}
                  </div>
                </Link>
              ))}
            </div>
          )}
        </div>

        <div className="stack">
          <form className="card card-pad stack" onSubmit={submitLookup}>
            <h2 className="h3">Open by address</h2>
            <p className="help">
              Invited to an agreement? Paste its address. Nothing is stored on a server.
            </p>
            <div className="field">
              <input
                className="input mono"
                placeholder="Agreement address"
                value={lookup}
                onChange={(event) => setLookup(event.target.value)}
                spellCheck={false}
                aria-label="Agreement address"
              />
              {lookupError ? <span className="field-error">{lookupError}</span> : null}
            </div>
            <button type="submit" className="btn btn-secondary">
              Open
            </button>
          </form>

          <div className="card card-pad stack">
            <h2 className="h3">Network</h2>
            <div className="kv">
              <div className="kv-row">
                <span className="kv-key">Cluster</span>
                <span className="kv-val">{CLUSTER}</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Program</span>
                <span className="kv-val">{shortenAddress(PROGRAM_ID, 6)}</span>
              </div>
              <div className="kv-row">
                <span className="kv-key">Settlement mint</span>
                <span className="kv-val">{mintInfo.symbol}</span>
              </div>
            </div>
            <p className="help">
              OpenWork has no backend in the money path. If this page is up, you can still
              release funds with any Solana wallet.
            </p>
          </div>

          <div className="card card-pad stack">
            <h2 className="h3">Recent receipts</h2>
            {listReceipts(undefined, viewer).length === 0 ? (
              <p className="help">Transactions you submit appear here.</p>
            ) : (
              <div className="stack" style={{ gap: 0 }}>
                {listReceipts(undefined, viewer)
                  .slice(0, 6)
                  .map((receipt) => (
                    <div className="receipt" key={receipt.signature}>
                      <div>
                        <div className="receipt-kind">{receipt.kind}</div>
                        <div className="receipt-meta">
                          {shortenAddress(receipt.agreement, 6)} ·{' '}
                          {relativeTime(Math.floor(receipt.at / 1000))}
                        </div>
                      </div>
                      <a
                        className="tiny mono"
                        href={`https://explorer.solana.com/tx/${receipt.signature}?cluster=${CLUSTER}`}
                        target="_blank"
                        rel="noreferrer"
                      >
                        explorer ↗
                      </a>
                    </div>
                  ))}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
