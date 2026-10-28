use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::DomainError;
use crate::limits::{MAX_DELIVERABLE_NOTE_LEN, MAX_DELIVERABLE_URI_LEN};
use crate::terms::{hex_lower, push_js_string};

/// Off-chain deliverable metadata attached to a milestone submission.
///
/// Only the hash is ever written on chain (`submit_milestone(deliverable_hash)`),
/// so this document is the canonical preimage of that hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deliverable {
    pub version: u8,
    pub milestone_index: u16,
    pub uri: String,
    pub note: String,
}

impl Deliverable {
    pub const VERSION: u8 = 1;

    pub fn new(milestone_index: u16, uri: impl Into<String>, note: impl Into<String>) -> Self {
        Deliverable {
            version: Self::VERSION,
            milestone_index,
            uri: uri.into(),
            note: note.into(),
        }
    }

    /// Bounded, plain text only. Deliverables are metadata, never markup: the
    /// API refuses HTML so a freelancer cannot smuggle script into a client's
    /// dashboard through this field.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.version != Self::VERSION {
            return Err(DomainError::UnsupportedTermsVersion(self.version));
        }
        let uri = self.uri.trim();
        if uri.is_empty() {
            return Err(DomainError::EmptyField { field: "uri" });
        }
        if uri.len() > MAX_DELIVERABLE_URI_LEN {
            return Err(DomainError::FieldTooLong {
                field: "uri",
                max: MAX_DELIVERABLE_URI_LEN,
            });
        }
        if self.note.len() > MAX_DELIVERABLE_NOTE_LEN {
            return Err(DomainError::FieldTooLong {
                field: "note",
                max: MAX_DELIVERABLE_NOTE_LEN,
            });
        }
        if has_markup_or_control(&self.uri) || has_markup_or_control(&self.note) {
            return Err(DomainError::DisallowedCharacters {
                field: "deliverable",
            });
        }
        if !has_allowed_scheme(uri) {
            return Err(DomainError::DisallowedCharacters { field: "uri" });
        }
        Ok(())
    }

    pub fn canonical_json(&self) -> String {
        let mut out = String::with_capacity(128 + self.note.len());
        out.push_str("{\"version\":");
        out.push_str(&self.version.to_string());
        out.push_str(",\"milestoneIndex\":");
        out.push_str(&self.milestone_index.to_string());
        out.push_str(",\"uri\":");
        push_js_string(&mut out, &self.uri);
        out.push_str(",\"note\":");
        push_js_string(&mut out, &self.note);
        out.push('}');
        out
    }

    pub fn hash_bytes(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json().as_bytes());
        hasher.finalize().into()
    }

    pub fn hash_hex(&self) -> String {
        hex_lower(&self.hash_bytes())
    }
}

fn has_markup_or_control(value: &str) -> bool {
    value
        .chars()
        .any(|c| c == '<' || c == '>' || (c.is_control() && c != '\n' && c != '\t'))
}

fn has_allowed_scheme(uri: &str) -> bool {
    ["https://", "http://", "ipfs://", "ar://"]
        .iter()
        .any(|prefix| uri.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_form_is_stable() {
        let deliverable = Deliverable::new(0, "https://example.com/work", "done");
        assert_eq!(
            deliverable.canonical_json(),
            r#"{"version":1,"milestoneIndex":0,"uri":"https://example.com/work","note":"done"}"#
        );
        assert_eq!(deliverable.hash_hex().len(), 64);
        assert_eq!(deliverable.hash_hex(), deliverable.hash_hex());
    }

    #[test]
    fn hash_changes_with_content() {
        let a = Deliverable::new(0, "https://example.com/a", "");
        let b = Deliverable::new(0, "https://example.com/b", "");
        assert_ne!(a.hash_hex(), b.hash_hex());
        let c = Deliverable::new(1, "https://example.com/a", "");
        assert_ne!(a.hash_hex(), c.hash_hex());
    }

    #[test]
    fn rejects_html_and_control_characters() {
        let with_html = Deliverable::new(0, "https://example.com", "<script>alert(1)</script>");
        assert_eq!(
            with_html.validate(),
            Err(DomainError::DisallowedCharacters {
                field: "deliverable"
            })
        );

        let with_control = Deliverable::new(0, "https://example.com", "a\u{0}b");
        assert!(with_control.validate().is_err());
    }

    #[test]
    fn rejects_unknown_schemes_and_empty_uris() {
        assert!(Deliverable::new(0, "javascript:alert(1)", "")
            .validate()
            .is_err());
        assert!(Deliverable::new(0, "   ", "").validate().is_err());
        assert!(Deliverable::new(0, "ipfs://bafy", "").validate().is_ok());
    }

    #[test]
    fn enforces_length_bounds() {
        let long_uri = format!(
            "https://example.com/{}",
            "a".repeat(MAX_DELIVERABLE_URI_LEN)
        );
        assert!(Deliverable::new(0, long_uri, "").validate().is_err());

        let note = "a".repeat(MAX_DELIVERABLE_NOTE_LEN + 1);
        assert!(Deliverable::new(0, "https://example.com", note)
            .validate()
            .is_err());
    }
}
