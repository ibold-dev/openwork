/**
 * End-to-end lifecycle against a real local validator.
 *
 * Prerequisites: `scripts/test-integration.sh` builds the program, starts
 * `solana-test-validator` with it loaded and runs this file. These tests move
 * real SPL tokens on that throwaway ledger; they never touch Devnet/Mainnet.
 */
import { beforeAll, describe, expect, it } from 'vitest';

import {
  AGREEMENT_ACCOUNT_SIZE,
  MAX_MILESTONES,
  MILESTONE_ACCOUNT_SIZE,
  bytesToHex,
  decodeAgreementAccount,
  decodeMilestoneAccount,
  decodeRpcAccountData,
  findAgreementPda,
  findMilestonePda,
  findVaultAuthorityPda,
  getAcceptAgreementInstruction,
  getApproveAndReleaseInstruction,
  getCancelDraftAgreementInstruction,
  getCreateAgreementInstruction,
  getCreateAssociatedTokenIdempotentInstruction,
  getCreateMilestoneInstruction,
  getFundMilestoneInstruction,
  getMutualCancelAndRefundInstruction,
  getRaiseDisputeInstruction,
  getSubmitMilestoneInstruction,
  hashTerms,
  type AgreementAccount,
  type MilestoneAccount,
} from '../src/index.js';
import {
  type Rpc,
  createRpc,
  expectTransactionFailure,
  makeParticipants,
  sendTransaction,
} from './helpers.js';
import {
  MINT_ACCOUNT_SIZE,
  findAssociatedTokenAddress,
  getInitializeMint2Instruction,
  getMintToInstruction,
  getSystemCreateAccountInstruction,
  readTokenBalance,
} from './token.js';
import type { Address, KeyPairSigner } from '@solana/kit';
import { generateKeyPairSigner } from '@solana/kit';

const MINT_DECIMALS = 6;
const MILESTONE_0_AMOUNT = 100_000_000n; // 100.000000
const MILESTONE_1_AMOUNT = 250_000_000n; // 250.000000
const CLIENT_STARTING_BALANCE = 10_000_000_000n;

let rpc: Rpc;
let client: KeyPairSigner;
let freelancer: KeyPairSigner;
let mint: KeyPairSigner;
let otherMint: KeyPairSigner;
let clientAta: Address;
let freelancerAta: Address;

let agreement: Address;
let milestone0: Address;
let milestone1: Address;
let vault: Address;
let vaultAuthority: Address;
let termsHash: Uint8Array;
const AGREEMENT_NONCE = 1n;

async function accountData(address: Address): Promise<Uint8Array> {
  const { value } = await rpc.getAccountInfo(address, { encoding: 'base64' }).send();
  if (!value) throw new Error(`account ${address} not found`);
  return decodeRpcAccountData(value.data as readonly [string, string]);
}

async function readAgreement(): Promise<AgreementAccount> {
  return decodeAgreementAccount(await accountData(agreement));
}

async function readMilestone(address: Address): Promise<MilestoneAccount> {
  return decodeMilestoneAccount(await accountData(address));
}

async function createMint(decimals: number): Promise<KeyPairSigner> {
  const mintSigner = await generateKeyPairSigner();
  const rent = await rpc
    .getMinimumBalanceForRentExemption(BigInt(MINT_ACCOUNT_SIZE))
    .send();
  await sendTransaction(
    rpc,
    client,
    [
      getSystemCreateAccountInstruction({
        payer: client.address,
        newAccount: mintSigner.address,
        lamports: rent,
        space: BigInt(MINT_ACCOUNT_SIZE),
        owner: 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA' as Address,
      }),
      getInitializeMint2Instruction({
        mint: mintSigner.address,
        decimals,
        mintAuthority: client.address,
      }),
    ],
    { signers: [mintSigner] },
  );
  return mintSigner;
}

async function createAta(owner: Address, mintAddress: Address): Promise<Address> {
  const ata = await findAssociatedTokenAddress(owner, mintAddress);
  await sendTransaction(rpc, client, [
    getCreateAssociatedTokenIdempotentInstruction({
      payer: client.address,
      ata,
      owner,
      mint: mintAddress,
    }),
  ]);
  return ata;
}

describe('escrow lifecycle on a local validator', () => {
  beforeAll(async () => {
    rpc = createRpc();
    try {
      await rpc.getHealth().send();
    } catch (error) {
      throw new Error(
        `local validator unreachable at ${String(error)}. Run scripts/test-integration.sh`,
      );
    }

    ({ client, freelancer } = await makeParticipants(rpc));
    mint = await createMint(MINT_DECIMALS);
    otherMint = await createMint(MINT_DECIMALS);

    clientAta = await createAta(client.address, mint.address);
    freelancerAta = await createAta(freelancer.address, mint.address);
    await sendTransaction(rpc, client, [
      getMintToInstruction({
        mint: mint.address,
        destination: clientAta,
        authority: client.address,
        amount: CLIENT_STARTING_BALANCE,
      }),
    ]);

    agreement = await findAgreementPda(client.address, AGREEMENT_NONCE);
    milestone0 = await findMilestonePda(agreement, 0);
    milestone1 = await findMilestonePda(agreement, 1);
    vaultAuthority = await findVaultAuthorityPda(agreement);
    vault = await findAssociatedTokenAddress(vaultAuthority, mint.address);

    termsHash = await hashTerms({
      title: 'Protocol landing page',
      description: 'Design and build the public marketing site.',
      client: client.address,
      freelancer: freelancer.address,
      mint: mint.address,
      milestones: [
        {
          index: 0,
          title: 'Design',
          description: 'Wireframes and visual language',
          amount: MILESTONE_0_AMOUNT,
        },
        {
          index: 1,
          title: 'Build',
          description: 'Responsive implementation',
          amount: MILESTONE_1_AMOUNT,
        },
      ],
    });
  });

  it('creates the agreement at the PDA the society expects', async () => {
    await sendTransaction(rpc, client, [
      getCreateAgreementInstruction({
        client: client.address,
        agreement,
        freelancer: freelancer.address,
        mint: mint.address,
        nonce: AGREEMENT_NONCE,
        termsHash,
      }),
    ]);

    const data = await accountData(agreement);
    expect(data.length).toBe(AGREEMENT_ACCOUNT_SIZE);
    const decoded = decodeAgreementAccount(data);
    expect(decoded.state).toBe('draft');
    expect(decoded.client).toBe(client.address);
    expect(decoded.freelancer).toBe(freelancer.address);
    expect(decoded.mint).toBe(mint.address);
    expect(bytesToHex(decoded.termsHash)).toBe(bytesToHex(termsHash));
    expect(decoded.milestoneCount).toBe(0);
  });

  it('adds milestones in order and rejects gaps', async () => {
    const gap = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getCreateMilestoneInstruction({
          client: client.address,
          agreement,
          milestone: milestone1,
          index: 1,
          amount: MILESTONE_1_AMOUNT,
        }),
      ]),
    );
    expect(gap.logs.join('\n')).toMatch(/NonContiguousMilestone|6017/);

    await sendTransaction(rpc, client, [
      getCreateMilestoneInstruction({
        client: client.address,
        agreement,
        milestone: milestone0,
        index: 0,
        amount: MILESTONE_0_AMOUNT,
      }),
    ]);
    await sendTransaction(rpc, client, [
      getCreateMilestoneInstruction({
        client: client.address,
        agreement,
        milestone: milestone1,
        index: 1,
        amount: MILESTONE_1_AMOUNT,
      }),
    ]);

    const data = await accountData(milestone0);
    expect(data.length).toBe(MILESTONE_ACCOUNT_SIZE);
    expect(decodeMilestoneAccount(data)).toMatchObject({
      index: 0,
      state: 'pending',
      agreement,
      amount: MILESTONE_0_AMOUNT,
    });
    expect((await readAgreement()).totalAmount).toBe(
      MILESTONE_0_AMOUNT + MILESTONE_1_AMOUNT,
    );
  });

  it('accepts only with the committed terms hash', async () => {
    const wrongHash = new Uint8Array(32).fill(1);
    const mismatch = await expectTransactionFailure(() =>
      sendTransaction(rpc, freelancer, [
        getAcceptAgreementInstruction({
          freelancer: freelancer.address,
          agreement,
          termsHash: wrongHash,
        }),
      ]),
    );
    expect(mismatch.logs.join('\n')).toMatch(/TermsHashMismatch|6018/);

    const clientAttempt = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getAcceptAgreementInstruction({
          freelancer: freelancer.address,
          agreement,
          termsHash,
        }),
        // Client cannot satisfy the freelancer signer; the transaction is
        // rejected before the instruction executes.
      ]),
    );
    expect(clientAttempt.message.length).toBeGreaterThan(0);

    await sendTransaction(rpc, freelancer, [
      getAcceptAgreementInstruction({
        freelancer: freelancer.address,
        agreement,
        termsHash,
      }),
    ]);

    const decoded = await readAgreement();
    expect(decoded.state).toBe('active');
    expect(decoded.acceptedAt).toBeGreaterThan(0n);
  });

  it('rejects funding from a non-client and with a foreign mint', async () => {
    const wrongSigner = await expectTransactionFailure(() =>
      sendTransaction(rpc, freelancer, [
        getFundMilestoneInstruction({
          client: freelancer.address,
          agreement,
          milestone: milestone0,
          mint: mint.address,
          clientTokenAccount: freelancerAta,
          vault,
          vaultAuthority,
        }),
      ]),
    );
    expect(wrongSigner.logs.join('\n').length).toBeGreaterThan(0);

    const foreignAta = await createAta(client.address, otherMint.address);
    const wrongMint = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getFundMilestoneInstruction({
          client: client.address,
          agreement,
          milestone: milestone0,
          mint: otherMint.address,
          clientTokenAccount: foreignAta,
          vault,
          vaultAuthority,
        }),
      ]),
    );
    expect(wrongMint.logs.join('\n').length).toBeGreaterThan(0);

    expect((await readMilestone(milestone0)).state).toBe('pending');
  });

  it('funds the first milestone into the vault', async () => {
    const before = await readTokenBalance(rpc, clientAta);

    await sendTransaction(rpc, client, [
      getFundMilestoneInstruction({
        client: client.address,
        agreement,
        milestone: milestone0,
        mint: mint.address,
        clientTokenAccount: clientAta,
        vault,
        vaultAuthority,
      }),
    ]);

    expect(await readTokenBalance(rpc, clientAta)).toBe(before - MILESTONE_0_AMOUNT);
    expect(await readTokenBalance(rpc, vault)).toBe(MILESTONE_0_AMOUNT);
    const decoded = await readMilestone(milestone0);
    expect(decoded.state).toBe('funded');
    expect(decoded.fundedAt).toBeGreaterThan(0n);
  });

  it('rejects funding the same milestone twice', async () => {
    const failure = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getFundMilestoneInstruction({
          client: client.address,
          agreement,
          milestone: milestone0,
          mint: mint.address,
          clientTokenAccount: clientAta,
          vault,
          vaultAuthority,
        }),
      ]),
    );
    expect(failure.logs.join('\n')).toMatch(/MilestoneNotPending|6006/);
    expect(await readTokenBalance(rpc, vault)).toBe(MILESTONE_0_AMOUNT);
  });

  it('lets only the freelancer submit a deliverable', async () => {
    const deliverableHash = new Uint8Array(32).fill(0xab);

    const clientAttempt = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getSubmitMilestoneInstruction({
          freelancer: client.address,
          agreement,
          milestone: milestone0,
          deliverableHash,
        }),
      ]),
    );
    expect(clientAttempt.logs.join('\n').length).toBeGreaterThan(0);

    await sendTransaction(rpc, freelancer, [
      getSubmitMilestoneInstruction({
        freelancer: freelancer.address,
        agreement,
        milestone: milestone0,
        deliverableHash,
      }),
    ]);

    const decoded = await readMilestone(milestone0);
    expect(decoded.state).toBe('submitted');
    expect(bytesToHex(decoded.deliverableHash)).toBe(bytesToHex(deliverableHash));
  });

  it('releases exactly the milestone amount to the freelancer', async () => {
    const freelancerBefore = await readTokenBalance(rpc, freelancerAta);

    const signature = await sendTransaction(rpc, client, [
      getApproveAndReleaseInstruction({
        client: client.address,
        agreement,
        milestone: milestone0,
        freelancer: freelancer.address,
        mint: mint.address,
        freelancerTokenAccount: freelancerAta,
        vault,
        vaultAuthority,
      }),
    ]);
    expect(signature).toBeTruthy();

    expect(await readTokenBalance(rpc, freelancerAta)).toBe(
      freelancerBefore + MILESTONE_0_AMOUNT,
    );
    expect(await readTokenBalance(rpc, vault)).toBe(0n);

    const decoded = await readMilestone(milestone0);
    expect(decoded.state).toBe('released');
    expect(decoded.settledAt).toBeGreaterThan(0n);

    const agreementState = await readAgreement();
    expect(agreementState.releasedAmount).toBe(MILESTONE_0_AMOUNT);
    expect(agreementState.settledCount).toBe(1);
    expect(agreementState.state).toBe('active');
  });

  it('cannot release the same milestone twice', async () => {
    const failure = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getApproveAndReleaseInstruction({
          client: client.address,
          agreement,
          milestone: milestone0,
          freelancer: freelancer.address,
          mint: mint.address,
          freelancerTokenAccount: freelancerAta,
          vault,
          vaultAuthority,
        }),
      ]),
    );
    expect(failure.logs.join('\n')).toMatch(/MilestoneNotSubmitted|6008/);
    expect(await readTokenBalance(rpc, vault)).toBe(0n);
  });

  it('rejects a release paid to the wrong token account', async () => {
    // Fund, submit, then pay a token account owned by the client instead of the
    // freelancer. The token::authority constraint must reject it.
    await sendTransaction(rpc, client, [
      getFundMilestoneInstruction({
        client: client.address,
        agreement,
        milestone: milestone1,
        mint: mint.address,
        clientTokenAccount: clientAta,
        vault,
        vaultAuthority,
      }),
    ]);
    await sendTransaction(rpc, freelancer, [
      getSubmitMilestoneInstruction({
        freelancer: freelancer.address,
        agreement,
        milestone: milestone1,
        deliverableHash: new Uint8Array(32).fill(0xcd),
      }),
    ]);

    const failure = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getApproveAndReleaseInstruction({
          client: client.address,
          agreement,
          milestone: milestone1,
          freelancer: freelancer.address,
          mint: mint.address,
          freelancerTokenAccount: clientAta,
          vault,
          vaultAuthority,
        }),
      ]),
    );
    expect(failure.logs.join('\n').length).toBeGreaterThan(0);
    expect(await readTokenBalance(rpc, vault)).toBe(MILESTONE_1_AMOUNT);
  });

  it('freezes release after a dispute and requires mutual cancellation to refund', async () => {
    // The client cannot unilaterally drain a disputed milestone either: the
    // refund instruction requires the freelancer's signature.
    await sendTransaction(rpc, freelancer, [
      getRaiseDisputeInstruction({
        participant: freelancer.address,
        agreement,
        milestone: milestone1,
      }),
    ]);
    expect((await readMilestone(milestone1)).state).toBe('disputed');

    const releaseAfterDispute = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getApproveAndReleaseInstruction({
          client: client.address,
          agreement,
          milestone: milestone1,
          freelancer: freelancer.address,
          mint: mint.address,
          freelancerTokenAccount: freelancerAta,
          vault,
          vaultAuthority,
        }),
      ]),
    );
    expect(releaseAfterDispute.logs.join('\n')).toMatch(
      /MilestoneNotSubmitted|MilestoneDisputed|6008|6010/,
    );

    const clientOnlyRefund = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getMutualCancelAndRefundInstruction({
          client: client.address,
          freelancer: freelancer.address,
          agreement,
          milestone: milestone1,
          clientTokenAccount: clientAta,
          vault,
          vaultAuthority,
        }),
      ]),
    );
    expect(clientOnlyRefund.message.length).toBeGreaterThan(0);
    expect(await readTokenBalance(rpc, vault)).toBe(MILESTONE_1_AMOUNT);

    const clientBefore = await readTokenBalance(rpc, clientAta);
    await sendTransaction(
      rpc,
      client,
      [
        getMutualCancelAndRefundInstruction({
          client: client.address,
          freelancer: freelancer.address,
          agreement,
          milestone: milestone1,
          clientTokenAccount: clientAta,
          vault,
          vaultAuthority,
        }),
      ],
      { signers: [freelancer] },
    );

    expect(await readTokenBalance(rpc, clientAta)).toBe(
      clientBefore + MILESTONE_1_AMOUNT,
    );
    expect(await readTokenBalance(rpc, vault)).toBe(0n);

    const milestoneState = await readMilestone(milestone1);
    expect(milestoneState.state).toBe('refunded');

    const agreementState = await readAgreement();
    expect(agreementState.settledCount).toBe(2);
    expect(agreementState.refundedCount).toBe(1);
    // One released and one refunded milestone settles the agreement.
    expect(agreementState.state).toBe('completed');
  });

  it('rejects milestones after acceptance and cancelling an active agreement', async () => {
    const milestone2 = await findMilestonePda(agreement, 2);
    const afterAccept = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getCreateMilestoneInstruction({
          client: client.address,
          agreement,
          milestone: milestone2,
          index: 2,
          amount: 1n,
        }),
      ]),
    );
    expect(afterAccept.logs.join('\n')).toMatch(/AgreementNotDraft|6003/);

    const cancelActive = await expectTransactionFailure(() =>
      sendTransaction(rpc, client, [
        getCancelDraftAgreementInstruction({
          client: client.address,
          agreement,
        }),
      ]),
    );
    expect(cancelActive.logs.join('\n')).toMatch(/AgreementNotDraft|6003/);
  });

  it('binds the milestone cap into the program', async () => {
    // A fresh agreement proves the cap is enforced per agreement, not globally.
    const capClient = (await makeParticipants(rpc)).client;
    const capAgreement = await findAgreementPda(capClient.address, 1n);
    const capTerms = await hashTerms({
      title: 'Cap probe',
      description: '',
      client: capClient.address,
      freelancer: freelancer.address,
      mint: mint.address,
      milestones: [{ index: 0, title: 'only', description: '', amount: 1n }],
    });
    await sendTransaction(rpc, capClient, [
      getCreateAgreementInstruction({
        client: capClient.address,
        agreement: capAgreement,
        freelancer: freelancer.address,
        mint: mint.address,
        nonce: 1n,
        termsHash: capTerms,
      }),
    ]);
    for (let index = 0; index < MAX_MILESTONES; index += 1) {
      await sendTransaction(rpc, capClient, [
        getCreateMilestoneInstruction({
          client: capClient.address,
          agreement: capAgreement,
          milestone: await findMilestonePda(capAgreement, index),
          index,
          amount: 1_000n,
        }),
      ]);
    }
    const overCapPda = await findMilestonePda(capAgreement, MAX_MILESTONES);
    const overflow = await expectTransactionFailure(() =>
      sendTransaction(rpc, capClient, [
        getCreateMilestoneInstruction({
          client: capClient.address,
          agreement: capAgreement,
          milestone: overCapPda,
          index: MAX_MILESTONES,
          amount: 1_000n,
        }),
      ]),
    );
    expect(overflow.logs.join('\n')).toMatch(/TooManyMilestones|6016/);
  });
});
