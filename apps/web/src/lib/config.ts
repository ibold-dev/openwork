/**
 * Runtime configuration.
 *
 * Everything here is devnet-only by default. The mainnet flag exists so the
 * cross-chain adapter cannot be enabled by accident; the UI reads the same flag
 * and labels itself accordingly.
 */
import type { Address } from '@solana/kit';

export const CLUSTER = (process.env.NEXT_PUBLIC_SOLANA_CLUSTER ?? 'devnet') as
  'devnet' | 'localnet' | 'mainnet-beta';

export const RPC_URL =
  process.env.NEXT_PUBLIC_SOLANA_RPC_URL ??
  (CLUSTER === 'localnet' ? 'http://127.0.0.1:8899' : 'https://api.devnet.solana.com');

export const CHAIN = `solana:${CLUSTER}` as const;

export const PROGRAM_ID = 'GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR' as Address;

/**
 * Default settlement mint.
 *
 * This is the test mint created by the Devnet smoke run
 * (`docs/devnet-smoke.json`). It is a plain SPL mint with six decimals, no
 * freeze authority and no market value — the UI never calls it USDC.
 */
export const DEFAULT_MINT = (process.env.NEXT_PUBLIC_DEFAULT_MINT ??
  'ENmQKqTu9FaWJtmwk3nXiNT3GxX6F4k7rZe8t2bqUYT8') as Address;

export const DEFAULT_MINT_SYMBOL = process.env.NEXT_PUBLIC_DEFAULT_MINT_SYMBOL ?? 'tUSDC';

export const IS_MAINNET = CLUSTER === 'mainnet-beta';

/**
 * Origin asset identifiers in NEAR 1Click's `nep141:` notation.
 *
 * These are documentation examples, not a catalogue: the Devnet simulator does
 * not resolve them to real chains and the real adapter is mainnet-gated. The
 * funding screen tells the user exactly that.
 */
export const ORIGIN_ASSET_EXAMPLES: readonly {
  readonly id: string;
  readonly label: string;
  readonly symbol: string;
  readonly decimals: number;
}[] = [
  {
    id: 'nep141:base-eth.omft.near',
    label: 'ETH on Base',
    symbol: 'ETH',
    decimals: 18,
  },
  {
    id: 'nep141:eth.omft.near',
    label: 'ETH on Ethereum',
    symbol: 'ETH',
    decimals: 18,
  },
  {
    id: 'nep141:usdt.tether-token.near',
    label: 'USDT on NEAR',
    symbol: 'USDT',
    decimals: 6,
  },
];

/** Provider-shaped destination identifier for the settlement mint. */
export function destinationAssetId(mint: string): string {
  return `solana:${CLUSTER}/spl:${mint}`;
}

/** API is optional: the app degrades to chain-only mode without it. */
export const API_URL = '/api/proxy';

export const EXPLORER_BASE = `https://explorer.solana.com`;

export function explorerTx(signature: string): string {
  return `${EXPLORER_BASE}/tx/${signature}?cluster=${CLUSTER}`;
}

export function explorerAddress(address: string): string {
  return `${EXPLORER_BASE}/address/${address}?cluster=${CLUSTER}`;
}

export const KNOWN_MINTS: Record<string, { symbol: string; decimals: number }> = {
  [DEFAULT_MINT]: { symbol: DEFAULT_MINT_SYMBOL, decimals: 6 },
  // Devnet USDC used by several faucets.
  '4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU': {
    symbol: 'USDC',
    decimals: 6,
  },
};

/** Rent and fee figures shown in the UI, in lamports. */
export const BASE_TX_FEE_LAMPORTS = 5_000;

/** v1 takes no platform fee. Immutable: the program has no fee instruction. */
export const PLATFORM_FEE_BPS = 0;
