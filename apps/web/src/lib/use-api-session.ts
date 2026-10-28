'use client';

/**
 * Optional index session.
 *
 * The metadata API is a convenience layer: it stores titles, descriptions and
 * deliverable notes, and it verifies transaction claims. It never signs and
 * never holds funds, so every feature here degrades to chain-only when the API
 * or the session is missing.
 *
 * A session belongs to a wallet, not to the browser. `contracts` is therefore
 * empty until a wallet has signed in, and the API only returns agreements where
 * that wallet is a party.
 */
import { useCallback, useEffect, useMemo, useState } from 'react';

import { useWallet } from '@/components/wallet-provider';
import {
  ApiUnavailableError,
  apiLive,
  listContracts,
  logout,
  type ApiContract,
  type ApiSession,
} from './api';
import { restoreSession, signInWithSolana } from './session';

export interface ApiSessionState {
  /** True once the liveness probe has answered, whatever the answer was. */
  readonly probed: boolean;
  readonly online: boolean;
  readonly session: ApiSession | null;
  readonly contracts: readonly ApiContract[];
  readonly signingIn: boolean;
  readonly error: string | null;
  /** Finds an indexed agreement by its on-chain address. */
  readonly indexedFor: (agreementPda: string) => ApiContract | null;
  readonly signIn: () => Promise<void>;
  readonly refresh: () => Promise<void>;
}

export function useApiSession(): ApiSessionState {
  const { wallet, account } = useWallet();
  const [probed, setProbed] = useState(false);
  const [online, setOnline] = useState(false);
  const [session, setSession] = useState<ApiSession | null>(null);
  const [contracts, setContracts] = useState<readonly ApiContract[]>([]);
  const [signingIn, setSigningIn] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const live = await apiLive();
    setProbed(true);
    setOnline(live);
    if (!live) {
      setSession(null);
      setContracts([]);
      return;
    }
    const restored = await restoreSession();
    if (restored && account && restored.user.wallet !== account.address) {
      // The cookie belongs to a wallet that is no longer connected. Drop it so
      // a claim is never filed under the wrong party.
      await logout();
      setSession(null);
      setContracts([]);
      return;
    }
    setSession(restored);
    if (!restored) {
      setContracts([]);
      return;
    }
    try {
      setContracts(await listContracts());
    } catch {
      // A stale cookie is not a reason to break the page.
      setContracts([]);
    }
  }, [account]);

  useEffect(() => {
    void refresh();
  }, [refresh, account?.address]);

  const signIn = useCallback(async () => {
    if (!wallet || !account) return;
    setSigningIn(true);
    setError(null);
    try {
      const next = await signInWithSolana(wallet, account);
      setSession(next);
      setContracts(await listContracts());
    } catch (cause) {
      setError(
        cause instanceof ApiUnavailableError
          ? 'The metadata API is not reachable. Chain actions still work.'
          : cause instanceof Error
            ? cause.message
            : 'Could not sign in to the indexer',
      );
    } finally {
      setSigningIn(false);
    }
  }, [wallet, account]);

  const indexed = useMemo(() => {
    const map = new Map<string, ApiContract>();
    for (const contract of contracts) {
      map.set(contract.agreementPda.toLowerCase(), contract);
    }
    return map;
  }, [contracts]);

  const indexedFor = useCallback(
    (agreementPda: string) => indexed.get(agreementPda.toLowerCase()) ?? null,
    [indexed],
  );

  return {
    probed,
    online,
    session,
    contracts,
    signingIn,
    error,
    indexedFor,
    signIn,
    refresh,
  };
}
