/**
 * Error decoding for the escrow program.
 *
 * Anchor encodes program errors as `0x1770 + code` (6000 + code) in the low
 * four bytes of the custom error word. The mapping below mirrors
 * `programs/openwork-escrow/src/error.rs` in declaration order, so the index
 * of each message is its on-chain code. `tests/unit.test.ts` checks that every
 * program error code in the committed IDL has a message here.
 */

export const ANCHOR_ERROR_OFFSET = 6000;

export const ESCROW_ERROR_MESSAGES = [
  'Only the agreement client may perform this action',
  'Only the agreement freelancer may perform this action',
  'Signer is not a participant of this agreement',
  'Agreement is not in draft state',
  'Agreement is not active',
  'Agreement is already closed',
  'Milestone is not pending funding',
  'Milestone has not been funded',
  'Milestone work has not been submitted',
  'Milestone is already settled',
  'Milestone is disputed and release is frozen',
  'Milestone cannot be disputed in its current state',
  'Client and freelancer must be distinct, non-default wallets',
  'Token mint is invalid for this agreement',
  'Vault token account does not match the agreement',
  'Milestone amount must be greater than zero',
  'Too many milestones for this agreement',
  'Milestone index must be the next contiguous index',
  'Terms hash does not match the committed agreement terms',
  'Agreement must have at least one milestone before acceptance',
  'Funding amount did not match the milestone amount',
  'Deliverable hash must not be empty',
  'Arithmetic overflow',
] as const;

export type EscrowErrorName =
  | 'UnauthorizedClient'
  | 'UnauthorizedFreelancer'
  | 'UnauthorizedParticipant'
  | 'AgreementNotDraft'
  | 'AgreementNotActive'
  | 'AgreementClosed'
  | 'MilestoneNotPending'
  | 'MilestoneNotFunded'
  | 'MilestoneNotSubmitted'
  | 'MilestoneSettled'
  | 'MilestoneDisputed'
  | 'InvalidDisputeState'
  | 'InvalidCounterparty'
  | 'InvalidMint'
  | 'InvalidVault'
  | 'InvalidAmount'
  | 'TooManyMilestones'
  | 'NonContiguousMilestone'
  | 'TermsHashMismatch'
  | 'NoMilestones'
  | 'FundingAmountMismatch'
  | 'InvalidDeliverableHash'
  | 'Overflow';

const ERROR_NAMES: readonly EscrowErrorName[] = [
  'UnauthorizedClient',
  'UnauthorizedFreelancer',
  'UnauthorizedParticipant',
  'AgreementNotDraft',
  'AgreementNotActive',
  'AgreementClosed',
  'MilestoneNotPending',
  'MilestoneNotFunded',
  'MilestoneNotSubmitted',
  'MilestoneSettled',
  'MilestoneDisputed',
  'InvalidDisputeState',
  'InvalidCounterparty',
  'InvalidMint',
  'InvalidVault',
  'InvalidAmount',
  'TooManyMilestones',
  'NonContiguousMilestone',
  'TermsHashMismatch',
  'NoMilestones',
  'FundingAmountMismatch',
  'InvalidDeliverableHash',
  'Overflow',
];

export interface DecodedEscrowError {
  /** Anchor program error code (0-based). */
  readonly code: number;
  /** Error name from `EscrowError`. */
  readonly name: EscrowErrorName;
  readonly message: string;
}

/**
 * Decodes `ProgramError::Custom(n)` into readable text. Unknown codes are
 * returned as-is rather than swallowed, so logs never lie about an error.
 */
export function decodeEscrowError(customCode: number): DecodedEscrowError {
  const index = customCode - ANCHOR_ERROR_OFFSET;
  const name = ERROR_NAMES[index];
  const message = ESCROW_ERROR_MESSAGES[index];
  if (!name || !message) {
    return {
      code: customCode,
      name: 'Overflow',
      message: `Unknown program error code ${customCode}`,
    };
  }
  return { code: customCode, name, message };
}

/**
 * Extracts a custom program error code from a transaction failure message.
 *
 * Kit surfaces simulation failures as human-readable logs; the code appears as
 * `custom program error: 0x1771`. Returns `null` when no code is present so
 * callers can fall back to the raw log.
 */
export function extractCustomErrorCode(log: string): number | null {
  const match = /custom program error: (0x[0-9a-fA-F]+|\d+)/.exec(log);
  if (!match?.[1]) return null;
  const raw = match[1];
  return raw.startsWith('0x') ? Number.parseInt(raw, 16) : Number(raw);
}

/** Human-readable description of a failed transaction. */
export function describeTransactionError(log: string): string {
  const code = extractCustomErrorCode(log);
  if (code === null) return log;
  const decoded = decodeEscrowError(code);
  return `${decoded.name}: ${decoded.message} (code ${decoded.code})`;
}
