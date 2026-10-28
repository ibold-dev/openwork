/**
 * Anchor instruction discriminators: first 8 bytes of sha256("global:<name>").
 *
 * Hardcoded so the SDK has no hashing dependency at import time. The unit test
 * re-derives them from the same rule and would fail on any drift.
 */
export const IX_DISCRIMINATORS = {
  create_agreement: 'dc9c41acfc444ae9',
  create_milestone: 'ef3ac91c28baad30',
  accept_agreement: '456301fa9887c4a0',
  cancel_draft_agreement: 'ed9cb4409eab6f7a',
  fund_milestone: '6882484c543a25b5',
  submit_milestone: '2360dcd766538b34',
  approve_and_release: '3ffe9a2d952050ec',
  raise_dispute: '29f30133965ff649',
  mutual_cancel_and_refund: '00497fe0b5914bc4',
} as const;

export type InstructionName = keyof typeof IX_DISCRIMINATORS;

/** Anchor account discriminators: first 8 bytes of sha256("account:<Name>"). */
export const ACCOUNT_DISCRIMINATORS = {
  Agreement: '53d4056ee1f9c554',
  Milestone: '26d2efb155b80a2c',
} as const;

export type AccountName = keyof typeof ACCOUNT_DISCRIMINATORS;
