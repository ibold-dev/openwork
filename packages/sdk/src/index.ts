/**
 * @openwork/sdk — typed, non-custodial client for the OpenWork escrow program.
 *
 * Everything exported here is pure: it builds transactions and decodes chain
 * state. It never signs, never sends, and never holds keys.
 */

export {
  ACCOUNT_DISCRIMINATORS,
  IX_DISCRIMINATORS,
  type AccountName,
  type InstructionName,
} from './discriminators.js';

export {
  AGREEMENT_SEED,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  MAX_MILESTONES,
  MILESTONE_SEED,
  OPENWORK_ESCROW_PROGRAM_ID,
  SCHEMA_VERSION,
  SYSTEM_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  VAULT_AUTHORITY_SEED,
  bytesToHex,
  concatBytes,
  findAgreementPda,
  findMilestonePda,
  findVaultAuthorityPda,
  hexToBytes,
  i64le,
  u16le,
  u64le,
} from './program.js';

export {
  base58Decode,
  base58Encode,
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
  type CreateAgreementArgs,
  type CreateMilestoneArgs,
  type FundMilestoneAccounts,
} from './instructions.js';

export {
  AGREEMENT_ACCOUNT_SIZE,
  AccountDecodeError,
  MILESTONE_ACCOUNT_SIZE,
  decodeAgreementAccount,
  decodeMilestoneAccount,
  decodeRpcAccountData,
  type AgreementAccount,
  type AgreementState,
  type MilestoneAccount,
  type MilestoneState,
} from './accounts.js';

export {
  MAX_DESCRIPTION_LENGTH,
  MAX_MILESTONE_DESCRIPTION,
  MAX_MILESTONE_TITLE,
  MAX_TITLE_LENGTH,
  TERMS_VERSION,
  TermsValidationError,
  canonicalizeTerms,
  hashDeliverable,
  hashTerms,
  type TermsInput,
  type TermsMilestoneInput,
} from './terms.js';

export {
  ANCHOR_ERROR_OFFSET,
  ESCROW_ERROR_MESSAGES,
  decodeEscrowError,
  describeTransactionError,
  extractCustomErrorCode,
  type DecodedEscrowError,
  type EscrowErrorName,
} from './errors.js';
