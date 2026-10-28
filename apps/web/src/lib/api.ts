'use client';

/**
 * Optional metadata API client.
 *
 * The API indexes off-chain titles, descriptions and funding quotes; it never
 * signs and never holds funds. When it is unreachable the app keeps working
 * from chain state alone and shows a "chain-only" badge instead of inventing
 * data.
 *
 * Request and response shapes mirror `apps/api/src/routes/dto.rs`.
 */

export interface ApiUser {
  readonly id: string;
  readonly wallet: string;
  readonly createdAt: string;
}

export interface ApiMilestone {
  readonly id: string;
  readonly contractId: string;
  readonly chain: string;
  readonly index: number;
  readonly title: string;
  readonly description: string;
  readonly amount: string;
  readonly state: string;
  readonly chainState?: string | null;
  readonly milestonePda: string;
  readonly deliverableUri?: string | null;
  readonly deliverableNote?: string | null;
  readonly deliverableHash?: string | null;
  readonly fundedAt?: string | null;
  readonly submittedAt?: string | null;
  readonly settledAt?: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface ApiContract {
  readonly id: string;
  readonly chain: string;
  readonly role: 'client' | 'freelancer' | 'observer';
  readonly client: string;
  readonly freelancer: string;
  readonly mint: string;
  readonly title: string;
  readonly description: string;
  readonly termsHash: string;
  readonly termsHashBytes: readonly number[];
  readonly agreementPda: string;
  readonly agreementNonce: string;
  readonly state: string;
  readonly draftState: string;
  readonly totalAmount: string;
  readonly milestoneCount: number;
  readonly milestones: readonly ApiMilestone[];
  readonly acceptedIntentAt?: string | null;
  readonly chainState?: string | null;
  readonly chainSyncedAt?: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface ApiQuote {
  readonly id: string;
  readonly contractId?: string | null;
  readonly mode: 'mock' | 'mainnet';
  readonly provider: string;
  readonly externalId: string;
  readonly invoice: string;
  readonly amountIn: string;
  readonly amountOut?: string | null;
  readonly originAsset: string;
  readonly destinationAsset: string;
  readonly recipient: string;
  readonly refundTo: string;
  readonly depositAddress: string;
  readonly depositMemo?: string | null;
  readonly status: string;
  readonly simulated: boolean;
  readonly deadline: string;
  readonly expiresAt: string;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface ApiSession {
  readonly user: ApiUser;
  readonly csrfToken: string;
  readonly cluster: string;
}

export class ApiUnavailableError extends Error {
  constructor(message = 'The OpenWork API is not reachable') {
    super(message);
    this.name = 'ApiUnavailableError';
  }
}

export class ApiRequestError extends Error {
  readonly status: number;

  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiRequestError';
    this.status = status;
  }
}

let csrfToken: string | null = null;

export function setCsrfToken(token: string | null): void {
  csrfToken = token;
}

async function request<T>(
  path: string,
  init: RequestInit & { signal?: AbortSignal } = {},
): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set('accept', 'application/json');
  if (init.body) headers.set('content-type', 'application/json');
  if (csrfToken && init.method && init.method !== 'GET') {
    headers.set('x-openwork-csrf', csrfToken);
  }

  let response: Response;
  try {
    response = await fetch(`/api/proxy${path}`, {
      ...init,
      headers,
      credentials: 'include',
      signal: init.signal ?? AbortSignal.timeout(10_000),
    });
  } catch (error) {
    throw new ApiUnavailableError(
      error instanceof Error ? error.message : 'network error',
    );
  }

  if (response.status === 204) return undefined as T;
  const text = await response.text();
  let parsed: unknown = null;
  if (text) {
    try {
      parsed = JSON.parse(text) as unknown;
    } catch {
      parsed = null;
    }
  }

  if (!response.ok) {
    const problem = parsed as { error?: { message?: string; code?: string } } | null;
    throw new ApiRequestError(
      problem?.error?.message ?? `API request failed (${response.status})`,
      response.status,
    );
  }
  return parsed as T;
}

/** True when the API answers its liveness probe. Never throws. */
export async function apiLive(): Promise<boolean> {
  try {
    const response = await fetch('/api/proxy/health/live', {
      signal: AbortSignal.timeout(2500),
    });
    return response.ok;
  } catch {
    return false;
  }
}

export async function getMe(): Promise<ApiSession | null> {
  try {
    const session = await request<ApiSession>('/v1/me');
    setCsrfToken(session.csrfToken);
    return session;
  } catch {
    return null;
  }
}

export async function createChallenge(input: {
  address: string;
  cluster: string;
}): Promise<{
  challengeId: string;
  message: string;
  nonce: string;
  expiresAt: string;
}> {
  return request('/v1/auth/challenge', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export async function verifyChallenge(input: {
  challengeId: string;
  /** Base58 signature over the challenge message. */
  signature: string;
}): Promise<{ user: ApiUser; csrfToken: string }> {
  const result = await request<{ user: ApiUser; csrfToken: string }>('/v1/auth/verify', {
    method: 'POST',
    body: JSON.stringify(input),
  });
  setCsrfToken(result.csrfToken);
  return result;
}

export async function logout(): Promise<void> {
  try {
    await request('/v1/auth/logout', { method: 'POST' });
  } finally {
    setCsrfToken(null);
  }
}

export async function listContracts(): Promise<readonly ApiContract[]> {
  const payload = await request<{ contracts: readonly ApiContract[] }>('/v1/contracts');
  return payload.contracts;
}

export async function getContract(id: string): Promise<ApiContract> {
  return request(`/v1/contracts/${id}`);
}

/** Finds an indexed contract by its on-chain agreement address. */
export async function findContractByPda(
  agreementPda: string,
): Promise<ApiContract | null> {
  const contracts = await listContracts();
  return (
    contracts.find(
      (contract) =>
        contract.agreementPda === agreementPda ||
        contract.agreementPda.toLowerCase() === agreementPda.toLowerCase(),
    ) ?? null
  );
}

export interface CreateContractInput {
  readonly freelancer: string;
  readonly mint: string;
  readonly title: string;
  readonly description: string;
  readonly milestones: readonly {
    readonly title: string;
    readonly description: string;
    readonly amount: string;
  }[];
  /**
   * Supplied when the browser already created the agreement on chain, so the
   * index stores the same PDA instead of allocating its own nonce.
   */
  readonly agreementNonce?: string;
  readonly termsHash?: string;
}

export async function createContract(input: CreateContractInput): Promise<ApiContract> {
  return request('/v1/contracts', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export async function submitMilestone(input: {
  milestoneId: string;
  uri: string;
  note: string;
}): Promise<ApiMilestone> {
  return request(`/v1/milestones/${input.milestoneId}/submit`, {
    method: 'POST',
    body: JSON.stringify({ uri: input.uri, note: input.note }),
  });
}

export async function createQuote(input: {
  contractId?: string;
  invoice: string;
  amount: string;
  originAsset: string;
  destinationAsset: string;
  recipient: string;
  refundTo: string;
  deadlineSeconds?: number;
  slippageBps?: number;
}): Promise<ApiQuote> {
  return request('/v1/quotes', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}

export async function getQuote(id: string): Promise<ApiQuote> {
  return request(`/v1/quotes/${id}`);
}

export async function recordSettlement(input: {
  contractId: string;
  milestoneIndex?: number;
  signature: string;
  kind: string;
}): Promise<void> {
  await request('/v1/settlements', {
    method: 'POST',
    body: JSON.stringify(input),
  });
}
