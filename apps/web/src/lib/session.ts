'use client';

/**
 * Sign-In-With-Solana for the optional metadata API.
 *
 * The signature proves wallet ownership to the indexer only. It is not a
 * transaction, moves no funds and grants no spending authority. The API stores
 * an HttpOnly session cookie; the chain remains the source of truth.
 */
import { getBase58Decoder } from '@solana/kit';

import {
  ApiUnavailableError,
  createChallenge,
  getMe,
  verifyChallenge,
  type ApiSession,
} from './api';
import { CLUSTER } from './config';
import {
  signMessageBytes,
  supportsSignMessage,
  type StandardWallet,
  type WalletAccount,
} from './wallet';

export class SignInUnavailableError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'SignInUnavailableError';
  }
}

export async function signInWithSolana(
  wallet: StandardWallet,
  account: WalletAccount,
): Promise<ApiSession> {
  if (!supportsSignMessage(wallet)) {
    throw new SignInUnavailableError(
      `${wallet.name} does not expose message signing, so it cannot sign in to the indexer. Chain actions still work.`,
    );
  }

  let challenge;
  try {
    challenge = await createChallenge({
      address: account.address,
      cluster: CLUSTER,
    });
  } catch (error) {
    if (error instanceof ApiUnavailableError) {
      throw new SignInUnavailableError(
        'The metadata API is not running. The escrow still works from chain state.',
      );
    }
    throw error;
  }

  const signature = await signMessageBytes(
    wallet,
    account,
    new TextEncoder().encode(challenge.message),
  );
  const signatureBase58 = getBase58Decoder().decode(signature);
  const { user, csrfToken } = await verifyChallenge({
    challengeId: challenge.challengeId,
    signature: signatureBase58,
  });

  return { user, csrfToken, cluster: CLUSTER };
}

/** Restores a session cookie if one is still valid. */
export async function restoreSession(): Promise<ApiSession | null> {
  try {
    return await getMe();
  } catch {
    return null;
  }
}
