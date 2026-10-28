'use client';

/**
 * Cross-chain funding screen.
 *
 * This page is deliberately explicit about what is simulated. The Devnet quote
 * comes from `MockIntentAdapter`: the shape matches the NEAR 1Click API, but no
 * bridge is involved and no funds move. Even in Mainnet mode the flow is
 * two-step — routed value lands in the *client's own wallet*, then the client
 * signs a separate `fund_milestone` transaction. OpenWork never holds the
 * bridged funds.
 */
import Link from 'next/link';
import { useSearchParams } from 'next/navigation';
import { Suspense, useCallback, useEffect, useMemo, useRef, useState } from 'react';

import { useWallet } from '@/components/wallet-provider';
import {
  ApiRequestError,
  ApiUnavailableError,
  apiLive,
  createQuote,
  getQuote,
  listContracts,
  type ApiContract,
  type ApiQuote,
} from '@/lib/api';
import {
  CLUSTER,
  DEFAULT_MINT,
  DEFAULT_MINT_SYMBOL,
  KNOWN_MINTS,
  ORIGIN_ASSET_EXAMPLES,
  destinationAssetId,
} from '@/lib/config';
import { formatAmount, parseAmount, shortenAddress } from '@/lib/format';
import { restoreSession, signInWithSolana } from '@/lib/session';
import { formatTimestamp } from '@/lib/format';

const STATUS_COPY: Record<string, string> = {
  requested: 'Quote requested',
  quoted: 'Waiting for your deposit',
  source_transaction_detected: 'Origin transaction seen',
  routing: 'Routing to Solana',
  arrived_in_client_wallet: 'Arrived in your wallet',
  escrow_funding_pending: 'Ready for you to fund the milestone',
  escrow_funded: 'Milestone funded',
  failed: 'Routing failed',
  expired: 'Quote expired',
  refund_pending: 'Refund pending',
  refunded: 'Refunded',
};

const ACTIVE_STATUSES = new Set([
  'requested',
  'quoted',
  'source_transaction_detected',
  'routing',
  'arrived_in_client_wallet',
  'escrow_funding_pending',
]);

export default function FundPage() {
  return (
    <Suspense
      fallback={
        <div className="shell" style={{ paddingBlock: '3rem' }}>
          <div className="stack">
            <div className="skeleton" style={{ height: 28, width: 260 }} />
            <div className="skeleton" style={{ height: 180 }} />
          </div>
        </div>
      }
    >
      <FundFlow />
    </Suspense>
  );
}

function FundFlow() {
  const params = useSearchParams();
  const agreementParam = params.get('agreement');
  const { account, wallet, open } = useWallet();

  const [apiOnline, setApiOnline] = useState<boolean | null>(null);
  const [sessionWallet, setSessionWallet] = useState<string | null>(null);
  const [contracts, setContracts] = useState<readonly ApiContract[]>([]);
  const [contractId, setContractId] = useState<string>('');
  const [originAsset, setOriginAsset] = useState(ORIGIN_ASSET_EXAMPLES[0]!.id);
  const [amount, setAmount] = useState('');
  const [quote, setQuote] = useState<ApiQuote | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [now, setNow] = useState(() => Date.now());
  const pollRef = useRef<number | null>(null);

  const mintInfo = KNOWN_MINTS[DEFAULT_MINT] ?? {
    symbol: DEFAULT_MINT_SYMBOL,
    decimals: 6,
  };

  const origin = useMemo(
    () => ORIGIN_ASSET_EXAMPLES.find((item) => item.id === originAsset)!,
    [originAsset],
  );

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const online = await apiLive();
      if (cancelled) return;
      setApiOnline(online);
      if (!online) return;
      const session = await restoreSession();
      if (!cancelled && session) setSessionWallet(session.user.wallet);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!sessionWallet) return;
    let cancelled = false;
    void listContracts()
      .then((rows) => {
        if (cancelled) return;
        setContracts(rows);
        if (agreementParam) {
          const match = rows.find((row) => row.agreementPda === agreementParam);
          if (match) setContractId(match.id);
        }
      })
      .catch(() => {
        if (!cancelled) setContracts([]);
      });
    return () => {
      cancelled = true;
    };
  }, [sessionWallet, agreementParam]);

  // Poll a live quote until it reaches a terminal state or expires.
  useEffect(() => {
    if (!quote || !ACTIVE_STATUSES.has(quote.status)) return;
    let cancelled = false;
    const tick = async () => {
      try {
        const next = await getQuote(quote.id);
        if (!cancelled) setQuote(next);
      } catch {
        // A transient poll failure is not a status change; the next tick retries.
      }
    };
    pollRef.current = window.setInterval(() => void tick(), 5000);
    return () => {
      cancelled = true;
      if (pollRef.current) window.clearInterval(pollRef.current);
      pollRef.current = null;
    };
  }, [quote]);

  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);

  const signIn = useCallback(async () => {
    if (!wallet || !account) {
      open();
      return;
    }
    setBusy('Signing in');
    setError(null);
    try {
      const session = await signInWithSolana(wallet, account);
      setSessionWallet(session.user.wallet);
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : 'Could not sign in to the indexer',
      );
    } finally {
      setBusy(null);
    }
  }, [wallet, account, open]);

  async function requestQuote() {
    if (!account) {
      open();
      return;
    }
    if (!sessionWallet) {
      setError('Sign in first: quotes are stored against your wallet.');
      return;
    }
    const baseUnits = parseAmount(amount, origin.decimals);
    if (baseUnits === null || baseUnits <= 0n) {
      setError('Enter an amount greater than zero.');
      return;
    }

    setBusy('Requesting quote');
    setError(null);
    try {
      const next = await createQuote({
        contractId: contractId || undefined,
        invoice: `openwork-${Date.now().toString(36)}`,
        amount: baseUnits.toString(),
        originAsset: origin.id,
        destinationAsset: destinationAssetId(DEFAULT_MINT),
        recipient: account.address,
        refundTo: account.address,
        deadlineSeconds: 600,
        slippageBps: 100,
      });
      setQuote(next);
    } catch (cause) {
      if (cause instanceof ApiUnavailableError) {
        setError('The metadata API went away. Chain actions still work.');
      } else if (cause instanceof ApiRequestError) {
        setError(cause.message);
      } else {
        setError(cause instanceof Error ? cause.message : 'Quote failed');
      }
    } finally {
      setBusy(null);
    }
  }

  const selectedContract = contracts.find((row) => row.id === contractId) ?? null;
  const expiresIn = quote
    ? Math.max(0, Math.floor((new Date(quote.expiresAt).getTime() - now) / 1000))
    : 0;

  return (
    <div className="shell">
      <div className="dash-head">
        <div className="stack" style={{ gap: '0.35rem' }}>
          <p className="eyebrow">Funding · {CLUSTER}</p>
          <h1 className="h1">Fund from another chain</h1>
        </div>
        <Link href="/app" className="btn btn-ghost btn-sm">
          Back to ledger
        </Link>
      </div>

      <div className="callout callout-warn" style={{ marginBottom: '1.25rem' }}>
        <strong className="small">Devnet simulation</strong>
        <p className="help">
          Quotes on this deployment come from a local mock adapter. The request and
          response shapes match the NEAR 1Click API, but nothing bridges and no value
          moves. A quote marked <span className="mono">simulated</span> is never treated
          as a payment. The real adapter is compiled behind a mainnet-only flag.
        </p>
      </div>

      {!account ? (
        <div className="empty">
          <p className="eyebrow" style={{ justifyContent: 'center' }}>
            Wallet required
          </p>
          <h2 className="h3">Connect the client wallet</h2>
          <p className="help measure">
            Routed funds always land in the client&rsquo;s own wallet. OpenWork never
            routes value to a third party, so the recipient must be the wallet that signs.
          </p>
          <button type="button" className="btn" onClick={open}>
            Connect wallet
          </button>
        </div>
      ) : (
        <div className="detail-grid">
          <div className="stack">
            <div className="card card-pad stack">
              <div className="cluster" style={{ justifyContent: 'space-between' }}>
                <h2 className="h3">1 · Request a quote</h2>
                <span className="pill">
                  <span className="pill-dot" />
                  {apiOnline === null
                    ? 'checking API'
                    : apiOnline
                      ? 'API reachable'
                      : 'API offline'}
                </span>
              </div>

              {apiOnline === false ? (
                <div className="callout callout-bad small">
                  The metadata API is not running, so quotes cannot be issued. Escrow
                  funding from a wallet that already holds {mintInfo.symbol} still works —
                  open the agreement instead.
                </div>
              ) : null}

              {apiOnline && !sessionWallet ? (
                <div className="callout">
                  <strong className="small">Sign in to store the quote</strong>
                  <p className="help">
                    A signature over a text message proves wallet ownership. It is not a
                    transaction and cannot move funds.
                  </p>
                  <button
                    type="button"
                    className="btn btn-secondary btn-sm"
                    onClick={() => void signIn()}
                    disabled={busy !== null}
                  >
                    {busy === 'Signing in'
                      ? 'Waiting for wallet…'
                      : 'Sign in with Solana'}
                  </button>
                </div>
              ) : null}

              {sessionWallet ? (
                <>
                  {contracts.length > 0 ? (
                    <div className="field">
                      <label className="label" htmlFor="contract">
                        Link to agreement
                        <span className="label-hint">optional</span>
                      </label>
                      <select
                        id="contract"
                        className="select"
                        value={contractId}
                        onChange={(event) => setContractId(event.target.value)}
                      >
                        <option value="">No agreement — just a quote</option>
                        {contracts.map((contract) => (
                          <option key={contract.id} value={contract.id}>
                            {contract.title} · {shortenAddress(contract.agreementPda, 5)}
                          </option>
                        ))}
                      </select>
                    </div>
                  ) : null}

                  <div className="field">
                    <label className="label" htmlFor="origin">
                      Origin asset
                      <span className="label-hint">1Click identifier</span>
                    </label>
                    <select
                      id="origin"
                      className="select"
                      value={originAsset}
                      onChange={(event) => setOriginAsset(event.target.value)}
                    >
                      {ORIGIN_ASSET_EXAMPLES.map((asset) => (
                        <option key={asset.id} value={asset.id}>
                          {asset.label}
                        </option>
                      ))}
                    </select>
                    <span className="help mono tiny">{origin.id}</span>
                  </div>

                  <div className="field">
                    <label className="label" htmlFor="amount">
                      Amount
                      <span className="label-hint">{origin.symbol}</span>
                    </label>
                    <input
                      id="amount"
                      className="input mono"
                      inputMode="decimal"
                      value={amount}
                      onChange={(event) => setAmount(event.target.value)}
                      placeholder={origin.decimals === 6 ? '250.00' : '0.050'}
                    />
                  </div>

                  <div className="field">
                    <span className="label">Destination</span>
                    <span className="help mono tiny">
                      {mintInfo.symbol} · {shortenAddress(DEFAULT_MINT, 6)} ·{' '}
                      {account.address === sessionWallet
                        ? 'your wallet'
                        : 'sign-in mismatch'}
                    </span>
                  </div>

                  <button
                    type="button"
                    className="btn"
                    onClick={() => void requestQuote()}
                    disabled={busy !== null}
                  >
                    {busy === 'Requesting quote' ? 'Requesting…' : 'Request quote'}
                  </button>
                </>
              ) : null}

              {error ? (
                <div className="callout callout-bad small" role="alert">
                  {error}
                </div>
              ) : null}
            </div>

            {quote ? (
              <div className="card card-pad stack">
                <div className="cluster" style={{ justifyContent: 'space-between' }}>
                  <h2 className="h3">2 · Send the deposit</h2>
                  <span className={quote.simulated ? 'pill pill-warn' : 'pill pill-good'}>
                    <span className="pill-dot" />
                    {quote.simulated ? 'simulated' : 'live'}
                  </span>
                </div>

                <div className="callout callout-warn small">
                  <strong className="small">Simulation — do not send funds</strong>
                  <p className="help">
                    This deposit address is generated locally from your invoice and
                    amount. It is not a real 1Click deposit address and no chain is
                    watching it.
                  </p>
                </div>

                <div className="kv">
                  <div className="kv-row">
                    <span className="kv-key">Deposit address</span>
                    <span className="kv-val mono tiny">{quote.depositAddress}</span>
                  </div>
                  {quote.depositMemo ? (
                    <div className="kv-row">
                      <span className="kv-key">Memo</span>
                      <span className="kv-val mono tiny">{quote.depositMemo}</span>
                    </div>
                  ) : null}
                  <div className="kv-row">
                    <span className="kv-key">Amount in</span>
                    <span className="kv-val">
                      {formatAmount(BigInt(quote.amountIn), origin.decimals)}{' '}
                      {origin.symbol}
                    </span>
                  </div>
                  <div className="kv-row">
                    <span className="kv-key">Amount out</span>
                    <span className="kv-val">
                      {quote.amountOut
                        ? `${formatAmount(BigInt(quote.amountOut), mintInfo.decimals)} ${mintInfo.symbol}`
                        : 'quoted at execution'}
                    </span>
                  </div>
                  <div className="kv-row">
                    <span className="kv-key">Status</span>
                    <span className="kv-val">
                      {STATUS_COPY[quote.status] ?? quote.status}
                    </span>
                  </div>
                  <div className="kv-row">
                    <span className="kv-key">Deadline</span>
                    <span className="kv-val">
                      {expiresIn > 0
                        ? `${Math.floor(expiresIn / 60)}m ${expiresIn % 60}s left`
                        : `expired ${formatTimestamp(BigInt(Math.floor(new Date(quote.expiresAt).getTime() / 1000)))}`}
                    </span>
                  </div>
                  <div className="kv-row">
                    <span className="kv-key">Provider</span>
                    <span className="kv-val mono tiny">
                      {quote.provider} · {quote.mode}
                    </span>
                  </div>
                </div>

                <p className="help">
                  Status is polled from the API every five seconds. The mock adapter stops
                  at <span className="mono">escrow_funding_pending</span>: it never
                  reports that a bridge delivered value, because no bridge is running.
                </p>
              </div>
            ) : null}
          </div>

          <div className="stack">
            <div className="card card-pad stack">
              <h2 className="h3">3 · Fund the milestone</h2>
              <p className="help">
                Routed funds arrive in your wallet, not in escrow. Funding a milestone is
                a separate transaction you sign, and it moves the exact milestone amount
                into the agreement vault.
              </p>
              <div className="kv">
                <div className="kv-row">
                  <span className="kv-key">Settlement mint</span>
                  <span className="kv-val">{mintInfo.symbol}</span>
                </div>
                <div className="kv-row">
                  <span className="kv-key">Vault owner</span>
                  <span className="kv-val">agreement PDA</span>
                </div>
                <div className="kv-row">
                  <span className="kv-key">Platform fee</span>
                  <span className="kv-val">0%</span>
                </div>
              </div>
              {selectedContract ? (
                <Link
                  href={`/app/agreements/${selectedContract.agreementPda}`}
                  className="btn btn-secondary btn-sm"
                >
                  Open {selectedContract.title}
                </Link>
              ) : agreementParam ? (
                <Link
                  href={`/app/agreements/${agreementParam}`}
                  className="btn btn-secondary btn-sm"
                >
                  Open agreement
                </Link>
              ) : (
                <Link href="/app" className="btn btn-secondary btn-sm">
                  Choose an agreement
                </Link>
              )}
            </div>

            <div className="card card-pad stack">
              <h2 className="h3">What this page cannot do</h2>
              <ul
                className="help"
                style={{ paddingLeft: '1.1rem', display: 'grid', gap: '0.4rem' }}
              >
                <li>It cannot move funds from another chain.</li>
                <li>It cannot fund escrow on your behalf.</li>
                <li>It cannot mark a milestone funded from a quote.</li>
              </ul>
              <p className="help">
                Escrow status is read from the agreement account. A quote that says{' '}
                <span className="mono">escrow_funding_pending</span> is a prompt, not a
                payment.
              </p>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
