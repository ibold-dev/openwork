//! Anchor event decoding from transaction logs.
//!
//! Events are the audit trail of the escrow program. They are recorded in
//! `chain_events`, deduplicated by `(chain, signature, event_index)`, and used
//! only to explain state transitions that the account snapshots already prove.
//!
//! Logs coming from other programs in the same transaction are ignored: the
//! decoder tracks the program invocation stack and only accepts program data
//! emitted while the escrow program itself is on top of the stack.

use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EventError {
    #[error("invalid base64 in a program data log")]
    Base64,
    #[error("event payload has {actual} bytes, expected {expected}")]
    Layout { expected: usize, actual: usize },
    #[error("program data log is shorter than an 8 byte discriminator")]
    Truncated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainEvent {
    AgreementCreated {
        agreement: [u8; 32],
        client: [u8; 32],
        freelancer: [u8; 32],
        mint: [u8; 32],
        agreement_nonce: u64,
        terms_hash: [u8; 32],
    },
    MilestoneCreated {
        agreement: [u8; 32],
        index: u16,
        amount: u64,
    },
    AgreementAccepted {
        agreement: [u8; 32],
        freelancer: [u8; 32],
        terms_hash: [u8; 32],
        accepted_at: i64,
    },
    MilestoneFunded {
        agreement: [u8; 32],
        index: u16,
        amount: u64,
        vault_balance: u64,
    },
    MilestoneSubmitted {
        agreement: [u8; 32],
        index: u16,
        deliverable_hash: [u8; 32],
    },
    MilestoneReleased {
        agreement: [u8; 32],
        index: u16,
        amount: u64,
        freelancer: [u8; 32],
    },
    MilestoneRefunded {
        agreement: [u8; 32],
        index: u16,
        amount: u64,
        client: [u8; 32],
    },
    MilestoneDisputed {
        agreement: [u8; 32],
        index: u16,
        raised_by: [u8; 32],
    },
    AgreementCancelled {
        agreement: [u8; 32],
        client: [u8; 32],
    },
}

impl ChainEvent {
    pub fn name(&self) -> &'static str {
        match self {
            ChainEvent::AgreementCreated { .. } => "AgreementCreated",
            ChainEvent::MilestoneCreated { .. } => "MilestoneCreated",
            ChainEvent::AgreementAccepted { .. } => "AgreementAccepted",
            ChainEvent::MilestoneFunded { .. } => "MilestoneFunded",
            ChainEvent::MilestoneSubmitted { .. } => "MilestoneSubmitted",
            ChainEvent::MilestoneReleased { .. } => "MilestoneReleased",
            ChainEvent::MilestoneRefunded { .. } => "MilestoneRefunded",
            ChainEvent::MilestoneDisputed { .. } => "MilestoneDisputed",
            ChainEvent::AgreementCancelled { .. } => "AgreementCancelled",
        }
    }

    pub fn agreement(&self) -> [u8; 32] {
        match self {
            ChainEvent::AgreementCreated { agreement, .. }
            | ChainEvent::MilestoneCreated { agreement, .. }
            | ChainEvent::AgreementAccepted { agreement, .. }
            | ChainEvent::MilestoneFunded { agreement, .. }
            | ChainEvent::MilestoneSubmitted { agreement, .. }
            | ChainEvent::MilestoneReleased { agreement, .. }
            | ChainEvent::MilestoneRefunded { agreement, .. }
            | ChainEvent::MilestoneDisputed { agreement, .. }
            | ChainEvent::AgreementCancelled { agreement, .. } => *agreement,
        }
    }

    pub fn milestone_index(&self) -> Option<u16> {
        match self {
            ChainEvent::MilestoneCreated { index, .. }
            | ChainEvent::MilestoneFunded { index, .. }
            | ChainEvent::MilestoneSubmitted { index, .. }
            | ChainEvent::MilestoneReleased { index, .. }
            | ChainEvent::MilestoneRefunded { index, .. }
            | ChainEvent::MilestoneDisputed { index, .. } => Some(*index),
            ChainEvent::AgreementCreated { .. }
            | ChainEvent::AgreementAccepted { .. }
            | ChainEvent::AgreementCancelled { .. } => None,
        }
    }

    /// JSON payload stored in `chain_events.payload`.
    ///
    /// Amounts are decimal strings and keys are camelCase so the payload can be
    /// handed to JavaScript clients without precision loss.
    pub fn payload(&self) -> Value {
        match self {
            ChainEvent::AgreementCreated {
                agreement,
                client,
                freelancer,
                mint,
                agreement_nonce,
                terms_hash,
            } => json!({
                "agreement": bs58::encode(agreement).into_string(),
                "client": bs58::encode(client).into_string(),
                "freelancer": bs58::encode(freelancer).into_string(),
                "mint": bs58::encode(mint).into_string(),
                "agreementNonce": agreement_nonce.to_string(),
                "termsHash": hex::encode(terms_hash),
            }),
            ChainEvent::MilestoneCreated {
                agreement,
                index,
                amount,
            } => json!({
                "agreement": bs58::encode(agreement).into_string(),
                "index": index,
                "amount": amount.to_string(),
            }),
            ChainEvent::AgreementAccepted {
                agreement,
                freelancer,
                terms_hash,
                accepted_at,
            } => json!({
                "agreement": bs58::encode(agreement).into_string(),
                "freelancer": bs58::encode(freelancer).into_string(),
                "termsHash": hex::encode(terms_hash),
                "acceptedAt": accepted_at.to_string(),
            }),
            ChainEvent::MilestoneFunded {
                agreement,
                index,
                amount,
                vault_balance,
            } => json!({
                "agreement": bs58::encode(agreement).into_string(),
                "index": index,
                "amount": amount.to_string(),
                "vaultBalance": vault_balance.to_string(),
            }),
            ChainEvent::MilestoneSubmitted {
                agreement,
                index,
                deliverable_hash,
            } => json!({
                "agreement": bs58::encode(agreement).into_string(),
                "index": index,
                "deliverableHash": hex::encode(deliverable_hash),
            }),
            ChainEvent::MilestoneReleased {
                agreement,
                index,
                amount,
                freelancer,
            } => json!({
                "agreement": bs58::encode(agreement).into_string(),
                "index": index,
                "amount": amount.to_string(),
                "freelancer": bs58::encode(freelancer).into_string(),
            }),
            ChainEvent::MilestoneRefunded {
                agreement,
                index,
                amount,
                client,
            } => json!({
                "agreement": bs58::encode(agreement).into_string(),
                "index": index,
                "amount": amount.to_string(),
                "client": bs58::encode(client).into_string(),
            }),
            ChainEvent::MilestoneDisputed {
                agreement,
                index,
                raised_by,
            } => json!({
                "agreement": bs58::encode(agreement).into_string(),
                "index": index,
                "raisedBy": bs58::encode(raised_by).into_string(),
            }),
            ChainEvent::AgreementCancelled { agreement, client } => json!({
                "agreement": bs58::encode(agreement).into_string(),
                "client": bs58::encode(client).into_string(),
            }),
        }
    }
}

/// Parses `Program data:` logs emitted by `program_id`, in transaction order.
pub fn parse_logs(logs: &[String], program_id: &[u8; 32]) -> Result<Vec<ChainEvent>, EventError> {
    let mut stack: Vec<[u8; 32]> = Vec::new();
    let mut events = Vec::new();

    for line in logs {
        let line = line.trim_end();

        // `Program data: …` also starts with `Program `, so it must be handled
        // before the invoke/success parser or every event would be skipped.
        if let Some(encoded) = line.strip_prefix("Program data: ") {
            if stack.last() != Some(program_id) {
                continue;
            }
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .map_err(|_| EventError::Base64)?;
            if let Some(event) = decode_event(&bytes)? {
                events.push(event);
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("Program ") {
            if let Some((address, action)) = rest.split_once(' ') {
                if action.starts_with("invoke") {
                    if let Ok(bytes) = bs58::decode(address).into_vec() {
                        if let Ok(bytes) = <[u8; 32]>::try_from(bytes) {
                            stack.push(bytes);
                        }
                    }
                } else if action.starts_with("success") || action.starts_with("failed") {
                    stack.pop();
                }
                continue;
            }
        }
    }

    Ok(events)
}

fn decode_event(data: &[u8]) -> Result<Option<ChainEvent>, EventError> {
    if data.len() < 8 {
        return Err(EventError::Truncated);
    }
    let (discriminator, body) = data.split_at(8);

    macro_rules! check {
        ($expected:expr) => {
            if body.len() != $expected {
                return Err(EventError::Layout {
                    expected: $expected,
                    actual: body.len(),
                });
            }
        };
    }

    if discriminator == event_discriminator("AgreementCreated") {
        check!(168);
        let mut reader = Reader::new(body);
        let agreement = reader.array32();
        let client = reader.array32();
        let freelancer = reader.array32();
        let mint = reader.array32();
        let agreement_nonce = reader.u64();
        let terms_hash = reader.array32();
        return Ok(Some(ChainEvent::AgreementCreated {
            agreement,
            client,
            freelancer,
            mint,
            agreement_nonce,
            terms_hash,
        }));
    }
    if discriminator == event_discriminator("MilestoneCreated") {
        check!(42);
        let mut reader = Reader::new(body);
        return Ok(Some(ChainEvent::MilestoneCreated {
            agreement: reader.array32(),
            index: reader.u16(),
            amount: reader.u64(),
        }));
    }
    if discriminator == event_discriminator("AgreementAccepted") {
        check!(104);
        let mut reader = Reader::new(body);
        return Ok(Some(ChainEvent::AgreementAccepted {
            agreement: reader.array32(),
            freelancer: reader.array32(),
            terms_hash: reader.array32(),
            accepted_at: reader.i64(),
        }));
    }
    if discriminator == event_discriminator("MilestoneFunded") {
        check!(50);
        let mut reader = Reader::new(body);
        return Ok(Some(ChainEvent::MilestoneFunded {
            agreement: reader.array32(),
            index: reader.u16(),
            amount: reader.u64(),
            vault_balance: reader.u64(),
        }));
    }
    if discriminator == event_discriminator("MilestoneSubmitted") {
        check!(66);
        let mut reader = Reader::new(body);
        return Ok(Some(ChainEvent::MilestoneSubmitted {
            agreement: reader.array32(),
            index: reader.u16(),
            deliverable_hash: reader.array32(),
        }));
    }
    if discriminator == event_discriminator("MilestoneReleased") {
        check!(74);
        let mut reader = Reader::new(body);
        return Ok(Some(ChainEvent::MilestoneReleased {
            agreement: reader.array32(),
            index: reader.u16(),
            amount: reader.u64(),
            freelancer: reader.array32(),
        }));
    }
    if discriminator == event_discriminator("MilestoneRefunded") {
        check!(74);
        let mut reader = Reader::new(body);
        return Ok(Some(ChainEvent::MilestoneRefunded {
            agreement: reader.array32(),
            index: reader.u16(),
            amount: reader.u64(),
            client: reader.array32(),
        }));
    }
    if discriminator == event_discriminator("MilestoneDisputed") {
        check!(66);
        let mut reader = Reader::new(body);
        return Ok(Some(ChainEvent::MilestoneDisputed {
            agreement: reader.array32(),
            index: reader.u16(),
            raised_by: reader.array32(),
        }));
    }
    if discriminator == event_discriminator("AgreementCancelled") {
        check!(64);
        let mut reader = Reader::new(body);
        return Ok(Some(ChainEvent::AgreementCancelled {
            agreement: reader.array32(),
            client: reader.array32(),
        }));
    }

    // Forward-compatible: an event added by a newer program version is recorded
    // as unknown by the caller only if it decodes; here we simply skip it.
    Ok(None)
}

pub fn event_discriminator(name: &str) -> [u8; 8] {
    let digest = Sha256::digest(format!("event:{name}").as_bytes());
    let mut out = [0u8; 8];
    out.copy_from_slice(&digest[..8]);
    out
}

/// A reader over an already length-checked event body.
struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, offset: 0 }
    }

    fn bytes(&mut self, len: usize) -> &'a [u8] {
        let slice = &self.data[self.offset..self.offset + len];
        self.offset += len;
        slice
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.bytes(2).try_into().expect("length checked"))
    }

    fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.bytes(8).try_into().expect("length checked"))
    }

    fn i64(&mut self) -> i64 {
        i64::from_le_bytes(self.bytes(8).try_into().expect("length checked"))
    }

    fn array32(&mut self) -> [u8; 32] {
        self.bytes(32).try_into().expect("length checked")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROGRAM: [u8; 32] = [9u8; 32];

    fn program_key() -> String {
        bs58::encode(PROGRAM).into_string()
    }

    fn data_line(name: &str, body: &[u8]) -> String {
        let mut data = event_discriminator(name).to_vec();
        data.extend_from_slice(body);
        format!(
            "Program data: {}",
            base64::engine::general_purpose::STANDARD.encode(data)
        )
    }

    #[test]
    fn discriminators_match_anchor() {
        assert_eq!(
            hex::encode(event_discriminator("MilestoneFunded")),
            "85df55eb3824eef0"
        );
    }

    #[test]
    fn parses_events_emitted_by_our_program() {
        let mut body = Vec::new();
        body.extend_from_slice(&[1u8; 32]); // agreement
        body.extend_from_slice(&3u16.to_le_bytes());
        body.extend_from_slice(&1_000_000u64.to_le_bytes());
        body.extend_from_slice(&5_000_000u64.to_le_bytes());

        let logs = vec![
            format!("Program {} invoke [1]", program_key()),
            "Program log: Instruction: FundMilestone".to_string(),
            data_line("MilestoneFunded", &body),
            format!("Program {} success", program_key()),
        ];

        let events = parse_logs(&logs, &PROGRAM).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0],
            ChainEvent::MilestoneFunded {
                agreement: [1u8; 32],
                index: 3,
                amount: 1_000_000,
                vault_balance: 5_000_000,
            }
        );
        assert_eq!(events[0].name(), "MilestoneFunded");
        assert_eq!(events[0].milestone_index(), Some(3));
        assert_eq!(events[0].payload()["amount"], "1000000");
    }

    #[test]
    fn ignores_events_from_other_programs_and_other_invocations() {
        let other = [7u8; 32];
        let mut body = Vec::new();
        body.extend_from_slice(&[2u8; 32]);
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(&1u64.to_le_bytes());

        let logs = vec![
            format!("Program {} invoke [1]", bs58::encode(other).into_string()),
            data_line("MilestoneCreated", &body),
            format!("Program {} success", bs58::encode(other).into_string()),
            format!("Program {} invoke [1]", program_key()),
            format!("Program {} invoke [2]", program_key()),
            data_line("MilestoneCreated", &body),
            format!("Program {} success", program_key()),
            format!("Program {} success", program_key()),
        ];

        // The nested invocation is on top of the stack, so this event is
        // attributed to our program (CPI into itself is unusual, but the stack
        // semantics are what matter).
        let events = parse_logs(&logs, &PROGRAM).unwrap();
        assert_eq!(events.len(), 1);
        assert!(parse_logs(&logs[..3], &PROGRAM).unwrap().is_empty());
    }

    #[test]
    fn rejects_malformed_known_events() {
        let mut short_body = vec![0u8; 41];
        short_body[0] = 1;
        let logs = vec![
            format!("Program {} invoke [1]", program_key()),
            data_line("MilestoneCreated", &short_body),
        ];
        assert_eq!(
            parse_logs(&logs, &PROGRAM).unwrap_err(),
            EventError::Layout {
                expected: 42,
                actual: 41
            }
        );

        let logs = vec![
            format!("Program {} invoke [1]", program_key()),
            "Program data: !!!not-base64!!!".to_string(),
        ];
        assert_eq!(parse_logs(&logs, &PROGRAM).unwrap_err(), EventError::Base64);
    }

    #[test]
    fn unknown_event_discriminators_are_skipped() {
        let mut data = vec![0u8; 8];
        data[0] = 42;
        let line = format!(
            "Program data: {}",
            base64::engine::general_purpose::STANDARD.encode(data)
        );
        let logs = vec![format!("Program {} invoke [1]", program_key()), line];
        assert!(parse_logs(&logs, &PROGRAM).unwrap().is_empty());
    }

    #[test]
    fn encodes_created_and_cancelled_payloads() {
        let created = ChainEvent::AgreementCreated {
            agreement: [1u8; 32],
            client: [2u8; 32],
            freelancer: [3u8; 32],
            mint: [4u8; 32],
            agreement_nonce: 7,
            terms_hash: [5u8; 32],
        };
        let payload = created.payload();
        assert_eq!(payload["agreementNonce"], "7");
        assert_eq!(payload["termsHash"].as_str().unwrap().len(), 64);

        let cancelled = ChainEvent::AgreementCancelled {
            agreement: [1u8; 32],
            client: [2u8; 32],
        };
        assert_eq!(cancelled.agreement(), [1u8; 32]);
        assert_eq!(cancelled.milestone_index(), None);
    }
}
