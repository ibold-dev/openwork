'use client';

/**
 * Instruction composition for the agreement screen.
 *
 * Every builder here is pure: it takes addresses and returns Kit instructions.
 * Signing and sending happen in `lib/chain.ts`.
 */
import type { Address, Instruction } from '@solana/kit';

import {
  getAcceptAgreementInstruction,
  getApproveAndReleaseInstruction,
  getCancelDraftAgreementInstruction,
  getFundMilestoneInstruction,
  getMutualCancelAndRefundInstruction,
  getRaiseDisputeInstruction,
  getSubmitMilestoneInstruction,
} from '@openwork/sdk';

export interface AgreementRefs {
  readonly agreement: Address;
  readonly client: Address;
  readonly freelancer: Address;
  readonly mint: Address;
  readonly vault: Address;
  readonly vaultAuthority: Address;
}

export function acceptTerms(refs: AgreementRefs, termsHash: Uint8Array): Instruction[] {
  return [
    getAcceptAgreementInstruction({
      freelancer: refs.freelancer,
      agreement: refs.agreement,
      termsHash,
    }),
  ];
}

export function cancelDraft(refs: AgreementRefs): Instruction[] {
  return [
    getCancelDraftAgreementInstruction({
      client: refs.client,
      agreement: refs.agreement,
    }),
  ];
}

export function fundMilestone(input: {
  refs: AgreementRefs;
  milestone: Address;
  clientTokenAccount: Address;
}): Instruction[] {
  return [
    getFundMilestoneInstruction({
      client: input.refs.client,
      agreement: input.refs.agreement,
      milestone: input.milestone,
      mint: input.refs.mint,
      clientTokenAccount: input.clientTokenAccount,
      vault: input.refs.vault,
      vaultAuthority: input.refs.vaultAuthority,
    }),
  ];
}

export function submitDeliverable(input: {
  refs: AgreementRefs;
  milestone: Address;
  deliverableHash: Uint8Array;
}): Instruction[] {
  return [
    getSubmitMilestoneInstruction({
      freelancer: input.refs.freelancer,
      agreement: input.refs.agreement,
      milestone: input.milestone,
      deliverableHash: input.deliverableHash,
    }),
  ];
}

export function releaseMilestone(input: {
  refs: AgreementRefs;
  milestone: Address;
  freelancerTokenAccount: Address;
}): Instruction[] {
  return [
    getApproveAndReleaseInstruction({
      client: input.refs.client,
      agreement: input.refs.agreement,
      milestone: input.milestone,
      freelancer: input.refs.freelancer,
      mint: input.refs.mint,
      freelancerTokenAccount: input.freelancerTokenAccount,
      vault: input.refs.vault,
      vaultAuthority: input.refs.vaultAuthority,
    }),
  ];
}

export function raiseDispute(input: {
  refs: AgreementRefs;
  milestone: Address;
  signer: Address;
}): Instruction[] {
  return [
    getRaiseDisputeInstruction({
      participant: input.signer,
      agreement: input.refs.agreement,
      milestone: input.milestone,
    }),
  ];
}

export function mutualRefund(input: {
  refs: AgreementRefs;
  milestone: Address;
  clientTokenAccount: Address;
}): Instruction[] {
  return [
    getMutualCancelAndRefundInstruction({
      client: input.refs.client,
      freelancer: input.refs.freelancer,
      agreement: input.refs.agreement,
      milestone: input.milestone,
      clientTokenAccount: input.clientTokenAccount,
      vault: input.refs.vault,
      vaultAuthority: input.refs.vaultAuthority,
    }),
  ];
}

/** base64url-encoded wire transaction for the co-signature handoff. */
export function encodeCoSign(wire: Uint8Array): string {
  let binary = '';
  for (const byte of wire) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export function decodeCoSign(value: string): Uint8Array | null {
  try {
    const padded = value.replace(/-/g, '+').replace(/_/g, '/');
    const binary = atob(padded.padEnd(Math.ceil(padded.length / 4) * 4, '='));
    return Uint8Array.from(binary, (char) => char.charCodeAt(0));
  } catch {
    return null;
  }
}
