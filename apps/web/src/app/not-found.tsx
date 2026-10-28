import Link from 'next/link';

export default function NotFound() {
  return (
    <div className="shell">
      <div className="empty" style={{ marginBlock: '5rem' }}>
        <p className="eyebrow" style={{ justifyContent: 'center' }}>
          404
        </p>
        <h1 className="h2">No page at this address</h1>
        <p className="help measure">
          If you followed an agreement link, check that the whole address after
          <span className="mono"> /app/agreements/ </span> was copied. Agreement links
          also carry a <span className="mono">#terms=</span> fragment that some chat
          clients strip.
        </p>
        <div
          className="cluster"
          style={{ justifyContent: 'center', marginTop: '0.5rem' }}
        >
          <Link href="/" className="btn btn-secondary">
            Home
          </Link>
          <Link href="/app" className="btn">
            Open the ledger
          </Link>
        </div>
      </div>
    </div>
  );
}
