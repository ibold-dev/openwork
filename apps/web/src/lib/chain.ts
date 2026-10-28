'use client';

/**
 * Chain client.
 *
 * Reads go straight to the RPC; writes are compiled locally and signed by the
 * user's wallet. The app never holds keys and never proxies signatures.
 */
import {
  type Address,
  type Base64EncodedWireTransaction,
  type Instruction,
  type Signature,
  appendTransactionMessageInstructions,
  compileTransaction,
  createSolanaRpc,
  createTransactionMessage,
  getBase58Decoder,
  getTransactionDecoder,
  getTransactionEncoder,
  pipe,
  setTransactionMessageFeePayer,
  setTransactionMessageLifetimeUsingBlockhash,
} from '@solana/kit';

import {
  AGREEMENT_ACCOUNT_SIZE,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  decodeAgreementAccount,
  decodeMilestoneAccount,
  decodeRpcAccountData,
  findAgreementPda,
  findMilestonePda,
  findVaultAuthorityPda,
  getCreateAgreementInstruction,
  getCreateMilestoneInstruction,
  type AgreementAccount,
  type MilestoneAccount,
} from '@openwork/sdk';

import { CHAIN, RPC_URL } from './config';
import { signTransactionBytes, type StandardWallet, type WalletAccount } from './wallet';

let cachedRpc: ReturnType<typeof createSolanaRpc> | null = null;

export function rpc() {
  if (!cachedRpc) cachedRpc = createSolanaRpc(RPC_URL);
  return cachedRpc;
}

export type Rpc = ReturnType<typeof rpc>;

export interface MilestoneView {
  readonly address: Address;
  readonly account: MilestoneAccount;
}

export interface AgreementView {
  readonly address: Address;
  readonly account: AgreementAccount;
  readonly milestones: readonly MilestoneView[];
  /** Vault token account balance, or null when the ATA does not exist yet. */
  readonly vaultBalance: bigint | null;
}

export async function fetchAgreement(address: Address): Promise<AgreementView | null> {
  const { value } = await rpc().getAccountInfo(address, { encoding: 'base64' }).send();
  if (!value) return null;
  const data = decodeRpcAccountData(value.data as readonly [string, string]);
  if (data.length !== AGREEMENT_ACCOUNT_SIZE) return null;
  const account = decodeAgreementAccount(data);

  const milestoneAddresses = await Promise.all(
    Array.from({ length: account.milestoneCount }, (_, index) =>
      findMilestonePda(address, index),
    ),
  );

  const milestones = (
    await Promise.all(
      milestoneAddresses.map(async (milestoneAddress): Promise<MilestoneView | null> => {
        const info = await rpc()
          .getAccountInfo(milestoneAddress, { encoding: 'base64' })
          .send();
        if (!info.value) return null;
        return {
          address: milestoneAddress,
          account: decodeMilestoneAccount(
            decodeRpcAccountData(info.value.data as readonly [string, string]),
          ),
        };
      }),
    )
  ).filter((entry): entry is MilestoneView => entry !== null);

  return {
    address,
    account,
    milestones,
    vaultBalance: await fetchTokenBalance(await vaultAddress(address, account.mint)),
  };
}

export async function associatedTokenAddress(
  owner: Address,
  mint: Address,
): Promise<Address> {
  const { getAddressEncoder, getProgramDerivedAddress } = await import('@solana/kit');
  const encoder = getAddressEncoder();
  const [ata] = await getProgramDerivedAddress({
    programAddress: ASSOCIATED_TOKEN_PROGRAM_ID,
    seeds: [
      encoder.encode(owner),
      encoder.encode(TOKEN_PROGRAM_ID),
      encoder.encode(mint),
    ],
  });
  return ata;
}

/** The escrow's vault token account for an agreement. */
export async function vaultAddress(agreement: Address, mint: Address): Promise<Address> {
  const authority = await findVaultAuthorityPda(agreement);
  return associatedTokenAddress(authority, mint);
}

/** SPL token balance, or null when the account has not been created yet. */
export async function fetchTokenBalance(tokenAccount: Address): Promise<bigint | null> {
  try {
    const { value } = await rpc().getTokenAccountBalance(tokenAccount).send();
    return BigInt(value.amount);
  } catch {
    return null;
  }
}

export interface SendResult {
  readonly signature: string;
  readonly confirmedAt: number;
}

/**
 * Compiles instructions against the wallet's fee payer, asks the wallet to sign
 * the wire transaction, submits it and waits for confirmation.
 */
export async function sendInstructions(input: {
  wallet: StandardWallet;
  account: WalletAccount;
  instructions: readonly Instruction[];
}): Promise<SendResult> {
  const feePayer = input.account.address as Address;
  const wireBytes = await buildWireTransaction(feePayer, input.instructions);
  const signed = await signTransactionBytes(
    input.wallet,
    input.account,
    CHAIN,
    wireBytes,
  );
  const signature = readSignature(signed, feePayer);
  await submitWireTransaction(signed);
  return { signature, confirmedAt: Date.now() };
}

/**
 * Compiles an unsigned transaction to wire bytes, with a zero-filled signature
 * placeholder. This is the shape wallets expect from `solana:signTransaction`.
 */
export async function buildWireTransaction(
  feePayer: Address,
  instructions: readonly Instruction[],
): Promise<Uint8Array> {
  const { value: latestBlockhash } = await rpc().getLatestBlockhash().send();
  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (tx) => setTransactionMessageFeePayer(feePayer, tx),
    (tx) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, tx),
    (tx) => appendTransactionMessageInstructions(instructions, tx),
  );
  const compiled = compileTransaction(message);
  const encoded = getTransactionEncoder().encode({
    messageBytes: compiled.messageBytes,
    signatures: { [feePayer]: new Uint8Array(64) } as never,
  });
  return new Uint8Array(encoded);
}

/** Adds this wallet's signature to an already-built wire transaction. */
export async function coSignWireTransaction(input: {
  wallet: StandardWallet;
  account: WalletAccount;
  wireTransaction: Uint8Array;
}): Promise<Uint8Array> {
  return signTransactionBytes(input.wallet, input.account, CHAIN, input.wireTransaction);
}

export async function submitWireTransaction(
  wireTransaction: Uint8Array,
): Promise<Signature> {
  const client = rpc();
  const signature = readSignature(wireTransaction, undefined);
  await client
    .sendTransaction(toBase64(wireTransaction) as Base64EncodedWireTransaction, {
      encoding: 'base64',
      preflightCommitment: 'confirmed',
    })
    .send();
  await pollSignature(signature);
  return signature as Signature;
}

function readSignature(
  wireTransaction: Uint8Array,
  feePayer: string | undefined,
): string {
  const decoded = getTransactionDecoder().decode(wireTransaction);
  const key = feePayer
    ? (feePayer as Address)
    : (Object.keys(decoded.signatures)[0] as Address | undefined);
  if (!key) throw new Error('transaction has no signatures');
  const bytes = decoded.signatures[key];
  if (!bytes || bytes.every((byte) => byte === 0)) {
    throw new Error('wallet did not sign this transaction');
  }
  return getBase58Decoder().decode(bytes);
}

export function toBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let index = 0; index < bytes.length; index += chunk) {
    binary += String.fromCharCode(...bytes.subarray(index, index + chunk));
  }
  return btoa(binary);
}

export function fromBase64(value: string): Uint8Array {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

export async function pollSignature(signature: string): Promise<void> {
  const client = rpc();
  const deadline = Date.now() + 90_000;
  while (Date.now() < deadline) {
    const { value } = await client.getSignatureStatuses([signature as Signature]).send();
    const status = value[0];
    if (status?.err) {
      throw new Error(`Transaction failed: ${JSON.stringify(status.err)}`);
    }
    if (
      status?.confirmationStatus === 'confirmed' ||
      status?.confirmationStatus === 'finalized'
    ) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 400));
  }
  throw new Error('Transaction was not confirmed within 90 seconds');
}

/** Reads an SPL mint's decimals. Falls back to 6 when the account is missing. */
export async function fetchMintDecimals(mint: Address): Promise<number> {
  const { value } = await rpc().getAccountInfo(mint, { encoding: 'base64' }).send();
  if (!value) return 6;
  const data = decodeRpcAccountData(value.data as readonly [string, string]);
  // SPL Mint layout: COption<Pubkey> authority (36) + u64 supply (8) → 44.
  return data.length >= 45 ? data[44]! : 6;
}

export interface DraftMilestone {
  readonly title: string;
  readonly description: string;
  readonly amount: bigint;
}

/**
 * Builds the two transactions the wizard submits: agreement creation, then
 * milestones in contiguous order.
 */
export async function buildDraftInstructions(input: {
  client: Address;
  freelancer: Address;
  mint: Address;
  nonce: bigint;
  termsHash: Uint8Array;
  milestones: readonly DraftMilestone[];
}): Promise<{ agreement: Address; transactions: readonly Instruction[][] }> {
  const agreement = await findAgreementPda(input.client, input.nonce);
  const milestoneAddresses = await Promise.all(
    input.milestones.map((_, index) => findMilestonePda(agreement, index)),
  );

  const createAgreement = getCreateAgreementInstruction({
    client: input.client,
    agreement,
    freelancer: input.freelancer,
    mint: input.mint,
    nonce: input.nonce,
    termsHash: input.termsHash,
  });

  const milestoneInstructions = input.milestones.map((milestone, index) =>
    getCreateMilestoneInstruction({
      client: input.client,
      agreement,
      milestone: milestoneAddresses[index]!,
      index,
      amount: milestone.amount,
    }),
  );

  return {
    agreement,
    transactions: [[createAgreement], milestoneInstructions],
  };
}
