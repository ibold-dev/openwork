import Link from 'next/link';

import { CLUSTER, PROGRAM_ID } from '@/lib/config';
import { shortenAddress } from '@/lib/format';

export function SiteFooter() {
  return (
    <footer className="footer">
      <div className="shell">
        <div className="footer-grid">
          <div>
            <Link href="/" className="wordmark">
              <span className="wordmark-mark" aria-hidden="true" />
              <span>OpenWork</span>
            </Link>
            <p className="help" style={{ marginTop: '0.75rem', maxWidth: '34ch' }}>
              Milestone escrow for independent work. Funds are held by a Solana program,
              not by OpenWork.
            </p>
            <p className="pill" style={{ marginTop: '0.9rem' }}>
              <span className="pill-dot" />
              {CLUSTER}
            </p>
          </div>

          <div>
            <h4>Product</h4>
            <ul>
              <li>
                <Link href="/app">Dashboard</Link>
              </li>
              <li>
                <Link href="/app/new">New agreement</Link>
              </li>
              <li>
                <Link href="/app/fund">Cross-chain funding</Link>
              </li>
            </ul>
          </div>

          <div>
            <h4>Trust</h4>
            <ul>
              <li>
                <Link href="/security">Security model</Link>
              </li>
              <li>
                <Link href="/#fees">Fees</Link>
              </li>
              <li>
                <Link href="/faq">FAQ</Link>
              </li>
            </ul>
          </div>

          <div>
            <h4>Program</h4>
            <ul>
              <li>
                <a
                  href={`https://explorer.solana.com/address/${PROGRAM_ID}?cluster=${CLUSTER}`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {shortenAddress(PROGRAM_ID, 5)} ↗
                </a>
              </li>
              <li>
                <Link href="/security#invariants">Invariants</Link>
              </li>
              <li>
                <span className="faint">No admin keys</span>
              </li>
            </ul>
          </div>
        </div>

        <div className="footer-bottom">
          <span>© {new Date().getFullYear()} OpenWork</span>
          <span>Escrow only. Not a bank, broker or custodian.</span>
        </div>
      </div>
    </footer>
  );
}
