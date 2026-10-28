import type { Metadata, Viewport } from 'next';
import { GeistMono } from 'geist/font/mono';
import { GeistSans } from 'geist/font/sans';

import { SiteFooter } from '@/components/site-footer';
import { SiteHeader } from '@/components/site-header';
import { WalletProvider } from '@/components/wallet-provider';

import './globals.css';

export const metadata: Metadata = {
  title: {
    default: 'OpenWork — milestone escrow for independent work',
    template: '%s · OpenWork',
  },
  description:
    'OpenWork holds client funds in a Solana program and releases each milestone on approval. Non-custodial, zero platform fee, no admin keys.',
  applicationName: 'OpenWork',
  keywords: ['freelance escrow', 'Solana', 'milestone payments', 'smart contract escrow'],
  openGraph: {
    title: 'OpenWork — milestone escrow for independent work',
    description:
      'Funds sit in a Solana program, not in OpenWork’s account. Release happens when the client approves, or refunds when both parties agree.',
    type: 'website',
  },
  robots: { index: true, follow: true },
  icons: { icon: '/favicon.svg' },
};

export const viewport: Viewport = {
  themeColor: '#f7f5f0',
  colorScheme: 'light',
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" className={`${GeistSans.variable} ${GeistMono.variable}`}>
      <body>
        <WalletProvider>
          <div className="page">
            <a href="#main" className="skip-link">
              Skip to content
            </a>
            <SiteHeader />
            <div className="page-main" id="main">
              {children}
            </div>
            <SiteFooter />
          </div>
        </WalletProvider>
      </body>
    </html>
  );
}
