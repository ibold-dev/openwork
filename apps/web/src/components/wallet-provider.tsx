'use client';

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';
import { AnimatePresence, motion, useReducedMotion } from 'motion/react';

import { CHAIN } from '@/lib/config';
import { shortenAddress } from '@/lib/format';
import {
  connectWallet,
  findWallet,
  startWalletDiscovery,
  subscribeWallets,
  supportsSignTransaction,
  type StandardWallet,
  type WalletAccount,
} from '@/lib/wallet';

interface WalletContextValue {
  readonly wallets: readonly StandardWallet[];
  readonly wallet: StandardWallet | null;
  readonly account: WalletAccount | null;
  readonly connecting: string | null;
  readonly error: string | null;
  readonly open: () => void;
  readonly close: () => void;
  readonly connect: (name: string) => Promise<void>;
  readonly disconnect: () => void;
}

const WalletContext = createContext<WalletContextValue | null>(null);

const STORAGE_KEY = 'openwork.wallet.v1';

export function WalletProvider({ children }: { children: ReactNode }) {
  const [wallets, setWallets] = useState<readonly StandardWallet[]>([]);
  const [wallet, setWallet] = useState<StandardWallet | null>(null);
  const [account, setAccount] = useState<WalletAccount | null>(null);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const reduceMotion = useReducedMotion();

  useEffect(() => {
    startWalletDiscovery();
    const unsubscribe = subscribeWallets(setWallets);
    return () => {
      unsubscribe();
    };
  }, []);

  const connect = useCallback(async (name: string) => {
    const target = findWallet(name);
    if (!target) {
      setError(`${name} is not available in this browser`);
      return;
    }
    if (!supportsSignTransaction(target)) {
      setError(`${name} cannot sign Solana transactions`);
      return;
    }
    setConnecting(name);
    setError(null);
    try {
      const accounts = await connectWallet(target);
      const primary =
        accounts.find((candidate) => candidate.chains.includes(CHAIN)) ?? accounts[0]!;
      setWallet(target);
      setAccount(primary);
      setModalOpen(false);
      window.localStorage.setItem(STORAGE_KEY, name);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : 'Connection failed');
    } finally {
      setConnecting(null);
    }
  }, []);

  const disconnect = useCallback(() => {
    setWallet(null);
    setAccount(null);
    setError(null);
    window.localStorage.removeItem(STORAGE_KEY);
  }, []);

  // Reconnect to the previously used wallet once it registers.
  useEffect(() => {
    if (wallet || wallets.length === 0) return;
    const name = window.localStorage.getItem(STORAGE_KEY);
    if (!name) return;
    const target = wallets.find((candidate) => candidate.name === name);
    if (!target) return;
    const feature = target.features['standard:connect'] as
      | {
          connect(input?: {
            silent?: boolean;
          }): Promise<{ accounts: readonly WalletAccount[] }>;
        }
      | undefined;
    if (!feature) return;
    let cancelled = false;
    void feature
      .connect({ silent: true })
      .then(({ accounts }) => {
        if (cancelled || accounts.length === 0) return;
        setWallet(target);
        setAccount(accounts[0]!);
      })
      .catch(() => {
        // Silent reconnect is best-effort; the user can connect manually.
      });
    return () => {
      cancelled = true;
    };
  }, [wallet, wallets]);

  const value = useMemo<WalletContextValue>(
    () => ({
      wallets,
      wallet,
      account,
      connecting,
      error,
      open: () => setModalOpen(true),
      close: () => setModalOpen(false),
      connect,
      disconnect,
    }),
    [wallets, wallet, account, connecting, error, connect, disconnect],
  );

  return (
    <WalletContext.Provider value={value}>
      {children}
      <AnimatePresence>
        {modalOpen ? (
          <motion.div
            className="modal-backdrop"
            initial={reduceMotion ? false : { opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={reduceMotion ? undefined : { opacity: 0 }}
            transition={{ duration: 0.18 }}
            onClick={() => setModalOpen(false)}
            role="presentation"
          >
            <motion.div
              className="modal"
              role="dialog"
              aria-modal="true"
              aria-labelledby="wallet-modal-title"
              initial={reduceMotion ? false : { opacity: 0, y: 12, scale: 0.985 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={reduceMotion ? undefined : { opacity: 0, y: 8, scale: 0.99 }}
              transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
              onClick={(event) => event.stopPropagation()}
            >
              <div className="cluster" style={{ justifyContent: 'space-between' }}>
                <h2 id="wallet-modal-title" className="h3">
                  Connect a wallet
                </h2>
                <span className="pill">{CHAIN}</span>
              </div>
              <p className="small muted">
                OpenWork never asks for a seed phrase or private key. Your wallet signs
                each transaction and keeps custody at all times.
              </p>

              {error ? (
                <div className="callout callout-bad small" role="alert">
                  {error}
                </div>
              ) : null}

              <div className="stack" style={{ gap: '0.5rem' }}>
                {wallets.length === 0 ? (
                  <div className="empty" style={{ padding: '1.75rem 1rem' }}>
                    <p className="small" style={{ fontWeight: 500 }}>
                      No Solana wallet detected
                    </p>
                    <p className="help">
                      Install Phantom, Backpack, Solflare or any Wallet
                      Standard-compatible wallet, then reload this page.
                    </p>
                  </div>
                ) : (
                  wallets.map((candidate) => (
                    <button
                      key={candidate.name}
                      type="button"
                      className="wallet-option"
                      onClick={() => void connect(candidate.name)}
                      disabled={connecting !== null}
                    >
                      {/* eslint-disable-next-line @next/next/no-img-element */}
                      <img src={candidate.icon} alt="" width={26} height={26} />
                      <span style={{ flex: 1 }}>
                        <span style={{ display: 'block', fontWeight: 500 }}>
                          {candidate.name}
                        </span>
                        <span className="help">
                          {candidate.accounts.length > 0
                            ? shortenAddress(candidate.accounts[0]!.address, 5)
                            : 'Ready to connect'}
                        </span>
                      </span>
                      {connecting === candidate.name ? (
                        <span className="spin" aria-label="Connecting" />
                      ) : (
                        <span className="mono tiny faint">connect →</span>
                      )}
                    </button>
                  ))
                )}
              </div>

              <div className="cluster" style={{ justifyContent: 'flex-end' }}>
                <button
                  type="button"
                  className="btn btn-ghost btn-sm"
                  onClick={() => setModalOpen(false)}
                >
                  Close
                </button>
              </div>
            </motion.div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </WalletContext.Provider>
  );
}

export function useWallet(): WalletContextValue {
  const context = useContext(WalletContext);
  if (!context) {
    throw new Error('useWallet must be used inside <WalletProvider>');
  }
  return context;
}
