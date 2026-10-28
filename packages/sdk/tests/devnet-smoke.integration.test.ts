/**
 * Devnet smoke proof.
 *
 * Moves real (valueless) SPL tokens on Solana Devnet against the program
 * deployed at `GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR`, then asserts the
 * account state and balances. Gated because it spends Devnet SOL and mutates a
 * public cluster:
 *
 *   OPENWORK_DEVNET_SMOKE=1 \
 *   OPENWORK_DEVNET_CLIENT_KEYPAIR=~/.config/solana/id.json \
 *   pnpm --filter @openwork/sdk test:devnet
 *
 * The client keypair is read from disk and never logged. The freelancer is a
 * fresh keypair funded with a small SOL transfer from the client, so no airdrop
 * faucet is involved.
 */
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, resolve } from 'node:path';

import {
  type Address,
  type Instruction,
  type KeyPairSigner,
  type Signature,
  AccountRole,
  addSignersToInstruction,
  appendTransactionMessageInstructions,
  createKeyPairSignerFromBytes,
  createSolanaRpc,
  createTransactionMessage,
  generateKeyPairSigner,
  getBase64EncodedWireTransaction,
  getSignatureFromTransaction,
  pipe,
  setTransactionMessageFeePayerSigner,
  setTransactionMessageLifetimeUsingBlockhash,
  signTransactionMessageWithSigners,
} from '@solana/kit';
import { beforeAll, describe, expect, it } from 'vitest';

import {
  bytesToHex,
  decodeAgreementAccount,
  decodeMilestoneAccount,
  decodeRpcAccountData,
  findAgreementPda,
  findMilestonePda,
  findVaultAuthorityPda,
  getAcceptAgreementInstruction,
  getApproveAndReleaseInstruction,
  getCreateAgreementInstruction,
  getCreateAssociatedTokenIdempotentInstruction,
  getCreateMilestoneInstruction,
  getFundMilestoneInstruction,
  getSubmitMilestoneInstruction,
  hashTerms,
  SYSTEM_PROGRAM_ID,
  type MilestoneAccount,
} from '../src/index.js';
import {
  MINT_ACCOUNT_SIZE,
  findAssociatedTokenAddress,
  getInitializeMint2Instruction,
  getMintToInstruction,
  getSystemCreateAccountInstruction,
  readTokenBalance,
} from './token.js';

const ENABLED = process.env.OPENWORK_DEVNET_SMOKE === '1';
const RPC_URL = process.env.OPENWORK_DEVNET_RPC_URL ?? 'https://api.devnet.solana.com';
const MINTS = 1_000_000_000n; // 1,000 tUSDC at six decimals
const MILESTONE_AMOUNT = 250_000_000n; // 250 tUSDC
const FREELANCER_SOL = 40_000_000n; // 0.04 SOL for fees and rent
const MINT_DECIMALS = 6;
const NONCE = BigInt(Date.now()); // unique per run, still stable inside the run
const RECEIPT_PATH = resolve(process.cwd(), '../../docs/devnet-smoke.json');

type Rpc = ReturnType<typeof createSolanaRpc>;

async function accountData(rpc: Rpc, address: Address): Promise<Uint8Array> {
  const { value } = await rpc.getAccountInfo(address, { encoding: 'base64' }).send();
  if (!value) throw new Error(`account ${address} not found on Devnet`);
  return decodeRpcAccountData(value.data as readonly [string, string]);
}

function keypairPath(): string {
  const configured = process.env.OPENWORK_DEVNET_CLIENT_KEYPAIR;
  return configured
    ? configured.replace(/^~(?=$|\/)/, homedir())
    : resolve(homedir(), '.config/solana/id.json');
}

async function loadClient(): Promise<KeyPairSigner> {
  const raw = readFileSync(keypairPath(), 'utf8');
  const bytes = Uint8Array.from(JSON.parse(raw) as number[]);
  if (bytes.length !== 64) {
    throw new Error('client keypair file must contain a 64 byte secret key');
  }
  return createKeyPairSignerFromBytes(bytes);
}

/** `SystemProgram::Transfer`. Kit 8.5 has no helper for this instruction. */
function getTransferSolInstruction(input: {
  readonly source: Address;
  readonly destination: Address;
  readonly amount: bigint;
}): Instruction {
  const data = new Uint8Array(12);
  new DataView(data.buffer).setUint32(0, 2, true);
  new DataView(data.buffer).setBigUint64(4, input.amount, true);
  return {
    programAddress: SYSTEM_PROGRAM_ID,
    accounts: [
      { address: input.source, role: AccountRole.WRITABLE_SIGNER },
      { address: input.destination, role: AccountRole.WRITABLE },
    ],
    data,
  };
}

async function confirm(rpc: Rpc, signature: Signature): Promise<void> {
  for (let attempt = 0; attempt < 90; attempt += 1) {
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
    await new Promise((resolve_) => setTimeout(resolve_, 1_000));
  }
  throw new Error(`transaction ${signature} was not confirmed in 90s`);
}

async function send(
  rpc: Rpc,
  feePayer: KeyPairSigner,
  instructions: readonly Instruction[],
  signers: readonly KeyPairSigner[] = [],
): Promise<Signature> {
  const withSigners = instructions.map((instruction) =>
    signers.length === 0
      ? instruction
      : addSignersToInstruction([...signers], instruction),
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
  await confirm(rpc, signature);
  return signature;
}

describe.runIf(ENABLED)('OpenWork on Solana Devnet', () => {
  let rpc: Rpc;
  let client: KeyPairSigner;
  let freelancer: KeyPairSigner;
  let mint: KeyPairSigner;
  let clientAta: Address;
  let freelancerAta: Address;
  let agreement: Address;
  let milestone: Address;
  let vaultAuthority: Address;
  let vault: Address;
  let termsHash: Uint8Array;

  const signatures: Record<string, string> = {};

  beforeAll(async () => {
    rpc = createSolanaRpc(RPC_URL);
    await rpc.getHealth().send();

    client = await loadClient();
    freelancer = await generateKeyPairSigner();

    signatures.fundFreelancer = await send(rpc, client, [
      getTransferSolInstruction({
        source: client.address,
        destination: freelancer.address,
        amount: FREELANCER_SOL,
      }),
    ]);

    // A brand-new mint per run keeps the smoke test independent of any prior
    // state and makes the receipt self-contained.
    mint = await generateKeyPairSigner();
    const mintRent = await rpc
      .getMinimumBalanceForRentExemption(BigInt(MINT_ACCOUNT_SIZE))
      .send();
    signatures.createMint = await send(
      rpc,
      client,
      [
        getSystemCreateAccountInstruction({
          payer: client.address,
          newAccount: mint.address,
          lamports: mintRent,
          space: BigInt(MINT_ACCOUNT_SIZE),
          owner: 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA' as Address,
        }),
        getInitializeMint2Instruction({
          mint: mint.address,
          decimals: MINT_DECIMALS,
          mintAuthority: client.address,
        }),
      ],
      [mint],
    );

    clientAta = await findAssociatedTokenAddress(client.address, mint.address);
    freelancerAta = await findAssociatedTokenAddress(freelancer.address, mint.address);
    signatures.createTokenAccounts = await send(rpc, client, [
      getCreateAssociatedTokenIdempotentInstruction({
        payer: client.address,
        ata: clientAta,
        owner: client.address,
        mint: mint.address,
      }),
      getCreateAssociatedTokenIdempotentInstruction({
        payer: client.address,
        ata: freelancerAta,
        owner: freelancer.address,
        mint: mint.address,
      }),
    ]);
    signatures.mintTokens = await send(rpc, client, [
      getMintToInstruction({
        mint: mint.address,
        destination: clientAta,
        authority: client.address,
        amount: MINTS,
      }),
    ]);

    agreement = await findAgreementPda(client.address, NONCE);
    milestone = await findMilestonePda(agreement, 0);
    vaultAuthority = await findVaultAuthorityPda(agreement);
    vault = await findAssociatedTokenAddress(vaultAuthority, mint.address);

    termsHash = await hashTerms({
      title: 'Devnet smoke — protocol landing page',
      description: 'Automated end-to-end run against the deployed Devnet program.',
      client: client.address,
      freelancer: freelancer.address,
      mint: mint.address,
      milestones: [
        {
          index: 0,
          title: 'Design and build',
          description: 'Single milestone proving the full escrow lifecycle.',
          amount: MILESTONE_AMOUNT,
        },
      ],
    });
  }, 120_000);

  it('creates the agreement', async () => {
    signatures.createAgreement = await send(rpc, client, [
      getCreateAgreementInstruction({
        client: client.address,
        agreement,
        freelancer: freelancer.address,
        mint: mint.address,
        nonce: NONCE,
        termsHash,
      }),
    ]);
    const decoded = decodeAgreementAccount(await accountData(rpc, agreement));
    expect(decoded.state).toBe('draft');
    expect(decoded.client).toBe(client.address);
    expect(decoded.freelancer).toBe(freelancer.address);
    expect(bytesToHex(decoded.termsHash)).toBe(bytesToHex(termsHash));
  });

  it('adds the milestone', async () => {
    signatures.createMilestone = await send(rpc, client, [
      getCreateMilestoneInstruction({
        client: client.address,
        agreement,
        milestone,
        index: 0,
        amount: MILESTONE_AMOUNT,
      }),
    ]);
    const decoded = decodeMilestoneAccount(await accountData(rpc, milestone));
    expect(decoded.state).toBe('pending');
    expect(decoded.amount).toBe(MILESTONE_AMOUNT);
  });

  it('accepts as the freelancer', async () => {
    signatures.acceptAgreement = await send(rpc, freelancer, [
      getAcceptAgreementInstruction({
        freelancer: freelancer.address,
        agreement,
        termsHash,
      }),
    ]);
    expect(decodeAgreementAccount(await accountData(rpc, agreement)).state).toBe(
      'active',
    );
  });

  it('funds, submits and releases the milestone', async () => {
    const clientBefore = await readTokenBalance(rpc, clientAta);

    signatures.fundMilestone = await send(rpc, client, [
      getFundMilestoneInstruction({
        client: client.address,
        agreement,
        milestone,
        mint: mint.address,
        clientTokenAccount: clientAta,
        vault,
        vaultAuthority,
      }),
    ]);
    expect(await readTokenBalance(rpc, clientAta)).toBe(clientBefore - MILESTONE_AMOUNT);
    expect(await readTokenBalance(rpc, vault)).toBe(MILESTONE_AMOUNT);

    signatures.submitMilestone = await send(rpc, freelancer, [
      getSubmitMilestoneInstruction({
        freelancer: freelancer.address,
        agreement,
        milestone,
        deliverableHash: new Uint8Array(32).fill(0x5a),
      }),
    ]);

    const freelancerBefore = await readTokenBalance(rpc, freelancerAta);
    signatures.approveAndRelease = await send(rpc, client, [
      getApproveAndReleaseInstruction({
        client: client.address,
        agreement,
        milestone,
        freelancer: freelancer.address,
        mint: mint.address,
        freelancerTokenAccount: freelancerAta,
        vault,
        vaultAuthority,
      }),
    ]);

    expect(await readTokenBalance(rpc, freelancerAta)).toBe(
      freelancerBefore + MILESTONE_AMOUNT,
    );
    expect(await readTokenBalance(rpc, vault)).toBe(0n);

    const decoded: MilestoneAccount = decodeMilestoneAccount(
      await accountData(rpc, milestone),
    );
    expect(decoded.state).toBe('released');
    expect(decoded.settledAt).toBeGreaterThan(0n);

    const decodedAgreement = decodeAgreementAccount(await accountData(rpc, agreement));
    expect(decodedAgreement.settledCount).toBe(1);
    expect(decodedAgreement.releasedAmount).toBe(MILESTONE_AMOUNT);
  }, 180_000);

  it('writes a public receipt', async () => {
    const receipt = {
      cluster: 'devnet',
      rpc: RPC_URL,
      programId:
        process.env.OPENWORK_PROGRAM_ID ?? 'GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR',
      recordedAt: new Date().toISOString(),
      mint: {
        address: mint.address,
        decimals: MINT_DECIMALS,
        symbol: 'tUSDC',
        supply: MINTS.toString(),
      },
      agreement: {
        pda: agreement,
        nonce: NONCE.toString(),
        termsHash: bytesToHex(termsHash),
        client: client.address,
        freelancer: freelancer.address,
        vault,
        vaultAuthority,
        milestone: milestone,
        amount: MILESTONE_AMOUNT.toString(),
      },
      signatures,
    };

    const directory = dirname(RECEIPT_PATH);
    mkdirSync(directory, { recursive: true });
    writeFileSync(RECEIPT_PATH, `${JSON.stringify(receipt, null, 2)}\n`, 'utf8');

    console.log('--- OpenWork Devnet receipt ---');
    console.log(JSON.stringify(receipt, null, 2));
    console.log('--- end receipt ---');
    expect(receipt.signatures.approveAndRelease).toBeTruthy();
  });
});
