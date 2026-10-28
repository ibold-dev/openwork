/**
 * Terms sharing without a server.
 *
 * The program stores only `sha256(canonical_terms)`. To accept an agreement the
 * freelancer needs the exact bytes that hash to that value. The client can hand
 * them over in the URL fragment (`#terms=…`), which never leaves the browser and
 * is not sent to any server. The page verifies the fragment against the on-chain
 * hash before enabling "Accept".
 */
import type { TermsInput } from '@openwork/sdk';

export interface SharedTerms {
  readonly canonical: string;
  readonly parsed: TermsInput & { readonly version: number };
}

export async function sha256Hex(input: string | Uint8Array): Promise<string> {
  const bytes = typeof input === 'string' ? new TextEncoder().encode(input) : input;
  // `crypto.subtle` only accepts ArrayBuffer-backed views; the copy makes the
  // backing buffer explicit instead of relying on the caller's allocation.
  const digest = await crypto.subtle.digest(
    'SHA-256',
    bytes.slice() as unknown as ArrayBuffer,
  );
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
}

/** base64url without padding, safe in a URL fragment. */
export function encodeTermsFragment(canonical: string): string {
  const bytes = new TextEncoder().encode(canonical);
  let binary = '';
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export function decodeTermsFragment(value: string): SharedTerms | null {
  try {
    const padded = value.replace(/-/g, '+').replace(/_/g, '/');
    const binary = atob(padded.padEnd(Math.ceil(padded.length / 4) * 4, '='));
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    const canonical = new TextDecoder().decode(bytes);
    const parsed = JSON.parse(canonical) as SharedTerms['parsed'];
    if (parsed.version !== 1) return null;
    if (!Array.isArray(parsed.milestones)) return null;
    return { canonical, parsed };
  } catch {
    return null;
  }
}

/** Reads `#terms=…` from the current URL without a navigation. */
export function readTermsFragment(): SharedTerms | null {
  if (typeof window === 'undefined') return null;
  const hash = window.location.hash.replace(/^#/, '');
  if (!hash) return null;
  const params = new URLSearchParams(hash);
  const terms = params.get('terms');
  return terms ? decodeTermsFragment(terms) : null;
}
