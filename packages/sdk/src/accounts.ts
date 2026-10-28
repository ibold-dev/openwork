/**
 * Decoders for on-chain escrow accounts.
 *
 * Layouts mirror `programs/openwork-escrow/src/state.rs` byte for byte.
 * `tests/unit.test.ts` asserts the fixed sizes; the integration suite asserts
 * the decoders against accounts written by the real program.
 */
import type { Address } from '@solana/kit';

import { ACCOUNT_DISCRIMINATORS } from './discriminators.js';
import { base58Encode, base58Decode } from './instructions.js';

export type AgreementState = 'draft' | 'active' | 'completed' | 'cancelled';

export type MilestoneState =
  'pending' | 'funded' | 'submitted' | 'released' | 'refunded' | 'disputed';

const AGREEMENT_STATES: readonly AgreementState[] = [
  'draft',
  'active',
  'completed',
  'cancelled',
];

const MILESTONE_STATES: readonly MilestoneState[] = [
  'pending',
  'funded',
  'submitted',
  'released',
  'refunded',
  'disputed',
];

export interface AgreementAccount {
  readonly schemaVersion: number;
  readonly bump: number;
  readonly state: AgreementState;
  readonly client: Address;
  readonly freelancer: Address;
  readonly mint: Address;
  readonly agreementNonce: bigint;
  readonly termsHash: Uint8Array;
  readonly milestoneCount: number;
  readonly settledCount: number;
  readonly refundedCount: number;
  readonly totalAmount: bigint;
  readonly releasedAmount: bigint;
  readonly createdAt: bigint;
  readonly acceptedAt: bigint;
}

export interface MilestoneAccount {
  readonly index: number;
  readonly bump: number;
  readonly state: MilestoneState;
  readonly agreement: Address;
  readonly amount: bigint;
  readonly deliverableHash: Uint8Array;
  readonly fundedAt: bigint;
  readonly submittedAt: bigint;
  readonly settledAt: bigint;
}

export const AGREEMENT_ACCOUNT_SIZE = 185;
export const MILESTONE_ACCOUNT_SIZE = 108;

export class AccountDecodeError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'AccountDecodeError';
  }
}

class Reader {
  private offset = 0;

  constructor(private readonly data: Uint8Array) {}

  u8(): number {
    this.need(1);
    const value = this.data[this.offset]!;
    this.offset += 1;
    return value;
  }

  u16(): number {
    this.need(2);
    const view = new DataView(this.data.buffer, this.data.byteOffset + this.offset, 2);
    this.offset += 2;
    return view.getUint16(0, true);
  }

  u64(): bigint {
    this.need(8);
    const view = new DataView(this.data.buffer, this.data.byteOffset + this.offset, 8);
    this.offset += 8;
    return view.getBigUint64(0, true);
  }

  i64(): bigint {
    this.need(8);
    const view = new DataView(this.data.buffer, this.data.byteOffset + this.offset, 8);
    this.offset += 8;
    return view.getBigInt64(0, true);
  }

  bytes(length: number): Uint8Array {
    this.need(length);
    const slice = this.data.slice(this.offset, this.offset + length);
    this.offset += length;
    return slice;
  }

  address(): Address {
    return base58Encode(this.bytes(32)) as Address;
  }

  private need(length: number): void {
    if (this.offset + length > this.data.length) {
      throw new AccountDecodeError(
        `unexpected end of account data at offset ${this.offset}`,
      );
    }
  }
}

function checkDiscriminator(data: Uint8Array, expectedHex: string, name: string): void {
  if (data.length < 8) {
    throw new AccountDecodeError(`${name} account data is too short`);
  }
  const actual = Array.from(data.slice(0, 8))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
  if (actual !== expectedHex) {
    throw new AccountDecodeError(
      `${name} discriminator mismatch: ${actual} != ${expectedHex}`,
    );
  }
}

/** Decodes raw account bytes (base64 payload already converted to bytes). */
export function decodeAgreementAccount(data: Uint8Array): AgreementAccount {
  checkDiscriminator(data, ACCOUNT_DISCRIMINATORS.Agreement, 'Agreement');
  const reader = new Reader(data.slice(8));
  const schemaVersion = reader.u8();
  const bump = reader.u8();
  const stateIndex = reader.u8();
  const state = AGREEMENT_STATES[stateIndex];
  if (!state) throw new AccountDecodeError(`unknown agreement state ${stateIndex}`);
  const agreement: AgreementAccount = {
    schemaVersion,
    bump,
    state,
    client: reader.address(),
    freelancer: reader.address(),
    mint: reader.address(),
    agreementNonce: reader.u64(),
    termsHash: reader.bytes(32),
    milestoneCount: reader.u16(),
    settledCount: reader.u16(),
    refundedCount: reader.u16(),
    totalAmount: reader.u64(),
    releasedAmount: reader.u64(),
    createdAt: reader.i64(),
    acceptedAt: reader.i64(),
  };
  if (data.length !== AGREEMENT_ACCOUNT_SIZE) {
    throw new AccountDecodeError(
      `Agreement account size ${data.length} != ${AGREEMENT_ACCOUNT_SIZE}`,
    );
  }
  return agreement;
}

export function decodeMilestoneAccount(data: Uint8Array): MilestoneAccount {
  checkDiscriminator(data, ACCOUNT_DISCRIMINATORS.Milestone, 'Milestone');
  const reader = new Reader(data.slice(8));
  const index = reader.u16();
  const bump = reader.u8();
  const stateIndex = reader.u8();
  const state = MILESTONE_STATES[stateIndex];
  if (!state) throw new AccountDecodeError(`unknown milestone state ${stateIndex}`);
  const milestone: MilestoneAccount = {
    index,
    bump,
    state,
    agreement: reader.address(),
    amount: reader.u64(),
    deliverableHash: reader.bytes(32),
    fundedAt: reader.i64(),
    submittedAt: reader.i64(),
    settledAt: reader.i64(),
  };
  if (data.length !== MILESTONE_ACCOUNT_SIZE) {
    throw new AccountDecodeError(
      `Milestone account size ${data.length} != ${MILESTONE_ACCOUNT_SIZE}`,
    );
  }
  return milestone;
}

/** Decodes the `[base64, "base64"]` tuple returned by `getAccountInfo`. */
export function decodeRpcAccountData(encoded: readonly [string, string]): Uint8Array {
  if (encoded[1] !== 'base64') {
    throw new AccountDecodeError(`unsupported account encoding: ${encoded[1]}`);
  }
  const binary = atob(encoded[0]);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
}

export { base58Decode };
