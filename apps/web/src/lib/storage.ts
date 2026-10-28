'use client';

/**
 * Browser-local caches.
 *
 * The chain is the source of truth for every escrow fact. These stores only
 * remember (a) which agreements this browser has seen, (b) the human-readable
 * terms whose hash is on-chain, and (c) receipts for transactions this browser
 * submitted. Clearing site data loses convenience, never money.
 */

export interface TrackedAgreement {
  readonly address: string;
  /** Role of the wallet that first opened this agreement in this browser. */
  readonly viewer: string;
  readonly createdAt: number;
}

export interface Receipt {
  readonly signature: string;
  readonly agreement: string;
  readonly kind:
    'create' | 'accept' | 'fund' | 'submit' | 'release' | 'dispute' | 'refund' | 'cancel';
  readonly at: number;
  readonly viewer: string;
}

export interface StoredTerms {
  readonly title: string;
  readonly description: string;
  readonly client: string;
  readonly freelancer: string;
  readonly mint: string;
  readonly milestones: readonly {
    readonly index: number;
    readonly title: string;
    readonly description: string;
    readonly amount: string;
  }[];
}

const AGREEMENTS_KEY = 'openwork.agreements.v1';
const RECEIPTS_KEY = 'openwork.receipts.v1';
const TERMS_KEY = 'openwork.terms.v1';
const NONCE_KEY = 'openwork.nonce.v1';

function readJson<T>(key: string, fallback: T): T {
  if (typeof window === 'undefined') return fallback;
  try {
    const raw = window.localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function writeJson(key: string, value: unknown): void {
  if (typeof window === 'undefined') return;
  try {
    window.localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage can be full or blocked; the app stays usable without the cache.
  }
}

export function listTrackedAgreements(viewer?: string): readonly TrackedAgreement[] {
  const all = readJson<readonly TrackedAgreement[]>(AGREEMENTS_KEY, []);
  const unique = new Map<string, TrackedAgreement>();
  for (const entry of all) {
    if (viewer && entry.viewer !== viewer) continue;
    const existing = unique.get(entry.address);
    if (!existing || entry.createdAt > existing.createdAt) {
      unique.set(entry.address, entry);
    }
  }
  return Array.from(unique.values()).sort((a, b) => b.createdAt - a.createdAt);
}

export function trackAgreement(address: string, viewer: string): void {
  const all = readJson<TrackedAgreement[]>(AGREEMENTS_KEY, []);
  if (all.some((entry) => entry.address === address && entry.viewer === viewer)) {
    return;
  }
  writeJson(AGREEMENTS_KEY, [...all, { address, viewer, createdAt: Date.now() }]);
}

export function untrackAgreement(address: string, viewer: string): void {
  const all = readJson<TrackedAgreement[]>(AGREEMENTS_KEY, []);
  writeJson(
    AGREEMENTS_KEY,
    all.filter((entry) => !(entry.address === address && entry.viewer === viewer)),
  );
}

export function appendReceipt(receipt: Receipt): void {
  const all = readJson<Receipt[]>(RECEIPTS_KEY, []);
  if (all.some((entry) => entry.signature === receipt.signature)) return;
  writeJson(RECEIPTS_KEY, [receipt, ...all].slice(0, 500));
}

export function listReceipts(agreement?: string, viewer?: string): readonly Receipt[] {
  return readJson<readonly Receipt[]>(RECEIPTS_KEY, []).filter(
    (receipt) =>
      (!agreement || receipt.agreement === agreement) &&
      (!viewer || receipt.viewer === viewer),
  );
}

export function saveTerms(termsHash: string, terms: StoredTerms): void {
  const all = readJson<Record<string, StoredTerms>>(TERMS_KEY, {});
  all[termsHash] = terms;
  writeJson(TERMS_KEY, all);
}

export function loadTerms(termsHash: string): StoredTerms | null {
  const all = readJson<Record<string, StoredTerms>>(TERMS_KEY, {});
  return all[termsHash] ?? null;
}

/** Monotonic per-browser nonce, so a client cannot collide with its own PDAs. */
export function nextAgreementNonce(): bigint {
  const current = readJson<{ value: string }>(NONCE_KEY, { value: '0' });
  const next = BigInt(current.value) + 1n;
  writeJson(NONCE_KEY, { value: next.toString() });
  return next;
}
