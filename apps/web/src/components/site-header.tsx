'use client';

import Link from 'next/link';
import { usePathname } from 'next/navigation';

import { WalletButton } from './wallet-button';

const NAV = [
  { href: '/#how', label: 'How it works' },
  { href: '/security', label: 'Security' },
  { href: '/faq', label: 'FAQ' },
];

export function SiteHeader() {
  const pathname = usePathname();
  const inApp = pathname?.startsWith('/app');

  return (
    <header className="header">
      <div className="header-inner">
        <Link href="/" className="wordmark" aria-label="OpenWork home">
          <span className="wordmark-mark" aria-hidden="true" />
          <span>
            OpenWork<span className="faint">/</span>escrow
          </span>
        </Link>

        <nav className="nav" aria-label="Primary">
          {NAV.map((item) => (
            <Link key={item.href} href={item.href}>
              {item.label}
            </Link>
          ))}
        </nav>

        <div className="header-actions">
          {inApp ? (
            <WalletButton />
          ) : (
            <>
              <Link href="/app" className="btn btn-secondary btn-sm">
                Open app
              </Link>
              <Link href="/app/new" className="btn btn-sm">
                Start an agreement
              </Link>
            </>
          )}
        </div>
      </div>
    </header>
  );
}
