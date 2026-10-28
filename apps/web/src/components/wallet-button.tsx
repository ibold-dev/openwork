'use client';

import { useWallet } from './wallet-provider';
import { shortenAddress } from '@/lib/format';

export function WalletButton({ size = 'md' }: { size?: 'md' | 'sm' }) {
  const { account, wallet, open, disconnect } = useWallet();

  if (!account || !wallet) {
    return (
      <button
        type="button"
        className={size === 'sm' ? 'btn btn-secondary btn-sm' : 'btn'}
        onClick={open}
      >
        Connect wallet
      </button>
    );
  }

  return (
    <div className="cluster" style={{ gap: '0.35rem' }}>
      <button
        type="button"
        className="wallet-btn"
        onClick={disconnect}
        title={`Disconnect ${wallet.name}`}
      >
        <span className="wallet-avatar" aria-hidden="true">
          {wallet.name.slice(0, 1)}
        </span>
        <span>{shortenAddress(account.address, 4)}</span>
      </button>
    </div>
  );
}
