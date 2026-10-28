/**
 * Minimal SPL Token helpers built as raw instructions.
 *
 * The SDK intentionally does not depend on `@solana/spl-token`; these cover
 * exactly what the integration tests need (create mint, mint tokens, create
 * associated token accounts).
 */
import {
  type Address,
  type Instruction,
  AccountRole,
  getAddressEncoder,
  getProgramDerivedAddress,
} from '@solana/kit';

import type { Rpc } from './helpers.js';
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  SYSTEM_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  concatBytes,
  u64le,
} from '../src/index.js';

const addressEncoder = getAddressEncoder();

const SYSTEM_CREATE_ACCOUNT = 0;
const TOKEN_INITIALIZE_MINT2 = 20;
const TOKEN_MINT_TO = 7;

export const MINT_ACCOUNT_SIZE = 82;
export const TOKEN_ACCOUNT_SIZE = 165;

/** `SystemProgram::CreateAccount`. */
export function getSystemCreateAccountInstruction(input: {
  readonly payer: Address;
  readonly newAccount: Address;
  readonly lamports: bigint;
  readonly space: bigint;
  readonly owner: Address;
}): Instruction {
  const data = new Uint8Array(4 + 8 + 8 + 32);
  const view = new DataView(data.buffer);
  view.setUint32(0, SYSTEM_CREATE_ACCOUNT, true);
  view.setBigUint64(4, input.lamports, true);
  view.setBigUint64(12, input.space, true);
  data.set(addressEncoder.encode(input.owner), 20);
  return {
    programAddress: SYSTEM_PROGRAM_ID,
    accounts: [
      { address: input.payer, role: AccountRole.WRITABLE_SIGNER },
      { address: input.newAccount, role: AccountRole.WRITABLE_SIGNER },
    ],
    data,
  };
}

/** `TokenInstruction::InitializeMint2` with no freeze authority. */
export function getInitializeMint2Instruction(input: {
  readonly mint: Address;
  readonly decimals: number;
  readonly mintAuthority: Address;
}): Instruction {
  const data = new Uint8Array(1 + 1 + 32 + 1);
  data[0] = TOKEN_INITIALIZE_MINT2;
  data[1] = input.decimals;
  data.set(addressEncoder.encode(input.mintAuthority), 2);
  data[34] = 0; // freeze authority: None
  return {
    programAddress: TOKEN_PROGRAM_ID,
    accounts: [{ address: input.mint, role: AccountRole.WRITABLE }],
    data,
  };
}

/** `TokenInstruction::MintTo`. */
export function getMintToInstruction(input: {
  readonly mint: Address;
  readonly destination: Address;
  readonly authority: Address;
  readonly amount: bigint;
}): Instruction {
  return {
    programAddress: TOKEN_PROGRAM_ID,
    accounts: [
      { address: input.mint, role: AccountRole.WRITABLE },
      { address: input.destination, role: AccountRole.WRITABLE },
      { address: input.authority, role: AccountRole.READONLY_SIGNER },
    ],
    data: concatBytes(new Uint8Array([TOKEN_MINT_TO]), u64le(input.amount)),
  };
}

/** Associated token account address for `(owner, mint)`. */
export async function findAssociatedTokenAddress(
  owner: Address,
  mint: Address,
): Promise<Address> {
  const [ata] = await getProgramDerivedAddress({
    programAddress: ASSOCIATED_TOKEN_PROGRAM_ID,
    seeds: [
      addressEncoder.encode(owner),
      addressEncoder.encode(TOKEN_PROGRAM_ID),
      addressEncoder.encode(mint),
    ],
  });
  return ata;
}

/** Reads an SPL token account balance via `getTokenAccountBalance`. */
export async function readTokenBalance(rpc: Rpc, tokenAccount: Address): Promise<bigint> {
  const { value } = await rpc.getTokenAccountBalance(tokenAccount).send();
  return BigInt(value.amount);
}
