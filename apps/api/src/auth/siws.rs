//! Sign-In-With-Solana style message construction and ed25519 verification.

use chrono::{DateTime, SecondsFormat, Utc};
use ed25519_dalek::{Signature, VerifyingKey};
use rand::RngCore;
use sha2::{Digest, Sha256};

use openwork_domain::pda::parse_pubkey;

/// Human-readable statement shown in the wallet.
pub const STATEMENT: &str =
    "Sign in to OpenWork. This request will not trigger a blockchain transaction or cost any fees.";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("invalid wallet address")]
    InvalidAddress,
    #[error("signature must be base58 encoded")]
    InvalidSignatureEncoding,
    #[error("signature must be 64 bytes")]
    InvalidSignatureLength,
    #[error("signature does not match the wallet and message")]
    VerificationFailed,
    #[error("challenge message does not match its signed fields")]
    MessageMismatch,
}

/// Builds the exact message the wallet signs.
///
/// The format is the SIWS layout:
///
/// ```text
/// {domain} wants you to sign in with your Solana account:
/// {address}
///
/// {statement}
///
/// URI: {origin}
/// Version: 1
/// Chain ID: solana:{cluster}
/// Nonce: {nonce}
/// Issued At: {iso8601}
/// Expiration Time: {iso8601}
/// ```
// The SIWS layout has eight independent fields; a struct would only rename the
// argument list without making a wrong field order less likely.
#[allow(clippy::too_many_arguments)]
pub fn build_message(
    domain: &str,
    address: &str,
    statement: &str,
    origin: &str,
    cluster: &str,
    nonce: &str,
    issued_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) -> String {
    format!(
        "{domain} wants you to sign in with your Solana account:\n\
         {address}\n\
         \n\
         {statement}\n\
         \n\
         URI: {origin}\n\
         Version: 1\n\
         Chain ID: solana:{cluster}\n\
         Nonce: {nonce}\n\
         Issued At: {issued}\n\
         Expiration Time: {expires}",
        issued = issued_at.to_rfc3339_opts(SecondsFormat::Secs, true),
        expires = expires_at.to_rfc3339_opts(SecondsFormat::Secs, true),
    )
}

/// Verifies an ed25519 signature over the exact UTF-8 bytes of `message`.
pub fn verify_signature(
    address: &str,
    message: &str,
    signature_base58: &str,
) -> Result<(), AuthError> {
    let pubkey = parse_pubkey(address).map_err(|_| AuthError::InvalidAddress)?;
    let verifying_key =
        VerifyingKey::from_bytes(&pubkey.to_bytes()).map_err(|_| AuthError::InvalidAddress)?;

    let decoded = bs58::decode(signature_base58)
        .into_vec()
        .map_err(|_| AuthError::InvalidSignatureEncoding)?;
    let signature_bytes: [u8; 64] = decoded
        .try_into()
        .map_err(|_| AuthError::InvalidSignatureLength)?;
    let signature = Signature::from_bytes(&signature_bytes);

    verifying_key
        .verify_strict(message.as_bytes(), &signature)
        .map_err(|_| AuthError::VerificationFailed)
}

/// 16 random bytes, base58 encoded. Single-use and stored only for the
/// challenge lifetime.
pub fn new_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bs58::encode(bytes).into_string()
}

/// Hex SHA-256, used to store session tokens.
pub fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

/// Constant-time comparison for CSRF tokens and other secrets.
pub fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn address(key: &SigningKey) -> String {
        bs58::encode(key.verifying_key().to_bytes()).into_string()
    }

    fn issued() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-28T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn message_matches_the_siws_layout() {
        let message = build_message(
            "app.openwork.xyz",
            "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM",
            STATEMENT,
            "https://app.openwork.xyz",
            "devnet",
            "nonce123",
            issued(),
            issued() + chrono::Duration::minutes(5),
        );
        assert_eq!(
            message,
            "app.openwork.xyz wants you to sign in with your Solana account:\n\
             9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM\n\
             \n\
             Sign in to OpenWork. This request will not trigger a blockchain transaction or cost any fees.\n\
             \n\
             URI: https://app.openwork.xyz\n\
             Version: 1\n\
             Chain ID: solana:devnet\n\
             Nonce: nonce123\n\
             Issued At: 2026-10-28T10:00:00Z\n\
             Expiration Time: 2026-10-28T10:05:00Z"
        );
    }

    #[test]
    fn verifies_a_valid_signature() {
        let key = key();
        let message = "openwork test message";
        let signature = key.sign(message.as_bytes());
        let encoded = bs58::encode(signature.to_bytes()).into_string();
        assert!(verify_signature(&address(&key), message, &encoded).is_ok());
    }

    #[test]
    fn rejects_a_signature_over_different_bytes() {
        let key = key();
        let signature = key.sign(b"message A");
        let encoded = bs58::encode(signature.to_bytes()).into_string();
        assert_eq!(
            verify_signature(&address(&key), "message B", &encoded),
            Err(AuthError::VerificationFailed)
        );
    }

    #[test]
    fn rejects_signatures_from_another_wallet() {
        let signer = key();
        let other = SigningKey::from_bytes(&[9u8; 32]);
        let signature = signer.sign(b"hello");
        let encoded = bs58::encode(signature.to_bytes()).into_string();
        assert_eq!(
            verify_signature(&address(&other), "hello", &encoded),
            Err(AuthError::VerificationFailed)
        );
    }

    #[test]
    fn rejects_malformed_signatures() {
        let key = key();
        assert_eq!(
            verify_signature(&address(&key), "hello", "not base58!!"),
            Err(AuthError::InvalidSignatureEncoding)
        );
        assert_eq!(
            verify_signature(
                &address(&key),
                "hello",
                &bs58::encode([1u8; 32]).into_string()
            ),
            Err(AuthError::InvalidSignatureLength)
        );
        assert_eq!(
            verify_signature("not-a-key", "hello", &bs58::encode([1u8; 64]).into_string()),
            Err(AuthError::InvalidAddress)
        );
    }

    #[test]
    fn nonces_are_unique_and_reasonably_sized() {
        let a = new_nonce();
        let b = new_nonce();
        assert_ne!(a, b);
        assert!(a.len() >= 20);
    }

    #[test]
    fn hashes_and_comparisons_are_stable() {
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(constant_time_eq("token", "token"));
        assert!(!constant_time_eq("token", "tokem"));
        assert!(!constant_time_eq("token", "token "));
    }
}
