/**
 * Program address, seeds and binary encoders.
 *
 * Every value here is part of the on-chain ABI. Tests assert determinism.
 */
import { type Address, getAddressEncoder, getProgramDerivedAddress } from '@solana/kit';

/** Deployed Devnet program. Mainnet uses a separate program id (docs/deployments.md). */
export const OPENWORK_ESCROW_PROGRAM_ID =
  'GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR' as Address;

export const SYSTEM_PROGRAM_ID = '11111111111111111111111111111111' as Address;
export const TOKEN_PROGRAM_ID = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA' as Address;
export const ASSOCIATED_TOKEN_PROGRAM_ID =
  'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL' as Address;

export const AGREEMENT_SEED = new TextEncoder().encode('agreement');
export const MILESTONE_SEED = new TextEncoder().encode('milestone');
export const VAULT_AUTHORITY_SEED = new TextEncoder().encode('vault-authority');

export const MAX_MILESTONES = 12;
export const SCHEMA_VERSION = 1;

const addressEncoder = getAddressEncoder();

/** Little-endian u16. */
export function u16le(value: number): Uint8Array {
  if (!Number.isInteger(value) || value < 0 || value > 0xffff) {
    throw new RangeError(`u16 out of range: ${value}`);
  }
  const out = new Uint8Array(2);
  new DataView(out.buffer).setUint16(0, value, true);
  return out;
}

/** Little-endian u64 from bigint (never use numbers for money). */
export function u64le(value: bigint): Uint8Array {
  if (value < 0n || value > 0xffff_ffff_ffff_ffffn) {
    throw new RangeError(`u64 out of range: ${value}`);
  }
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, value, true);
  return out;
}

/** Little-endian i64. */
export function i64le(value: bigint): Uint8Array {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigInt64(0, value, true);
  return out;
}

export function concatBytes(...parts: readonly Uint8Array[]): Uint8Array {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

export function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) throw new Error('hex string must be even length');
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

export function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes)
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

/**
 * Agreement PDA: `[b"agreement", client, agreement_nonce_le]`.
 * The nonce is supplied by the client and must be unique per client.
 */
export async function findAgreementPda(client: Address, nonce: bigint): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    seeds: [AGREEMENT_SEED, addressEncoder.encode(client), u64le(nonce)],
  });
  return pda;
}

/** Milestone PDA: `[b"milestone", agreement, milestone_index_le]`. */
export async function findMilestonePda(
  agreement: Address,
  index: number,
): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    seeds: [MILESTONE_SEED, addressEncoder.encode(agreement), u16le(index)],
  });
  return pda;
}

/** Vault authority PDA: `[b"vault-authority", agreement]`. */
export async function findVaultAuthorityPda(agreement: Address): Promise<Address> {
  const [pda] = await getProgramDerivedAddress({
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    seeds: [VAULT_AUTHORITY_SEED, addressEncoder.encode(agreement)],
  });
  return pda;
}
