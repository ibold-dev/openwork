'use client';

/**
 * Root error boundary. This renders when the root layout itself fails, so it
 * must supply its own <html> and <body> and cannot rely on design-system CSS.
 */
export default function GlobalError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <html lang="en">
      <body
        style={{
          margin: 0,
          minHeight: '100vh',
          display: 'grid',
          placeItems: 'center',
          background: '#f7f5ef',
          color: '#1c1d1a',
          fontFamily: 'ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif',
          padding: '2rem',
        }}
      >
        <main style={{ maxWidth: '32rem' }}>
          <p
            style={{
              textTransform: 'uppercase',
              letterSpacing: '0.14em',
              fontSize: '0.7rem',
              color: '#6c6d66',
            }}
          >
            OpenWork
          </p>
          <h1 style={{ fontSize: '1.6rem', lineHeight: 1.25 }}>
            The app failed to start
          </h1>
          <p style={{ color: '#4a4b45', lineHeight: 1.6 }}>
            {error.message || 'An unexpected error occurred while loading.'}
          </p>
          <p style={{ color: '#4a4b45', lineHeight: 1.6 }}>
            No on-chain state was changed. Agreements and vaults remain readable with any
            Solana explorer or wallet.
          </p>
          <button
            type="button"
            onClick={reset}
            style={{
              marginTop: '0.75rem',
              padding: '0.6rem 1rem',
              borderRadius: '0.5rem',
              border: '1px solid #1c1d1a',
              background: '#1c1d1a',
              color: '#f7f5ef',
              font: 'inherit',
              cursor: 'pointer',
            }}
          >
            Try again
          </button>
          {error.digest ? (
            <p style={{ color: '#6c6d66', fontSize: '0.75rem' }}>
              reference {error.digest}
            </p>
          ) : null}
        </main>
      </body>
    </html>
  );
}
