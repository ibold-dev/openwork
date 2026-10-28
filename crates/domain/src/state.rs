use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// Lifecycle of an agreement. Mirrors `AgreementState` in the program, whose
/// Borsh encoding is the declaration order below (0..=3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgreementState {
    Draft,
    Active,
    Completed,
    Cancelled,
}

impl AgreementState {
    pub const fn as_u8(self) -> u8 {
        match self {
            AgreementState::Draft => 0,
            AgreementState::Active => 1,
            AgreementState::Completed => 2,
            AgreementState::Cancelled => 3,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            AgreementState::Draft => "draft",
            AgreementState::Active => "active",
            AgreementState::Completed => "completed",
            AgreementState::Cancelled => "cancelled",
        }
    }

    pub fn from_u8(value: u8) -> Result<Self, DomainError> {
        match value {
            0 => Ok(AgreementState::Draft),
            1 => Ok(AgreementState::Active),
            2 => Ok(AgreementState::Completed),
            3 => Ok(AgreementState::Cancelled),
            other => Err(DomainError::UnknownState(other)),
        }
    }

    /// Agreement transitions accepted by the program.
    ///
    /// - `Draft -> Active` requires at least one milestone and the exact terms
    ///   hash (freelancer acceptance).
    /// - `Draft -> Cancelled` is the client abandoning a draft that never held
    ///   funds.
    /// - `Active -> Completed` happens when every milestone settled.
    /// - `Completed` and `Cancelled` are terminal.
    pub const fn can_transition_to(self, next: AgreementState) -> bool {
        matches!(
            (self, next),
            (AgreementState::Draft, AgreementState::Active)
                | (AgreementState::Draft, AgreementState::Cancelled)
                | (AgreementState::Active, AgreementState::Completed)
        )
    }

    pub fn ensure_transition(self, next: AgreementState) -> Result<(), DomainError> {
        if self.can_transition_to(next) {
            Ok(())
        } else {
            Err(DomainError::InvalidAgreementTransition {
                from: self.as_str(),
                to: next.as_str(),
            })
        }
    }
}

/// Lifecycle of a single milestone. Mirrors `MilestoneState` (0..=5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MilestoneState {
    Pending,
    Funded,
    Submitted,
    Released,
    Refunded,
    Disputed,
}

impl MilestoneState {
    pub const fn as_u8(self) -> u8 {
        match self {
            MilestoneState::Pending => 0,
            MilestoneState::Funded => 1,
            MilestoneState::Submitted => 2,
            MilestoneState::Released => 3,
            MilestoneState::Refunded => 4,
            MilestoneState::Disputed => 5,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            MilestoneState::Pending => "pending",
            MilestoneState::Funded => "funded",
            MilestoneState::Submitted => "submitted",
            MilestoneState::Released => "released",
            MilestoneState::Refunded => "refunded",
            MilestoneState::Disputed => "disputed",
        }
    }

    pub fn from_u8(value: u8) -> Result<Self, DomainError> {
        match value {
            0 => Ok(MilestoneState::Pending),
            1 => Ok(MilestoneState::Funded),
            2 => Ok(MilestoneState::Submitted),
            3 => Ok(MilestoneState::Released),
            4 => Ok(MilestoneState::Refunded),
            5 => Ok(MilestoneState::Disputed),
            other => Err(DomainError::UnknownState(other)),
        }
    }

    /// Milestone transitions accepted by the program.
    ///
    /// Funds leave the vault only through `Released` (client approval) or
    /// `Refunded` (mutual cancellation). `Disputed` freezes both.
    pub const fn can_transition_to(self, next: MilestoneState) -> bool {
        matches!(
            (self, next),
            (MilestoneState::Pending, MilestoneState::Funded)
                | (MilestoneState::Funded, MilestoneState::Submitted)
                | (MilestoneState::Funded, MilestoneState::Disputed)
                | (MilestoneState::Funded, MilestoneState::Refunded)
                | (MilestoneState::Submitted, MilestoneState::Released)
                | (MilestoneState::Submitted, MilestoneState::Disputed)
                | (MilestoneState::Submitted, MilestoneState::Refunded)
                | (MilestoneState::Disputed, MilestoneState::Refunded)
        )
    }

    pub fn ensure_transition(self, next: MilestoneState) -> Result<(), DomainError> {
        if self.can_transition_to(next) {
            Ok(())
        } else {
            Err(DomainError::InvalidMilestoneTransition {
                from: self.as_str(),
                to: next.as_str(),
            })
        }
    }

    /// Terminal states can never change again.
    pub const fn is_terminal(self) -> bool {
        matches!(self, MilestoneState::Released | MilestoneState::Refunded)
    }

    /// States in which the escrowed amount is still inside the vault.
    pub const fn holds_funds(self) -> bool {
        matches!(
            self,
            MilestoneState::Funded | MilestoneState::Submitted | MilestoneState::Disputed
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agreement_ordinals_match_the_program() {
        assert_eq!(AgreementState::Draft.as_u8(), 0);
        assert_eq!(AgreementState::Active.as_u8(), 1);
        assert_eq!(AgreementState::Completed.as_u8(), 2);
        assert_eq!(AgreementState::Cancelled.as_u8(), 3);
        for state in [
            AgreementState::Draft,
            AgreementState::Active,
            AgreementState::Completed,
            AgreementState::Cancelled,
        ] {
            assert_eq!(AgreementState::from_u8(state.as_u8()).unwrap(), state);
        }
        assert!(AgreementState::from_u8(9).is_err());
    }

    #[test]
    fn milestone_ordinals_match_the_program() {
        let states = [
            (MilestoneState::Pending, 0),
            (MilestoneState::Funded, 1),
            (MilestoneState::Submitted, 2),
            (MilestoneState::Released, 3),
            (MilestoneState::Refunded, 4),
            (MilestoneState::Disputed, 5),
        ];
        for (state, ordinal) in states {
            assert_eq!(state.as_u8(), ordinal);
            assert_eq!(MilestoneState::from_u8(ordinal).unwrap(), state);
        }
        assert!(MilestoneState::from_u8(6).is_err());
    }

    #[test]
    fn serde_is_lowercase() {
        assert_eq!(
            serde_json::to_string(&AgreementState::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(
            serde_json::to_string(&MilestoneState::Disputed).unwrap(),
            "\"disputed\""
        );
    }

    #[test]
    fn agreement_transitions_are_exhaustive() {
        let all = [
            AgreementState::Draft,
            AgreementState::Active,
            AgreementState::Completed,
            AgreementState::Cancelled,
        ];
        let mut allowed = 0;
        for from in all {
            for to in all {
                if from.can_transition_to(to) {
                    allowed += 1;
                }
            }
        }
        assert_eq!(allowed, 3);
        assert!(AgreementState::Draft.can_transition_to(AgreementState::Active));
        assert!(AgreementState::Draft.can_transition_to(AgreementState::Cancelled));
        assert!(AgreementState::Active.can_transition_to(AgreementState::Completed));
        assert!(!AgreementState::Active.can_transition_to(AgreementState::Draft));
        assert!(!AgreementState::Completed.can_transition_to(AgreementState::Active));
        assert!(!AgreementState::Cancelled.can_transition_to(AgreementState::Active));
    }

    #[test]
    fn milestone_transitions_are_exhaustive() {
        let all = [
            MilestoneState::Pending,
            MilestoneState::Funded,
            MilestoneState::Submitted,
            MilestoneState::Released,
            MilestoneState::Refunded,
            MilestoneState::Disputed,
        ];
        let mut allowed = 0;
        for from in all {
            for to in all {
                if from.can_transition_to(to) {
                    allowed += 1;
                }
            }
        }
        assert_eq!(allowed, 8);
        assert!(MilestoneState::Submitted.can_transition_to(MilestoneState::Released));
        assert!(MilestoneState::Disputed.can_transition_to(MilestoneState::Refunded));
        assert!(!MilestoneState::Disputed.can_transition_to(MilestoneState::Released));
        assert!(!MilestoneState::Released.can_transition_to(MilestoneState::Refunded));
    }

    #[test]
    fn ensure_transition_reports_the_pair() {
        let err = MilestoneState::Pending
            .ensure_transition(MilestoneState::Released)
            .unwrap_err();
        assert_eq!(
            err,
            DomainError::InvalidMilestoneTransition {
                from: "pending",
                to: "released"
            }
        );
    }

    #[test]
    fn terminal_states_hold_no_funds() {
        assert!(MilestoneState::Released.is_terminal());
        assert!(MilestoneState::Refunded.is_terminal());
        assert!(MilestoneState::Funded.holds_funds());
        assert!(!MilestoneState::Pending.holds_funds());
    }
}
