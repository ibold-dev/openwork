/**
 * Integration-test harness for the escrow program.
 *
 * Runs against a real `solana-test-validator` with the compiled program loaded,
 * because LiteSVM does not ship the SPL Token program needed by the escrow CPIs.
 * See `scripts/test-integration.sh`.
 */
import {
  type Address,
  type Instruction,
  type KeyPairSigner,
  type Signature,
  type TransactionSigner,
  addSignersToInstruction,
  appendTransactionMessageInstructions,
  createSolanaRpc,
  createTransactionMessage,
  generateKeyPairSigner,
  getBase64EncodedWireTransaction,
  getSignatureFromTransaction,
  lamports,
  pipe,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
} from '@solana/kit';

export const RPC_URL = process.env.OPENWORK_TEST_RPC_URL ?? 'http://127.0.0.1:8899';

export type Rpc = ReturnType<typeof createSolanaRpc>;

export function createRpc(): Rpc {
  return createSolanaRpc(RPC_URL);
}

const CONFIRM_ATTEMPTS = 120;
const CONFIRM_INTERVAL_MS = 250;

/** Waits for a signature to reach `confirmed` or `finalized`. */
export async function confirmSignature(rpc: Rpc, signature: Signature): Promise<void> {
  for (let attempt = 0; attempt < CONFIRM_ATTEMPTS; attempt += 1) {
    const { value } = await rpc.getSignatureStatuses([signature]).send();
    const status = value[0];
    if (status?.err) {
      throw new Error(`transaction ${signature} failed: ${JSON.stringify(status.err)}`);
    }
    if (
      status?.confirmationStatus === 'confirmed' ||
      status?.confirmationStatus === 'finalized'
    ) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, CONFIRM_INTERVAL_MS));
  }
  throw new Error(`transaction ${signature} was not confirmed in time`);
}

export interface SendOptions {
  /** Additional signers referenced by the instructions. */
  readonly signers?: readonly TransactionSigner[];
}

/**
 * Builds, signs, sends and confirms one transaction. Doesn't retry: a failed
 * assertion should surface the real program error, not a retry artefact.
 */
export async function sendTransaction(
  rpc: Rpc,
  feePayer: KeyPairSigner,
  instructions: readonly Instruction[],
  options: SendOptions = {},
): Promise<Signature> {
  const signers = options.signers ?? [];
  const withSigners = instructions.map((instruction) =>
    signers.length > 0 ? addSignersToInstruction([...signers], instruction) : instruction,
  );

  const { value: latestBlockhash } = await rpc.getLatestBlockhash().send();
  const message = pipe(
    createTransactionMessage({ version: 0 }),
    (tx) => setTransactionMessageFeePayerSigner(feePayer, tx),
    (tx) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, tx),
    (tx) => appendTransactionMessageInstructions(withSigners, tx),
  );
  const signed = await signTransactionMessageWithSigners(message);
  const signature = getSignatureFromTransaction(signed);
  await rpc
    .sendTransaction(getBase64EncodedWireTransaction(signed), {
      encoding: 'base64',
      preflightCommitment: 'confirmed',
    })
    .send();
  await confirmSignature(rpc, signature);
  return signature;
}

export interface FailureReport {
  readonly signature?: string;
  readonly logs: readonly string[];
  readonly message: string;
}

/**
 * Asserts that a transaction fails and returns its logs. Fails the test when
 * the transaction unexpectedly succeeds.
 */
export async function expectTransactionFailure(
  run: () => Promise<Signature>,
): Promise<FailureReport> {
  try {
    const signature = await run();
    throw new Error(`expected the transaction to fail, but ${signature} succeeded`);
  } catch (error) {
    if (error instanceof Error && error.message.startsWith('expected the transaction')) {
      throw error;
    }
    const context = (error as { context?: Record<string, unknown> }).context;
    const logs = Array.isArray(context?.logs) ? (context.logs as string[]) : [];
    return {
      logs,
      message: error instanceof Error ? error.message : String(error),
    };
  }
}

/** Airdrops and waits until the balance is visible to preflight. */
export async function fundAccount(rpc: Rpc, address: Address, sol = 10n): Promise<void> {
  // `requestAirdrop` is only exposed on test-cluster typings.
  const airdropRpc = rpc as unknown as {
    requestAirdrop: (
      address: Address,
      amount: ReturnType<typeof lamports>,
    ) => { send: () => Promise<Signature> };
  };
  const signature = await airdropRpc
    .requestAirdrop(address, lamports(sol * 1_000_000_000n))
    .send();
  await confirmSignature(rpc, signature);
}

/** Two funded signers for a fresh lifecycle test. */
export async function makeParticipants(
  rpc: Rpc,
): Promise<{ client: KeyPairSigner; freelancer: KeyPairSigner }> {
  const client = await generateKeyPairSigner();
  const freelancer = await generateKeyPairSigner();
  await Promise.all([
    fundAccount(rpc, client.address),
    fundAccount(rpc, freelancer.address),
  ]);
  return { client, freelancer };
}
