/**
 * Wallet Standard discovery and signing.
 *
 * OpenWork does not bundle a wallet adapter: it speaks the Wallet Standard the
 * wallet extensions already register. That keeps the dependency surface small
 * and means any Solana wallet works without a per-wallet integration.
 */

export interface WalletAccount {
  readonly address: string;
  readonly publicKey: Uint8Array;
  readonly chains: readonly string[];
  readonly features: readonly string[];
  readonly label?: string;
  readonly icon?: string;
}

export interface SignTransactionFeature {
  signTransaction(input: {
    transaction: Uint8Array;
    account: WalletAccount;
    chain: string;
  }): Promise<readonly { signedTransaction: Uint8Array }[]>;
}

export interface SignMessageFeature {
  signMessage(input: {
    message: Uint8Array;
    account: WalletAccount;
  }): Promise<readonly { signedMessage: Uint8Array; signature: Uint8Array }[]>;
}

export interface StandardWallet {
  readonly name: string;
  readonly icon: string;
  readonly chains: readonly string[];
  readonly accounts: readonly WalletAccount[];
  readonly features: Record<string, unknown>;
}

interface ConnectFeature {
  connect(input?: { silent?: boolean }): Promise<{ accounts: readonly WalletAccount[] }>;
}

interface RegisterPayload {
  register(...wallets: StandardWallet[]): () => void;
}

const registry = new Map<string, StandardWallet>();
const listeners = new Set<(wallets: readonly StandardWallet[]) => void>();
let started = false;

function emit(): void {
  const snapshot = Array.from(registry.values());
  for (const listener of listeners) listener(snapshot);
}

function register(...wallets: StandardWallet[]): () => void {
  for (const wallet of wallets) {
    registry.set(wallet.name, wallet);
  }
  emit();
  return () => {
    for (const wallet of wallets) registry.delete(wallet.name);
    emit();
  };
}

/** Starts listening for wallets. Safe to call repeatedly. */
export function startWalletDiscovery(): void {
  if (started || typeof window === 'undefined') return;
  started = true;

  window.addEventListener('wallet-standard:register-wallet', ((
    event: CustomEvent<RegisterPayload>,
  ) => {
    event.detail.register(...Array.from(registry.values()));
    // The wallet also offers a register callback for wallets it knows about.
    const originalRegister = event.detail.register.bind(event.detail);
    event.detail.register = (...wallets: StandardWallet[]) => {
      const cleanup = register(...wallets);
      const walletCleanup = originalRegister(...wallets);
      return () => {
        cleanup();
        walletCleanup();
      };
    };
  }) as EventListener);

  window.dispatchEvent(
    new CustomEvent<RegisterPayload>('wallet-standard:app-ready', {
      detail: { register },
    }),
  );
}

export function subscribeWallets(
  listener: (wallets: readonly StandardWallet[]) => void,
): () => void {
  listeners.add(listener);
  listener(Array.from(registry.values()));
  return () => listeners.delete(listener);
}

export function findWallet(name: string): StandardWallet | undefined {
  return registry.get(name);
}

export async function connectWallet(
  wallet: StandardWallet,
): Promise<readonly WalletAccount[]> {
  const feature = wallet.features['standard:connect'] as ConnectFeature | undefined;
  if (!feature) {
    throw new Error(`${wallet.name} does not support connect()`);
  }
  const { accounts } = await feature.connect();
  const usable = accounts.filter((account) =>
    account.chains.some((chain) => chain.startsWith('solana:')),
  );
  if (usable.length === 0) {
    throw new Error(`${wallet.name} returned no Solana account`);
  }
  return usable;
}

export function supportsSignTransaction(wallet: StandardWallet): boolean {
  return typeof wallet.features['solana:signTransaction'] === 'object';
}

export function supportsSignMessage(wallet: StandardWallet): boolean {
  return typeof wallet.features['solana:signMessage'] === 'object';
}

/**
 * Signs a plain message (SIWS). This is not a transaction: it cannot move
 * funds and wallets show it as a text signature prompt.
 */
export async function signMessageBytes(
  wallet: StandardWallet,
  account: WalletAccount,
  message: Uint8Array,
): Promise<Uint8Array> {
  const feature = wallet.features['solana:signMessage'] as SignMessageFeature | undefined;
  if (!feature) {
    throw new Error(`${wallet.name} cannot sign messages`);
  }
  const [result] = await feature.signMessage({ message, account });
  if (!result?.signature) {
    throw new Error(`${wallet.name} returned no signature`);
  }
  return result.signature;
}

/** Signs wire-format transaction bytes with the connected wallet. */
export async function signTransactionBytes(
  wallet: StandardWallet,
  account: WalletAccount,
  chain: string,
  transaction: Uint8Array,
): Promise<Uint8Array> {
  const feature = wallet.features['solana:signTransaction'] as
    SignTransactionFeature | undefined;
  if (!feature) {
    throw new Error(`${wallet.name} cannot sign transactions`);
  }
  const [result] = await feature.signTransaction({ transaction, account, chain });
  if (!result?.signedTransaction) {
    throw new Error(`${wallet.name} returned no signed transaction`);
  }
  return result.signedTransaction;
}
