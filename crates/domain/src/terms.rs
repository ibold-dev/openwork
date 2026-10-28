use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use solana_pubkey::Pubkey;

use crate::amount::Amount;
use crate::error::DomainError;
use crate::limits::{
    MAX_DESCRIPTION_LEN, MAX_MILESTONES, MAX_MILESTONE_DESCRIPTION_LEN, MAX_MILESTONE_TITLE_LEN,
    MAX_TITLE_LEN, TERMS_VERSION,
};

/// One milestone inside the canonical terms document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TermsMilestone {
    pub index: u16,
    pub title: String,
    pub description: String,
    pub amount: Amount,
}

/// The off-chain agreement terms that both parties commit to.
///
/// The hash of [`Terms::canonical_json`] is stored on chain in
/// `Agreement.terms_hash`, so the serialization below is a wire format: it must
/// match the SDK byte for byte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Terms {
    pub version: u8,
    pub title: String,
    pub description: String,
    pub client: String,
    pub freelancer: String,
    pub mint: String,
    pub milestones: Vec<TermsMilestone>,
}

impl Terms {
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        client: impl Into<String>,
        freelancer: impl Into<String>,
        mint: impl Into<String>,
        milestones: Vec<TermsMilestone>,
    ) -> Self {
        Terms {
            version: TERMS_VERSION,
            title: title.into(),
            description: description.into(),
            client: client.into(),
            freelancer: freelancer.into(),
            mint: mint.into(),
            milestones,
        }
    }

    /// Structural validation shared by every caller.
    ///
    /// An empty milestone list is allowed because a draft may be created before
    /// its milestones; [`Terms::validate_for_acceptance`] is the stricter gate.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.version != TERMS_VERSION {
            return Err(DomainError::UnsupportedTermsVersion(self.version));
        }
        validate_text("title", &self.title, MAX_TITLE_LEN, false)?;
        validate_text("description", &self.description, MAX_DESCRIPTION_LEN, true)?;
        parse_address("client", &self.client)?;
        parse_address("freelancer", &self.freelancer)?;
        parse_address("mint", &self.mint)?;
        if self.client == self.freelancer {
            return Err(DomainError::DisallowedCharacters {
                field: "freelancer",
            });
        }
        if self.milestones.len() > usize::from(MAX_MILESTONES) {
            return Err(DomainError::TooManyMilestones(MAX_MILESTONES));
        }
        for (position, milestone) in self.milestones.iter().enumerate() {
            let expected =
                u16::try_from(position).map_err(|_| DomainError::InvalidMilestoneIndex(0))?;
            if milestone.index != expected {
                return Err(DomainError::NonContiguousMilestone {
                    expected,
                    actual: milestone.index,
                });
            }
            validate_text(
                "milestone.title",
                &milestone.title,
                MAX_MILESTONE_TITLE_LEN,
                false,
            )?;
            validate_text(
                "milestone.description",
                &milestone.description,
                MAX_MILESTONE_DESCRIPTION_LEN,
                true,
            )?;
        }
        Ok(())
    }

    /// Acceptance requires something to fund.
    pub fn validate_for_acceptance(&self) -> Result<(), DomainError> {
        self.validate()?;
        if self.milestones.is_empty() {
            return Err(DomainError::NoMilestones);
        }
        Ok(())
    }

    /// Total of every milestone amount, checked.
    pub fn total_amount(&self) -> Result<u64, DomainError> {
        Amount::checked_sum(self.milestones.iter().map(|m| m.amount))
    }

    /// Canonical serialization, byte-identical to the SDK's
    /// `JSON.stringify({version, title, description, client, freelancer, mint,
    /// milestones: [{index, title, description, amount}]})`.
    ///
    /// Keys are always emitted in the order above, amounts are decimal strings,
    /// and strings use JavaScript escaping rules.
    pub fn canonical_json(&self) -> String {
        let mut out = String::with_capacity(256 + self.milestones.len() * 128);
        out.push_str("{\"version\":");
        out.push_str(&self.version.to_string());
        out.push_str(",\"title\":");
        push_js_string(&mut out, &self.title);
        out.push_str(",\"description\":");
        push_js_string(&mut out, &self.description);
        out.push_str(",\"client\":");
        push_js_string(&mut out, &self.client);
        out.push_str(",\"freelancer\":");
        push_js_string(&mut out, &self.freelancer);
        out.push_str(",\"mint\":");
        push_js_string(&mut out, &self.mint);
        out.push_str(",\"milestones\":[");
        for (position, milestone) in self.milestones.iter().enumerate() {
            if position > 0 {
                out.push(',');
            }
            out.push_str("{\"index\":");
            out.push_str(&milestone.index.to_string());
            out.push_str(",\"title\":");
            push_js_string(&mut out, &milestone.title);
            out.push_str(",\"description\":");
            push_js_string(&mut out, &milestone.description);
            out.push_str(",\"amount\":");
            push_js_string(&mut out, &milestone.amount.to_string());
            out.push('}');
        }
        out.push_str("]}");
        out
    }

    pub fn hash_bytes(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json().as_bytes());
        hasher.finalize().into()
    }

    /// 64 lowercase hex characters, as stored and returned by the API.
    pub fn hash_hex(&self) -> String {
        hex_lower(&self.hash_bytes())
    }
}

/// Lowercase hex, no `0x` prefix.
pub fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from_digit(u32::from(byte >> 4), 16).expect("nibble"));
        out.push(char::from_digit(u32::from(byte & 0x0f), 16).expect("nibble"));
    }
    out
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    allow_newlines: bool,
) -> Result<(), DomainError> {
    if field.ends_with("title") && value.trim().is_empty() {
        return Err(DomainError::EmptyField { field });
    }
    if value.len() > max {
        return Err(DomainError::FieldTooLong { field, max });
    }
    let has_control = value.chars().any(|c| {
        if c == '\n' || c == '\t' {
            !allow_newlines
        } else {
            c.is_control()
        }
    });
    if has_control {
        return Err(DomainError::DisallowedCharacters { field });
    }
    Ok(())
}

fn parse_address(_field: &'static str, value: &str) -> Result<Pubkey, DomainError> {
    value
        .parse::<Pubkey>()
        .map_err(|_| DomainError::InvalidPublicKey)
}

/// Appends `value` as a JSON string exactly like `JSON.stringify`.
///
/// Differences from naive escaping matter for the terms hash:
/// - only `"`, `\` and control characters are escaped,
/// - `\b \t \n \f \r` use their short forms,
/// - other control characters use lowercase `\u00xx`,
/// - non-ASCII is emitted as raw UTF-8.
pub fn push_js_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{0c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => {
                out.push_str("\\u");
                out.push(char::from_digit((c as u32) >> 12, 16).expect("nibble"));
                out.push(char::from_digit(((c as u32) >> 8) & 0x0f, 16).expect("nibble"));
                out.push(char::from_digit(((c as u32) >> 4) & 0x0f, 16).expect("nibble"));
                out.push(char::from_digit((c as u32) & 0x0f, 16).expect("nibble"));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENT: &str = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM";
    const FREELANCER: &str = "5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1";
    const MINT: &str = "4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU";

    fn sample() -> Terms {
        Terms::new(
            "Website rebuild",
            "Marketing site + CMS",
            CLIENT,
            FREELANCER,
            MINT,
            vec![
                TermsMilestone {
                    index: 0,
                    title: "Design".into(),
                    description: "Wireframes".into(),
                    amount: Amount::new(1_000_000).unwrap(),
                },
                TermsMilestone {
                    index: 1,
                    title: "Build".into(),
                    description: "Implementation".into(),
                    amount: Amount::new(2_500_000).unwrap(),
                },
            ],
        )
    }

    #[test]
    fn canonical_json_is_compact_and_ordered() {
        let json = sample().canonical_json();
        assert_eq!(
            json,
            concat!(
                r#"{"version":1,"title":"Website rebuild","description":"Marketing site + CMS","#,
                r#""client":"9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM","#,
                r#""freelancer":"5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1","#,
                r#""mint":"4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU","#,
                r#""milestones":[{"index":0,"title":"Design","description":"Wireframes","amount":"1000000"},"#,
                r#"{"index":1,"title":"Build","description":"Implementation","amount":"2500000"}]}"#
            )
        );
    }

    #[test]
    fn canonical_json_matches_javascript_escaping() {
        let mut terms = sample();
        terms.title = "He said \"hi\"\\ then\nleft \u{1}\t✓".to_string();
        let json = terms.canonical_json();
        assert!(json.contains(r#""title":"He said \"hi\"\\ then\nleft \u0001\t✓""#));

        // serde_json must produce the same *value*, which proves escaping does
        // not change the document, only its byte representation.
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["title"], "He said \"hi\"\\ then\nleft \u{1}\t✓");
    }

    #[test]
    fn serde_json_matches_canonical_for_plain_ascii() {
        // `serde_json` on the struct emits fields in declaration order, which is
        // the same order. This guards against accidental reordering of the
        // struct fields (a breaking change to the hash).
        let terms = sample();
        let from_serde = serde_json::to_string(&terms).unwrap();
        assert_eq!(from_serde, terms.canonical_json());
    }

    #[test]
    fn hash_is_stable_lowercase_hex() {
        let terms = sample();
        let hash = terms.hash_hex();
        assert_eq!(hash.len(), 64);
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        // Vector computed independently (Python hashlib) from the exact bytes
        // asserted in `canonical_json_is_compact_and_ordered`.
        assert_eq!(
            hash,
            "a6834cfc2e70951addb23e505d9069ad6ad4bae2a713ea43f6c551038ae1058e"
        );
    }

    #[test]
    fn hash_is_sensitive_to_every_field() {
        let base = sample();
        let mut variants = Vec::new();

        let mut t = base.clone();
        t.title.push('!');
        variants.push(t);

        let mut t = base.clone();
        t.description.push('!');
        variants.push(t);

        let mut t = base.clone();
        t.freelancer = CLIENT.to_string();
        t.client = FREELANCER.to_string();
        variants.push(t);

        let mut t = base.clone();
        t.milestones[0].amount = Amount::new(1).unwrap();
        variants.push(t);

        let mut t = base.clone();
        t.milestones[1].title = "Ship".into();
        variants.push(t);

        for variant in variants {
            assert_ne!(variant.hash_hex(), base.hash_hex());
        }
    }

    #[test]
    fn validation_accepts_a_well_formed_document() {
        assert!(sample().validate().is_ok());
        assert!(sample().validate_for_acceptance().is_ok());
    }

    #[test]
    fn validation_rejects_empty_titles_and_long_fields() {
        let mut terms = sample();
        terms.title = "   ".into();
        assert_eq!(
            terms.validate(),
            Err(DomainError::EmptyField { field: "title" })
        );

        let mut terms = sample();
        terms.description = "x".repeat(MAX_DESCRIPTION_LEN + 1);
        assert_eq!(
            terms.validate(),
            Err(DomainError::FieldTooLong {
                field: "description",
                max: MAX_DESCRIPTION_LEN
            })
        );

        let mut terms = sample();
        terms.title = "bad\nnewline".into();
        assert_eq!(
            terms.validate(),
            Err(DomainError::DisallowedCharacters { field: "title" })
        );
    }

    #[test]
    fn validation_rejects_contiguity_and_limits() {
        let mut terms = sample();
        terms.milestones[0].index = 1;
        assert_eq!(
            terms.validate(),
            Err(DomainError::NonContiguousMilestone {
                expected: 0,
                actual: 1
            })
        );

        let mut terms = sample();
        terms.milestones.clear();
        assert!(terms.validate().is_ok());
        assert_eq!(
            terms.validate_for_acceptance(),
            Err(DomainError::NoMilestones)
        );

        let mut terms = sample();
        terms.milestones = (0..=MAX_MILESTONES)
            .map(|index| TermsMilestone {
                index,
                title: "t".into(),
                description: String::new(),
                amount: Amount::new(1).unwrap(),
            })
            .collect();
        assert_eq!(
            terms.validate(),
            Err(DomainError::TooManyMilestones(MAX_MILESTONES))
        );
    }

    #[test]
    fn validation_rejects_bad_addresses_and_self_dealing() {
        let mut terms = sample();
        terms.client = "not base58!".into();
        assert_eq!(terms.validate(), Err(DomainError::InvalidPublicKey));

        let mut terms = sample();
        terms.freelancer = terms.client.clone();
        assert!(terms.validate().is_err());
    }

    #[test]
    fn totals_are_checked() {
        assert_eq!(sample().total_amount().unwrap(), 3_500_000);

        let mut terms = sample();
        terms.milestones[0].amount = Amount::MAX;
        terms.milestones[1].amount = Amount::new(1).unwrap();
        assert_eq!(terms.total_amount(), Err(DomainError::AmountOverflow));
    }
}
