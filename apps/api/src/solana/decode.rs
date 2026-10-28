//! Exact Borsh decoding of the escrow program's Anchor accounts.
//!
//! The layouts here mirror `programs/openwork-escrow/src/state.rs` byte for
//! byte. Sizes are asserted in tests, so an accidental struct change in the
//! program fails this crate's test suite instead of silently corrupting the
//! reconciled state.

use sha2::{Digest, Sha256};

use openwork_domain::{AgreementState, MilestoneState};

/// `Agreement::LEN` from the program.
pub const AGREEMENT_LEN: usize = 185;
/// `Milestone::LEN` from the program.
pub const MILESTONE_LEN: usize = 108;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DecodeError {
    #[error("account data has {actual} bytes, expected {expected}")]
    WrongLength { expected: usize, actual: usize },
    #[error("unexpected account discriminator")]
    Discriminator,
    #[error("unknown agreement state {0}")]
    UnknownAgreementState(u8),
    #[error("unknown milestone state {0}")]
    UnknownMilestoneState(u8),
    #[error("account data is truncated")]
    Truncated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgreementAccount {
    pub schema_version: u8,
    pub bump: u8,
    pub state: AgreementState,
    pub client: [u8; 32],
    pub freelancer: [u8; 32],
    pub mint: [u8; 32],
    pub agreement_nonce: u64,
    pub terms_hash: [u8; 32],
    pub milestone_count: u16,
    pub settled_count: u16,
    pub refunded_count: u16,
    pub total_amount: u64,
    pub released_amount: u64,
    pub created_at: i64,
    pub accepted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneAccount {
    pub index: u16,
    pub bump: u8,
    pub state: MilestoneState,
    pub agreement: [u8; 32],
    pub amount: u64,
    pub deliverable_hash: [u8; 32],
    pub funded_at: i64,
    pub submitted_at: i64,
    pub settled_at: i64,
}

pub fn anchor_discriminator(preimage: &str) -> [u8; 8] {
    let digest = Sha256::digest(preimage.as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

pub fn agreement_discriminator() -> [u8; 8] {
    anchor_discriminator("account:Agreement")
}

pub fn milestone_discriminator() -> [u8; 8] {
    anchor_discriminator("account:Milestone")
}

pub fn decode_agreement(data: &[u8]) -> Result<AgreementAccount, DecodeError> {
    if data.len() != AGREEMENT_LEN {
        return Err(DecodeError::WrongLength {
            expected: AGREEMENT_LEN,
            actual: data.len(),
        });
    }
    let mut reader = Reader::new(data);
    if reader.bytes(8)? != agreement_discriminator() {
        return Err(DecodeError::Discriminator);
    }

    let schema_version = reader.u8()?;
    let bump = reader.u8()?;
    let state_raw = reader.u8()?;
    let client = reader.array32()?;
    let freelancer = reader.array32()?;
    let mint = reader.array32()?;
    let agreement_nonce = reader.u64()?;
    let terms_hash = reader.array32()?;
    let milestone_count = reader.u16()?;
    let settled_count = reader.u16()?;
    let refunded_count = reader.u16()?;
    let total_amount = reader.u64()?;
    let released_amount = reader.u64()?;
    let created_at = reader.i64()?;
    let accepted_at = reader.i64()?;
    reader.finish()?;

    let state = AgreementState::from_u8(state_raw)
        .map_err(|_| DecodeError::UnknownAgreementState(state_raw))?;

    Ok(AgreementAccount {
        schema_version,
        bump,
        state,
        client,
        freelancer,
        mint,
        agreement_nonce,
        terms_hash,
        milestone_count,
        settled_count,
        refunded_count,
        total_amount,
        released_amount,
        created_at,
        accepted_at,
    })
}

pub fn decode_milestone(data: &[u8]) -> Result<MilestoneAccount, DecodeError> {
    if data.len() != MILESTONE_LEN {
        return Err(DecodeError::WrongLength {
            expected: MILESTONE_LEN,
            actual: data.len(),
        });
    }
    let mut reader = Reader::new(data);
    if reader.bytes(8)? != milestone_discriminator() {
        return Err(DecodeError::Discriminator);
    }

    let index = reader.u16()?;
    let bump = reader.u8()?;
    let state_raw = reader.u8()?;
    let agreement = reader.array32()?;
    let amount = reader.u64()?;
    let deliverable_hash = reader.array32()?;
    let funded_at = reader.i64()?;
    let submitted_at = reader.i64()?;
    let settled_at = reader.i64()?;
    reader.finish()?;

    let state = MilestoneState::from_u8(state_raw)
        .map_err(|_| DecodeError::UnknownMilestoneState(state_raw))?;

    Ok(MilestoneAccount {
        index,
        bump,
        state,
        agreement,
        amount,
        deliverable_hash,
        funded_at,
        submitted_at,
        settled_at,
    })
}

struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, offset: 0 }
    }

    fn bytes(&mut self, len: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.offset.checked_add(len).ok_or(DecodeError::Truncated)?;
        if end > self.data.len() {
            return Err(DecodeError::Truncated);
        }
        let slice = &self.data[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        let bytes: [u8; 2] = self.bytes(2)?.try_into().expect("length checked");
        Ok(u16::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, DecodeError> {
        let bytes: [u8; 8] = self.bytes(8)?.try_into().expect("length checked");
        Ok(u64::from_le_bytes(bytes))
    }

    fn i64(&mut self) -> Result<i64, DecodeError> {
        let bytes: [u8; 8] = self.bytes(8)?.try_into().expect("length checked");
        Ok(i64::from_le_bytes(bytes))
    }

    fn array32(&mut self) -> Result<[u8; 32], DecodeError> {
        Ok(self.bytes(32)?.try_into().expect("length checked"))
    }

    fn finish(&self) -> Result<(), DecodeError> {
        if self.offset == self.data.len() {
            Ok(())
        } else {
            Err(DecodeError::Truncated)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u64(out: &mut Vec<u8>, value: u64) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_i64(out: &mut Vec<u8>, value: i64) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn agreement_bytes() -> Vec<u8> {
        let mut out = agreement_discriminator().to_vec();
        out.push(1); // schema_version
        out.push(254); // bump
        out.push(1); // Active
        out.extend_from_slice(&[2u8; 32]); // client
        out.extend_from_slice(&[3u8; 32]); // freelancer
        out.extend_from_slice(&[4u8; 32]); // mint
        push_u64(&mut out, 42); // nonce
        out.extend_from_slice(&[5u8; 32]); // terms hash
        out.extend_from_slice(&2u16.to_le_bytes()); // milestone_count
        out.extend_from_slice(&1u16.to_le_bytes()); // settled_count
        out.extend_from_slice(&0u16.to_le_bytes()); // refunded_count
        push_u64(&mut out, 3_000_000);
        push_u64(&mut out, 1_000_000);
        push_i64(&mut out, 1_700_000_000);
        push_i64(&mut out, 1_700_000_100);
        out
    }

    fn milestone_bytes() -> Vec<u8> {
        let mut out = milestone_discriminator().to_vec();
        out.extend_from_slice(&3u16.to_le_bytes()); // index
        out.push(251); // bump
        out.push(2); // Submitted
        out.extend_from_slice(&[7u8; 32]); // agreement
        push_u64(&mut out, 1_500_000);
        out.extend_from_slice(&[9u8; 32]); // deliverable hash
        push_i64(&mut out, 1_700_000_000);
        push_i64(&mut out, 1_700_000_500);
        push_i64(&mut out, 0);
        out
    }

    #[test]
    fn discriminators_match_the_anchor_scheme() {
        // sha256("account:Agreement")[..8] and sha256("account:Milestone")[..8].
        assert_eq!(hex::encode(agreement_discriminator()), "53d4056ee1f9c554");
        assert_eq!(hex::encode(milestone_discriminator()), "26d2efb155b80a2c");
        assert_eq!(agreement_bytes().len(), AGREEMENT_LEN);
        assert_eq!(milestone_bytes().len(), MILESTONE_LEN);
    }

    #[test]
    fn decodes_an_agreement_exactly() {
        let decoded = decode_agreement(&agreement_bytes()).unwrap();
        assert_eq!(decoded.schema_version, 1);
        assert_eq!(decoded.bump, 254);
        assert_eq!(decoded.state, AgreementState::Active);
        assert_eq!(decoded.client, [2u8; 32]);
        assert_eq!(decoded.freelancer, [3u8; 32]);
        assert_eq!(decoded.mint, [4u8; 32]);
        assert_eq!(decoded.agreement_nonce, 42);
        assert_eq!(decoded.terms_hash, [5u8; 32]);
        assert_eq!(decoded.milestone_count, 2);
        assert_eq!(decoded.settled_count, 1);
        assert_eq!(decoded.total_amount, 3_000_000);
        assert_eq!(decoded.released_amount, 1_000_000);
        assert_eq!(decoded.accepted_at, 1_700_000_100);
    }

    #[test]
    fn decodes_a_milestone_exactly() {
        let decoded = decode_milestone(&milestone_bytes()).unwrap();
        assert_eq!(decoded.index, 3);
        assert_eq!(decoded.bump, 251);
        assert_eq!(decoded.state, MilestoneState::Submitted);
        assert_eq!(decoded.agreement, [7u8; 32]);
        assert_eq!(decoded.amount, 1_500_000);
        assert_eq!(decoded.deliverable_hash, [9u8; 32]);
        assert_eq!(decoded.submitted_at, 1_700_000_500);
    }

    #[test]
    fn rejects_wrong_lengths_and_discriminators() {
        assert_eq!(
            decode_agreement(&agreement_bytes()[..184]).unwrap_err(),
            DecodeError::WrongLength {
                expected: AGREEMENT_LEN,
                actual: 184
            }
        );
        let mut bad = agreement_bytes();
        bad[0] ^= 0xff;
        assert_eq!(
            decode_agreement(&bad).unwrap_err(),
            DecodeError::Discriminator
        );
        assert_eq!(
            decode_milestone(&milestone_bytes()[..107]).unwrap_err(),
            DecodeError::WrongLength {
                expected: MILESTONE_LEN,
                actual: 107
            }
        );
    }

    #[test]
    fn rejects_unknown_states() {
        let mut bytes = agreement_bytes();
        bytes[10] = 9;
        assert_eq!(
            decode_agreement(&bytes).unwrap_err(),
            DecodeError::UnknownAgreementState(9)
        );

        let mut bytes = milestone_bytes();
        bytes[11] = 6;
        assert_eq!(
            decode_milestone(&bytes).unwrap_err(),
            DecodeError::UnknownMilestoneState(6)
        );
    }
}
