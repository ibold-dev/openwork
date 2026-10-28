/**
 * Canonical agreement terms and their hash.
 *
 * The hash is committed on-chain at `create_agreement` and re-checked by
 * `accept_agreement`, so the freelancer can prove which terms they accepted.
 *
 * Canonical form is a single-line JSON object with a fixed key order and
 * decimal-string amounts. The Rust API implements the identical encoding;
 * `tests/unit.test.ts` pins a cross-language fixture.
 */
import type { Address } from '@solana/kit';

export interface TermsMilestoneInput {
  readonly index: number;
  readonly title: string;
  readonly description: string;
  /** Base units as a bigint. Serialized as a decimal string. */
  readonly amount: bigint;
}

export interface TermsInput {
  readonly title: string;
  readonly description: string;
  readonly client: Address;
  readonly freelancer: Address;
  readonly mint: Address;
  readonly milestones: readonly TermsMilestoneInput[];
}

export const TERMS_VERSION = 1;
export const MAX_TITLE_LENGTH = 200;
export const MAX_DESCRIPTION_LENGTH = 5000;
export const MAX_MILESTONE_TITLE = 200;
export const MAX_MILESTONE_DESCRIPTION = 2000;

export class TermsValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'TermsValidationError';
  }
}

/** Validates terms and returns the canonical JSON string that is hashed. */
export function canonicalizeTerms(input: TermsInput): string {
  if (input.title.trim().length === 0) {
    throw new TermsValidationError('title is required');
  }
  if (input.title.length > MAX_TITLE_LENGTH) {
    throw new TermsValidationError(
      `title must be at most ${MAX_TITLE_LENGTH} characters`,
    );
  }
  if (input.description.length > MAX_DESCRIPTION_LENGTH) {
    throw new TermsValidationError(
      `description must be at most ${MAX_DESCRIPTION_LENGTH} characters`,
    );
  }
  if (input.client === input.freelancer) {
    throw new TermsValidationError('client and freelancer must differ');
  }
  if (input.milestones.length === 0) {
    throw new TermsValidationError('at least one milestone is required');
  }

  const milestones = input.milestones.map((milestone, position) => {
    if (milestone.index !== position) {
      throw new TermsValidationError(
        `milestone index ${milestone.index} is not contiguous at position ${position}`,
      );
    }
    if (milestone.amount <= 0n) {
      throw new TermsValidationError('milestone amount must be greater than zero');
    }
    if (milestone.title.trim().length === 0) {
      throw new TermsValidationError(`milestone ${position} requires a title`);
    }
    if (milestone.title.length > MAX_MILESTONE_TITLE) {
      throw new TermsValidationError(`milestone ${position} title is too long`);
    }
    if (milestone.description.length > MAX_MILESTONE_DESCRIPTION) {
      throw new TermsValidationError(`milestone ${position} description is too long`);
    }
    return {
      index: milestone.index,
      title: milestone.title,
      description: milestone.description,
      amount: milestone.amount.toString(10),
    };
  });

  // Key order is the canonical order. Do not sort keys.
  return JSON.stringify({
    version: TERMS_VERSION,
    title: input.title,
    description: input.description,
    client: input.client,
    freelancer: input.freelancer,
    mint: input.mint,
    milestones,
  });
}

/** SHA-256 of the canonical terms, as 32 raw bytes. */
export async function hashTerms(input: TermsInput): Promise<Uint8Array> {
  const canonical = canonicalizeTerms(input);
  const digest = await crypto.subtle.digest(
    'SHA-256',
    new TextEncoder().encode(canonical),
  );
  return new Uint8Array(digest);
}

export async function hashDeliverable(input: {
  readonly uri: string;
  readonly note: string;
  readonly milestoneIndex: number;
  readonly agreement: string;
}): Promise<Uint8Array> {
  const canonical = JSON.stringify({
    kind: 'openwork.deliverable.v1',
    agreement: input.agreement,
    milestoneIndex: input.milestoneIndex,
    uri: input.uri,
    note: input.note,
  });
  const digest = await crypto.subtle.digest(
    'SHA-256',
    new TextEncoder().encode(canonical),
  );
  return new Uint8Array(digest);
}
