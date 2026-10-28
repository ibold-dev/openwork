import { createHash } from 'node:crypto';

import type { AccountMeta, Address, Instruction, ReadonlyUint8Array } from '@solana/kit';

import { describe, expect, it } from 'vitest';

import {
  ACCOUNT_DISCRIMINATORS,
  AGREEMENT_ACCOUNT_SIZE,
  ESCROW_ERROR_MESSAGES,
  IX_DISCRIMINATORS,
  MILESTONE_ACCOUNT_SIZE,
  AccountDecodeError,
  MAX_MILESTONES,
  TermsValidationError,
  base58Decode,
  base58Encode,
  bytesToHex,
  canonicalizeTerms,
  decodeAgreementAccount,
  decodeEscrowError,
  decodeMilestoneAccount,
  describeTransactionError,
  extractCustomErrorCode,
  findAgreementPda,
  findMilestonePda,
  findVaultAuthorityPda,
  getAcceptAgreementInstruction,
  getApproveAndReleaseInstruction,
  getCreateAgreementInstruction,
  getFundMilestoneInstruction,
  hashTerms,
  hexToBytes,
  type TermsInput,
} from '../src/index.js';
import idl from '../idl/openwork_escrow.json' with { type: 'json' };

const CLIENT = '4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T' as Address;
const FREELANCER = '9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM' as Address;
const MINT = 'So11111111111111111111111111111111111111112' as Address;

/** Test-only helper for synthetic token accounts. */
const addr = (value: string): Address => value as Address;

/** Kit's `Instruction` marks `data`/`accounts` optional; ours always set them. */
function dataOf(ix: Instruction): ReadonlyUint8Array {
  if (!ix.data) throw new Error('instruction has no data');
  return ix.data;
}

function accountsOf(ix: Instruction): readonly AccountMeta[] {
  if (!ix.accounts) throw new Error('instruction has no accounts');
  return ix.accounts;
}

describe('instruction discriminators', () => {
  it('are sha256("global:<name>")[:8] for every instruction in the IDL', () => {
    const idlNames = idl.instructions.map((ix) => ix.name).sort();
    const sdkNames = Object.keys(IX_DISCRIMINATORS).sort();
    expect(sdkNames).toEqual(idlNames);

    for (const [name, expected] of Object.entries(IX_DISCRIMINATORS)) {
      const derived = createHash('sha256')
        .update(`global:${name}`)
        .digest('hex')
        .slice(0, 16);
      expect(derived, `${name} discriminator drift`).toBe(expected);
    }
  });

  it('are sha256("account:<Name>")[:8] for every account in the IDL', () => {
    const idlNames = idl.accounts.map((account) => account.name).sort();
    const sdkNames = Object.keys(ACCOUNT_DISCRIMINATORS).sort();
    expect(sdkNames).toEqual(idlNames);

    for (const [name, expected] of Object.entries(ACCOUNT_DISCRIMINATORS)) {
      const derived = createHash('sha256')
        .update(`account:${name}`)
        .digest('hex')
        .slice(0, 16);
      expect(derived, `${name} discriminator drift`).toBe(expected);
    }
  });
});

describe('program errors', () => {
  it('covers every error code declared in the IDL', () => {
    const idlCodes = idl.errors.map((error) => error.code).sort((a, b) => a - b);
    // Anchor assigns 6000.. sequentially; assert we implement the full range.
    expect(idlCodes.length).toBe(ESCROW_ERROR_MESSAGES.length);
    expect(idlCodes[0]).toBe(6000);
    for (const [index, code] of idlCodes.entries()) {
      expect(code).toBe(6000 + index);
    }
  });

  it('decodes custom errors and preserves unknown codes', () => {
    expect(decodeEscrowError(6000)).toMatchObject({
      name: 'UnauthorizedClient',
      message: 'Only the agreement client may perform this action',
    });
    expect(decodeEscrowError(6018).name).toBe('TermsHashMismatch');
    expect(decodeEscrowError(9999).message).toContain('Unknown program error');
  });

  it('extracts custom error codes from simulation logs', () => {
    const log =
      'Program log: Instruction: ApproveAndRelease\ncustom program error: 0x177a';
    expect(extractCustomErrorCode(log)).toBe(6010);
    expect(describeTransactionError(log)).toContain('MilestoneDisputed');
    expect(extractCustomErrorCode('no error here')).toBeNull();
  });
});

describe('PDA derivation', () => {
  // Fixtures generated with `solana find-program-derived-address` against the
  // deployed program id. A mismatch means the seed encoding changed.
  it('derives the agreement PDA exactly like the program', async () => {
    await expect(findAgreementPda(CLIENT, 7n)).resolves.toBe(
      'FLbQEP4d4GwJxCzefqnxVuuDuhonodRmLTwTyd9meZdC',
    );
  });

  it('derives the milestone and vault authority PDAs exactly like the program', async () => {
    const agreement = await findAgreementPda(CLIENT, 7n);
    await expect(findMilestonePda(agreement, 0)).resolves.toBe(
      'CW9Ffm8CDTqQTHTutjKh31SKaV5uDVFdW5jcdBJSUV4e',
    );
    await expect(findVaultAuthorityPda(agreement)).resolves.toBe(
      'FXKEGbwZGi7LAMos3pbYbaip7W4xXGNzV6anttDR91w1',
    );
  });

  it('is deterministic and nonce-sensitive', async () => {
    const a = await findAgreementPda(CLIENT, 1n);
    const b = await findAgreementPda(CLIENT, 1n);
    const c = await findAgreementPda(CLIENT, 2n);
    expect(a).toBe(b);
    expect(a).not.toBe(c);
  });

  it('rejects out-of-range milestone indexes', async () => {
    const agreement = await findAgreementPda(CLIENT, 1n);
    await expect(findMilestonePda(agreement, -1)).rejects.toThrow();
    await expect(findMilestonePda(agreement, 65_536)).rejects.toThrow();
    await expect(findMilestonePda(agreement, 1.5)).rejects.toThrow();
  });
});

describe('instruction encoding', () => {
  it('packs create_agreement payload in program order', async () => {
    const agreement = await findAgreementPda(CLIENT, 7n);
    const ix = getCreateAgreementInstruction({
      client: CLIENT,
      agreement,
      freelancer: FREELANCER,
      mint: MINT,
      nonce: 7n,
      termsHash: new Uint8Array(32).fill(3),
    });

    expect(bytesToHex(dataOf(ix).slice(0, 8))).toBe(IX_DISCRIMINATORS.create_agreement);
    expect(dataOf(ix).length).toBe(8 + 8 + 32 + 32 + 32);
    // nonce follows the discriminator as little-endian u64
    expect(bytesToHex(dataOf(ix).slice(8, 16))).toBe('0700000000000000');
    // then freelancer, then mint as raw 32-byte keys
    expect(bytesToHex(dataOf(ix).slice(16, 48))).toBe(
      bytesToHex(base58Decode(FREELANCER)),
    );
    expect(bytesToHex(dataOf(ix).slice(48, 80))).toBe(bytesToHex(base58Decode(MINT)));
    expect(accountsOf(ix).map((account) => account.address)).toEqual([
      CLIENT,
      agreement,
      '11111111111111111111111111111111',
    ]);
    expect(accountsOf(ix)[0]!.role).toBe(3); // writable signer
  });

  it('rejects hashes that are not 32 bytes', async () => {
    const agreement = await findAgreementPda(CLIENT, 7n);
    expect(() =>
      getCreateAgreementInstruction({
        client: CLIENT,
        agreement,
        freelancer: FREELANCER,
        mint: MINT,
        nonce: 7n,
        termsHash: new Uint8Array(31),
      }),
    ).toThrow(TypeError);
    expect(() =>
      getAcceptAgreementInstruction({
        freelancer: FREELANCER,
        agreement,
        termsHash: new Uint8Array(33),
      }),
    ).toThrow(TypeError);
  });

  it('fund_milestone account order matches the program struct', async () => {
    const agreement = await findAgreementPda(CLIENT, 7n);
    const milestone = await findMilestonePda(agreement, 0);
    const vaultAuthority = await findVaultAuthorityPda(agreement);
    const ix = getFundMilestoneInstruction({
      client: CLIENT,
      agreement,
      milestone,
      mint: MINT,
      clientTokenAccount: addr('A'),
      vault: addr('B'),
      vaultAuthority,
    });
    expect(accountsOf(ix)).toHaveLength(10);
    expect(accountsOf(ix).map((account) => account.address)).toEqual([
      CLIENT,
      agreement,
      milestone,
      MINT,
      'A',
      'B',
      vaultAuthority,
      'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA',
      'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL',
      '11111111111111111111111111111111',
    ]);
    expect(accountsOf(ix)[0]!.role).toBe(3);
    expect(accountsOf(ix)[2]!.role).toBe(1); // writable, not signer
  });

  it('approve_and_release account order matches the program struct', async () => {
    const agreement = await findAgreementPda(CLIENT, 7n);
    const milestone = await findMilestonePda(agreement, 0);
    const vaultAuthority = await findVaultAuthorityPda(agreement);
    const ix = getApproveAndReleaseInstruction({
      client: CLIENT,
      agreement,
      milestone,
      freelancer: FREELANCER,
      mint: MINT,
      freelancerTokenAccount: addr('C'),
      vault: addr('D'),
      vaultAuthority,
    });
    expect(accountsOf(ix).map((account) => account.address)).toEqual([
      CLIENT,
      agreement,
      milestone,
      FREELANCER,
      MINT,
      'C',
      'D',
      vaultAuthority,
      'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA',
    ]);
  });
});

describe('base58 codec', () => {
  it('round-trips 32-byte keys including leading zeros', () => {
    const allZero = new Uint8Array(32);
    expect(base58Encode(allZero)).toBe('11111111111111111111111111111111');
    expect(bytesToHex(base58Decode('11111111111111111111111111111111'))).toBe(
      '00'.repeat(32),
    );

    const random = Uint8Array.from({ length: 32 }, (_, i) => (i * 7 + 3) % 256);
    expect(base58Decode(base58Encode(random))).toEqual(random);
    expect(base58Decode(CLIENT)).toHaveLength(32);
  });

  it('rejects malformed input', () => {
    expect(() => base58Decode('0OIl')).toThrow();
    expect(() => base58Decode('abc')).toThrow(/32-byte/);
  });
});

describe('terms canonicalization', () => {
  const terms: TermsInput = {
    title: 'Landing page rebuild',
    description: 'Two-week engagement, milestone-based.',
    client: CLIENT,
    freelancer: FREELANCER,
    mint: MINT,
    milestones: [
      { index: 0, title: 'Design', description: 'Wireframes', amount: 250_000_000n },
      { index: 1, title: 'Build', description: 'Next.js', amount: 500_000_000n },
    ],
  };

  it('produces a stable cross-language hash', async () => {
    const canonical = canonicalizeTerms(terms);
    // Fixture also asserted by crates/domain `terms::tests`, so a change in
    // either canonicalizer fails CI.
    expect(bytesToHex(await hashTerms(terms))).toBe(
      '2c31b945b06cdaadd11566ac6948dbedab0788d98578c72d78c5edaf4b9408d6',
    );
  });

  it('serializes amounts as decimal strings, never numbers', () => {
    const canonical = canonicalizeTerms(terms);
    expect(canonical).toContain('"amount":"250000000"');
    expect(canonical).not.toContain('"amount":250000000');
    expect(JSON.parse(canonical).version).toBe(1);
  });

  it('is key-order stable and rejects bad input', () => {
    const canonical = canonicalizeTerms(terms);
    expect(Object.keys(JSON.parse(canonical))).toEqual([
      'version',
      'title',
      'description',
      'client',
      'freelancer',
      'mint',
      'milestones',
    ]);

    expect(() => canonicalizeTerms({ ...terms, title: '  ' })).toThrow(
      TermsValidationError,
    );
    expect(() => canonicalizeTerms({ ...terms, client: FREELANCER })).toThrow(
      /must differ/,
    );
    expect(() =>
      canonicalizeTerms({
        ...terms,
        milestones: [{ ...terms.milestones[0]!, index: 1 }],
      }),
    ).toThrow(/contiguous/);
    expect(() =>
      canonicalizeTerms({
        ...terms,
        milestones: [{ ...terms.milestones[0]!, amount: 0n }],
      }),
    ).toThrow(/greater than zero/);
    expect(() =>
      canonicalizeTerms({
        ...terms,
        milestones: Array.from({ length: MAX_MILESTONES + 1 }, (_, index) => ({
          index,
          title: 'x',
          description: '',
          amount: 1n,
        })),
      }),
    ).not.toThrow(); // count is enforced on-chain; the SDK layout allows it
  });
});

interface AgreementOverrides {
  readonly schemaVersion?: number;
  readonly bump?: number;
  readonly state?: number;
  readonly client?: string;
  readonly freelancer?: string;
  readonly mint?: string;
  readonly agreementNonce?: bigint;
  readonly termsHash?: Uint8Array;
  readonly milestoneCount?: number;
  readonly settledCount?: number;
  readonly refundedCount?: number;
  readonly totalAmount?: bigint;
  readonly releasedAmount?: bigint;
  readonly createdAt?: bigint;
  readonly acceptedAt?: bigint;
}

function syntheticAgreement(overrides: AgreementOverrides = {}): Uint8Array {
  const buffer = new Uint8Array(AGREEMENT_ACCOUNT_SIZE);
  const view = new DataView(buffer.buffer);

  buffer.set(hexToBytes(ACCOUNT_DISCRIMINATORS.Agreement), 0);
  let offset = 8;

  buffer[offset++] = overrides.schemaVersion ?? 1;
  buffer[offset++] = overrides.bump ?? 254;
  buffer[offset++] = overrides.state ?? 1;
  buffer.set(base58Decode(overrides.client ?? CLIENT), offset);
  offset += 32;
  buffer.set(base58Decode(overrides.freelancer ?? FREELANCER), offset);
  offset += 32;
  buffer.set(base58Decode(overrides.mint ?? MINT), offset);
  offset += 32;
  view.setBigUint64(offset, overrides.agreementNonce ?? 7n, true);
  offset += 8;
  buffer.set(overrides.termsHash ?? new Uint8Array(32).fill(5), offset);
  offset += 32;
  view.setUint16(offset, overrides.milestoneCount ?? 2, true);
  offset += 2;
  view.setUint16(offset, overrides.settledCount ?? 1, true);
  offset += 2;
  view.setUint16(offset, overrides.refundedCount ?? 0, true);
  offset += 2;
  view.setBigUint64(offset, overrides.totalAmount ?? 750_000_000n, true);
  offset += 8;
  view.setBigUint64(offset, overrides.releasedAmount ?? 250_000_000n, true);
  offset += 8;
  view.setBigInt64(offset, overrides.createdAt ?? 1_760_000_000n, true);
  offset += 8;
  view.setBigInt64(offset, overrides.acceptedAt ?? 1_760_000_100n, true);
  offset += 8;

  expect(offset).toBe(AGREEMENT_ACCOUNT_SIZE);
  return buffer;
}

describe('account decoding', () => {
  it('decodes an Agreement written with the on-chain layout', () => {
    const decoded = decodeAgreementAccount(syntheticAgreement());
    expect(decoded).toMatchObject({
      schemaVersion: 1,
      bump: 254,
      state: 'active',
      client: CLIENT,
      freelancer: FREELANCER,
      mint: MINT,
      agreementNonce: 7n,
      milestoneCount: 2,
      settledCount: 1,
      refundedCount: 0,
      totalAmount: 750_000_000n,
      releasedAmount: 250_000_000n,
      createdAt: 1_760_000_000n,
      acceptedAt: 1_760_000_100n,
    });
    expect(decoded.termsHash).toEqual(new Uint8Array(32).fill(5));
  });

  it('decodes every agreement state', () => {
    const states = ['draft', 'active', 'completed', 'cancelled'];
    for (const [index, state] of states.entries()) {
      expect(decodeAgreementAccount(syntheticAgreement({ state: index })).state).toBe(
        state,
      );
    }
    expect(() => decodeAgreementAccount(syntheticAgreement({ state: 9 }))).toThrow(
      /unknown agreement state/,
    );
  });

  it('decodes a Milestone written with the on-chain layout', () => {
    const buffer = new Uint8Array(MILESTONE_ACCOUNT_SIZE);
    const view = new DataView(buffer.buffer);
    buffer.set(hexToBytes(ACCOUNT_DISCRIMINATORS.Milestone), 0);
    view.setUint16(8, 1, true);
    buffer[10] = 252;
    buffer[11] = 2; // submitted (Pending, Funded, Submitted, ...)
    buffer.set(base58Decode('FLbQEP4d4GwJxCzefqnxVuuDuhonodRmLTwTyd9meZdC'), 12);
    view.setBigUint64(44, 500_000_000n, true);
    buffer.set(new Uint8Array(32).fill(9), 52);
    view.setBigInt64(84, 1_760_000_000n, true);
    view.setBigInt64(92, 1_760_000_200n, true);
    view.setBigInt64(100, 0n, true);

    const decoded = decodeMilestoneAccount(buffer);
    expect(decoded).toEqual({
      index: 1,
      bump: 252,
      state: 'submitted',
      agreement: 'FLbQEP4d4GwJxCzefqnxVuuDuhonodRmLTwTyd9meZdC',
      amount: 500_000_000n,
      deliverableHash: new Uint8Array(32).fill(9),
      fundedAt: 1_760_000_000n,
      submittedAt: 1_760_000_200n,
      settledAt: 0n,
    });
  });

  it('rejects wrong discriminators and sizes', () => {
    const agreement = syntheticAgreement();
    const milestone = new Uint8Array(MILESTONE_ACCOUNT_SIZE);
    milestone.set(hexToBytes(ACCOUNT_DISCRIMINATORS.Milestone), 0);

    expect(() => decodeMilestoneAccount(agreement)).toThrow(AccountDecodeError);
    expect(() => decodeAgreementAccount(milestone)).toThrow(AccountDecodeError);
    expect(() => decodeAgreementAccount(agreement.slice(0, 100))).toThrow(
      /too short|end of account|size/,
    );
    expect(() => decodeAgreementAccount(new Uint8Array(8))).toThrow();
  });

  it('pins the on-chain account sizes', () => {
    expect(AGREEMENT_ACCOUNT_SIZE).toBe(185);
    expect(MILESTONE_ACCOUNT_SIZE).toBe(108);
    const idlAgreement = idl.accounts.find((account) => account.name === 'Agreement');
    // The IDL lists fields, not a size; the size assertion above is the guard
    // that the Rust `Agreement::LEN` test also asserts.
    expect(idlAgreement?.discriminator).toHaveLength(8);
  });
});
