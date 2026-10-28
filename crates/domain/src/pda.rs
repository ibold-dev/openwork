//! PDA derivation, identical to the on-chain program's seeds.

use solana_pubkey::Pubkey;

use crate::error::DomainError;

/// Agreement PDA: `[b"agreement", client, agreement_nonce_le]`.
pub const AGREEMENT_SEED: &[u8] = b"agreement";

/// Milestone PDA: `[b"milestone", agreement, milestone_index_le]`.
pub const MILESTONE_SEED: &[u8] = b"milestone";

/// Vault authority PDA: `[b"vault-authority", agreement]`.
pub const VAULT_AUTHORITY_SEED: &[u8] = b"vault-authority";

/// Parses a base58 Solana address.
pub fn parse_pubkey(value: &str) -> Result<Pubkey, DomainError> {
    value
        .parse::<Pubkey>()
        .map_err(|_| DomainError::InvalidPublicKey)
}

pub fn is_valid_pubkey(value: &str) -> bool {
    parse_pubkey(value).is_ok()
}

/// Derives the agreement account address for a client and nonce.
pub fn agreement_pda(program_id: &Pubkey, client: &Pubkey, agreement_nonce: u64) -> (Pubkey, u8) {
    Pubkey::derive_program_address(
        &[
            AGREEMENT_SEED,
            client.as_ref(),
            &agreement_nonce.to_le_bytes(),
        ],
        program_id,
    )
    .expect("agreement seeds are short enough to derive a PDA")
}

/// Derives a milestone account address for an agreement and index.
pub fn milestone_pda(program_id: &Pubkey, agreement: &Pubkey, index: u16) -> (Pubkey, u8) {
    Pubkey::derive_program_address(
        &[MILESTONE_SEED, agreement.as_ref(), &index.to_le_bytes()],
        program_id,
    )
    .expect("milestone seeds are short enough to derive a PDA")
}

/// Derives the vault authority (a data-less signer PDA).
pub fn vault_authority_pda(program_id: &Pubkey, agreement: &Pubkey) -> (Pubkey, u8) {
    Pubkey::derive_program_address(&[VAULT_AUTHORITY_SEED, agreement.as_ref()], program_id)
        .expect("vault seeds are short enough to derive a PDA")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: &str = "GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR";
    const CLIENT: &str = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";

    #[test]
    fn agreement_pda_is_deterministic_and_verified() {
        let program_id = parse_pubkey(PROGRAM).unwrap();
        let client = parse_pubkey(CLIENT).unwrap();
        let (pda, bump) = agreement_pda(&program_id, &client, 7);
        assert_eq!(pda, agreement_pda(&program_id, &client, 7).0);
        // Recompute the address from the seeds plus the returned bump. This is
        // the same hash the program performs; the bump proves the address is
        // off-curve.
        assert_eq!(
            Pubkey::derive_address(
                &[
                    AGREEMENT_SEED,
                    client.as_ref(),
                    &7u64.to_le_bytes(),
                    &[bump],
                ],
                None,
                &program_id
            ),
            pda
        );
    }

    #[test]
    fn nonce_and_index_change_the_address() {
        let program_id = parse_pubkey(PROGRAM).unwrap();
        let client = parse_pubkey(CLIENT).unwrap();
        let (a, _) = agreement_pda(&program_id, &client, 1);
        let (b, _) = agreement_pda(&program_id, &client, 2);
        assert_ne!(a, b);

        let (m0, _) = milestone_pda(&program_id, &a, 0);
        let (m1, _) = milestone_pda(&program_id, &a, 1);
        assert_ne!(m0, m1);
        assert_ne!(m0, a);
    }

    #[test]
    fn rejects_invalid_addresses() {
        assert_eq!(
            parse_pubkey("not a key"),
            Err(DomainError::InvalidPublicKey)
        );
        assert!(!is_valid_pubkey(""));
        assert!(is_valid_pubkey(CLIENT));
    }
}
