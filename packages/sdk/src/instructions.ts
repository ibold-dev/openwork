/**
 * Instruction builders for the OpenWork escrow program.
 *
 * The builders return plain Kit `Instruction` values. They never sign, never
 * send, and never infer balances: the caller composes and signs the transaction
 * in the user's wallet.
 */
import { AccountRole, type Address, type Instruction } from '@solana/kit';

import { IX_DISCRIMINATORS } from './discriminators.js';
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  OPENWORK_ESCROW_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  concatBytes,
  hexToBytes,
  u16le,
  u64le,
} from './program.js';

function discriminator(name: keyof typeof IX_DISCRIMINATORS): Uint8Array {
  return hexToBytes(IX_DISCRIMINATORS[name]);
}

export interface CreateAgreementArgs {
  readonly client: Address;
  readonly agreement: Address;
  readonly freelancer: Address;
  readonly mint: Address;
  readonly nonce: bigint;
  readonly termsHash: Uint8Array;
}

export function getCreateAgreementInstruction({
  client,
  agreement,
  freelancer,
  mint,
  nonce,
  termsHash,
}: CreateAgreementArgs): Instruction {
  assertBytes(termsHash, 32, 'termsHash');
  return {
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    accounts: [
      { address: client, role: AccountRole.WRITABLE_SIGNER },
      { address: agreement, role: AccountRole.WRITABLE },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
    ],
    data: concatBytes(
      discriminator('create_agreement'),
      u64le(nonce),
      base58Decode(freelancer),
      base58Decode(mint),
      termsHash,
    ),
  };
}

export interface CreateMilestoneArgs {
  readonly client: Address;
  readonly agreement: Address;
  readonly milestone: Address;
  readonly index: number;
  readonly amount: bigint;
}

export function getCreateMilestoneInstruction({
  client,
  agreement,
  milestone,
  index,
  amount,
}: CreateMilestoneArgs): Instruction {
  return {
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    accounts: [
      { address: client, role: AccountRole.WRITABLE_SIGNER },
      { address: agreement, role: AccountRole.WRITABLE },
      { address: milestone, role: AccountRole.WRITABLE },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
    ],
    data: concatBytes(discriminator('create_milestone'), u16le(index), u64le(amount)),
  };
}

export function getAcceptAgreementInstruction({
  freelancer,
  agreement,
  termsHash,
}: {
  readonly freelancer: Address;
  readonly agreement: Address;
  readonly termsHash: Uint8Array;
}): Instruction {
  assertBytes(termsHash, 32, 'termsHash');
  return {
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    accounts: [
      { address: freelancer, role: AccountRole.READONLY_SIGNER },
      { address: agreement, role: AccountRole.WRITABLE },
    ],
    data: concatBytes(discriminator('accept_agreement'), termsHash),
  };
}

export function getCancelDraftAgreementInstruction({
  client,
  agreement,
}: {
  readonly client: Address;
  readonly agreement: Address;
}): Instruction {
  return {
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    accounts: [
      { address: client, role: AccountRole.READONLY_SIGNER },
      { address: agreement, role: AccountRole.WRITABLE },
    ],
    data: discriminator('cancel_draft_agreement'),
  };
}

export interface FundMilestoneAccounts {
  readonly client: Address;
  readonly agreement: Address;
  readonly milestone: Address;
  readonly mint: Address;
  readonly clientTokenAccount: Address;
  readonly vault: Address;
  readonly vaultAuthority: Address;
}

export function getFundMilestoneInstruction({
  client,
  agreement,
  milestone,
  mint,
  clientTokenAccount,
  vault,
  vaultAuthority,
}: FundMilestoneAccounts): Instruction {
  return {
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    accounts: [
      { address: client, role: AccountRole.WRITABLE_SIGNER },
      { address: agreement, role: AccountRole.READONLY },
      { address: milestone, role: AccountRole.WRITABLE },
      { address: mint, role: AccountRole.READONLY },
      { address: clientTokenAccount, role: AccountRole.WRITABLE },
      { address: vault, role: AccountRole.WRITABLE },
      { address: vaultAuthority, role: AccountRole.READONLY },
      { address: TOKEN_PROGRAM_ID, role: AccountRole.READONLY },
      { address: ASSOCIATED_TOKEN_PROGRAM_ID, role: AccountRole.READONLY },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
    ],
    data: discriminator('fund_milestone'),
  };
}

export function getSubmitMilestoneInstruction({
  freelancer,
  agreement,
  milestone,
  deliverableHash,
}: {
  readonly freelancer: Address;
  readonly agreement: Address;
  readonly milestone: Address;
  readonly deliverableHash: Uint8Array;
}): Instruction {
  assertBytes(deliverableHash, 32, 'deliverableHash');
  return {
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    accounts: [
      { address: freelancer, role: AccountRole.READONLY_SIGNER },
      { address: agreement, role: AccountRole.READONLY },
      { address: milestone, role: AccountRole.WRITABLE },
    ],
    data: concatBytes(discriminator('submit_milestone'), deliverableHash),
  };
}

export function getApproveAndReleaseInstruction({
  client,
  agreement,
  milestone,
  freelancer,
  mint,
  freelancerTokenAccount,
  vault,
  vaultAuthority,
}: {
  readonly client: Address;
  readonly agreement: Address;
  readonly milestone: Address;
  readonly freelancer: Address;
  readonly mint: Address;
  readonly freelancerTokenAccount: Address;
  readonly vault: Address;
  readonly vaultAuthority: Address;
}): Instruction {
  return {
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    accounts: [
      { address: client, role: AccountRole.READONLY_SIGNER },
      { address: agreement, role: AccountRole.WRITABLE },
      { address: milestone, role: AccountRole.WRITABLE },
      { address: freelancer, role: AccountRole.READONLY },
      { address: mint, role: AccountRole.READONLY },
      { address: freelancerTokenAccount, role: AccountRole.WRITABLE },
      { address: vault, role: AccountRole.WRITABLE },
      { address: vaultAuthority, role: AccountRole.READONLY },
      { address: TOKEN_PROGRAM_ID, role: AccountRole.READONLY },
    ],
    data: discriminator('approve_and_release'),
  };
}

export function getRaiseDisputeInstruction({
  participant,
  agreement,
  milestone,
}: {
  readonly participant: Address;
  readonly agreement: Address;
  readonly milestone: Address;
}): Instruction {
  return {
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    accounts: [
      { address: participant, role: AccountRole.READONLY_SIGNER },
      { address: agreement, role: AccountRole.READONLY },
      { address: milestone, role: AccountRole.WRITABLE },
    ],
    data: discriminator('raise_dispute'),
  };
}

export function getMutualCancelAndRefundInstruction({
  client,
  freelancer,
  agreement,
  milestone,
  clientTokenAccount,
  vault,
  vaultAuthority,
}: {
  readonly client: Address;
  readonly freelancer: Address;
  readonly agreement: Address;
  readonly milestone: Address;
  readonly clientTokenAccount: Address;
  readonly vault: Address;
  readonly vaultAuthority: Address;
}): Instruction {
  return {
    programAddress: OPENWORK_ESCROW_PROGRAM_ID,
    accounts: [
      { address: client, role: AccountRole.READONLY_SIGNER },
      { address: freelancer, role: AccountRole.READONLY_SIGNER },
      { address: agreement, role: AccountRole.WRITABLE },
      { address: milestone, role: AccountRole.WRITABLE },
      { address: clientTokenAccount, role: AccountRole.WRITABLE },
      { address: vault, role: AccountRole.WRITABLE },
      { address: vaultAuthority, role: AccountRole.READONLY },
      { address: TOKEN_PROGRAM_ID, role: AccountRole.READONLY },
    ],
    data: discriminator('mutual_cancel_and_refund'),
  };
}

/**
 * Idempotent associated-token-account creation. Safe to include in every
 * transaction that needs an ATA: it is a no-op when the account exists.
 */
export function getCreateAssociatedTokenIdempotentInstruction({
  payer,
  ata,
  owner,
  mint,
}: {
  readonly payer: Address;
  readonly ata: Address;
  readonly owner: Address;
  readonly mint: Address;
}): Instruction {
  return {
    programAddress: ASSOCIATED_TOKEN_PROGRAM_ID,
    accounts: [
      { address: payer, role: AccountRole.WRITABLE_SIGNER },
      { address: ata, role: AccountRole.WRITABLE },
      { address: owner, role: AccountRole.READONLY },
      { address: mint, role: AccountRole.READONLY },
      { address: SYSTEM_PROGRAM_ID, role: AccountRole.READONLY },
      { address: TOKEN_PROGRAM_ID, role: AccountRole.READONLY },
    ],
    data: new Uint8Array([1]),
  };
}

/** Base58 is not in scope here; addresses are already base58 strings. */
const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

export function base58Decode(value: string): Uint8Array {
  let num = 0n;
  for (const char of value) {
    const index = BASE58_ALPHABET.indexOf(char);
    if (index === -1) throw new Error(`invalid base58 character: ${char}`);
    num = num * 58n + BigInt(index);
  }
  const bytes: number[] = [];
  while (num > 0n) {
    bytes.unshift(Number(num % 256n));
    num /= 256n;
  }
  for (const char of value) {
    if (char !== '1') break;
    bytes.unshift(0);
  }
  if (bytes.length !== 32) {
    throw new Error(`expected 32-byte address, got ${bytes.length}`);
  }
  return Uint8Array.from(bytes);
}

export function base58Encode(bytes: Uint8Array): string {
  let num = 0n;
  for (const byte of bytes) num = num * 256n + BigInt(byte);
  let out = '';
  while (num > 0n) {
    out = BASE58_ALPHABET[Number(num % 58n)] + out;
    num /= 58n;
  }
  for (const byte of bytes) {
    if (byte !== 0) break;
    out = `1${out}`;
  }
  return out;
}

function assertBytes(value: Uint8Array, length: number, label: string): void {
  if (value.length !== length) {
    throw new TypeError(`${label} must be ${length} bytes, got ${value.length}`);
  }
}
