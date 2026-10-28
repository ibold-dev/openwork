'use client';

import Link from 'next/link';
import { useEffect } from 'react';

/**
 * Route-level error boundary. It never claims a transaction succeeded: the
 * chain state is the only source of truth, and the user is told to re-read it.
 */
export default function ErrorBoundary({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    // Surface the failure in the browser console for support screenshots. The
    // app has no telemetry endpoint, so nothing is sent anywhere.
    console.error(error);
  }, [error]);

  return (
    <div className="shell">
      <div className="empty" style={{ marginBlock: '5rem' }}>
        <p className="eyebrow" style={{ justifyContent: 'center' }}>
          Something broke
        </p>
        <h1 className="h2">This screen could not render</h1>
        <p className="help measure">{error.message || 'An unexpected error occurred.'}</p>
        <p className="help measure">
          Nothing on chain changed because of this. If you had just signed a transaction,
          re-open the agreement to read its current state before retrying.
        </p>
        <div
          className="cluster"
          style={{ justifyContent: 'center', marginTop: '0.5rem' }}
        >
          <button type="button" className="btn" onClick={reset}>
            Try again
          </button>
          <Link href="/app" className="btn btn-secondary">
            Back to the ledger
          </Link>
        </div>
        {error.digest ? (
          <p className="tiny mono faint">reference {error.digest}</p>
        ) : null}
      </div>
    </div>
  );
}
