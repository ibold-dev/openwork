//! Program-level integration tests against an in-process SVM.
//!
//! These cover account layout, PDA determinism, role enforcement and the state
//! machine. Token movements are covered by the TypeScript integration suite
//! (`packages/sdk/tests`) against a local validator, because the SPL Token
//! program is not preloaded in LiteSVM.

use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    openwork_escrow::{
        constants::{AGREEMENT_SEED, MAX_MILESTONES, MILESTONE_SEED},
        state::{Agreement, AgreementState, Milestone, MilestoneState},
    },
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const FIXED_NOW: i64 = 1_760_000_000;

fn program_bytes() -> &'static [u8] {
    include_bytes!(concat!(
        env!("CARGO_TARGET_TMPDIR"),
        "/../deploy/openwork_escrow.so"
    ))
}

fn setup() -> (LiteSVM, Keypair, Keypair) {
    let mut svm = LiteSVM::new();
    svm.add_program(openwork_escrow::id(), program_bytes())
        .expect("program should load");
    let mut clock = svm.get_sysvar::<anchor_lang::solana_program::clock::Clock>();
    clock.unix_timestamp = FIXED_NOW;
    svm.set_sysvar(&clock);
    let client = Keypair::new();
    let freelancer = Keypair::new();
    svm.airdrop(&client.pubkey(), 100_000_000_000).unwrap();
    svm.airdrop(&freelancer.pubkey(), 100_000_000_000).unwrap();
    (svm, client, freelancer)
}

fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> Result<(), String> {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers)
        .map_err(|e| e.to_string())?;
    match svm.send_transaction(tx) {
        Ok(_) => Ok(()),
        Err(failure) => Err(format!("{failure:?}")),
    }
}

fn expect_ok(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) {
    if let Err(e) = send(svm, ixs, payer, signers) {
        panic!("expected success, got {e}");
    }
}

fn expect_fail(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, signers: &[&Keypair]) {
    if send(svm, ixs, payer, signers).is_ok() {
        panic!("expected failure, transaction succeeded");
    }
}

fn agreement_pda(client: &Pubkey, nonce: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[AGREEMENT_SEED, client.as_ref(), &nonce.to_le_bytes()],
        &openwork_escrow::id(),
    )
    .0
}

fn milestone_pda(agreement: &Pubkey, index: u16) -> Pubkey {
    Pubkey::find_program_address(
        &[MILESTONE_SEED, agreement.as_ref(), &index.to_le_bytes()],
        &openwork_escrow::id(),
    )
    .0
}

fn create_agreement_ix(
    client: &Pubkey,
    freelancer: &Pubkey,
    mint: &Pubkey,
    nonce: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        openwork_escrow::id(),
        &openwork_escrow::instruction::CreateAgreement {
            agreement_nonce: nonce,
            freelancer: *freelancer,
            mint: *mint,
            terms_hash: [7u8; 32],
        }
        .data(),
        openwork_escrow::accounts::CreateAgreement {
            client: *client,
            agreement: agreement_pda(client, nonce),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn create_milestone_ix(
    client: &Pubkey,
    agreement: &Pubkey,
    index: u16,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        openwork_escrow::id(),
        &openwork_escrow::instruction::CreateMilestone { index, amount }.data(),
        openwork_escrow::accounts::CreateMilestone {
            client: *client,
            agreement: *agreement,
            milestone: milestone_pda(agreement, index),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn accept_ix(
    freelancer: &Pubkey,
    client: &Pubkey,
    nonce: u64,
    terms_hash: [u8; 32],
) -> Instruction {
    Instruction::new_with_bytes(
        openwork_escrow::id(),
        &openwork_escrow::instruction::AcceptAgreement { terms_hash }.data(),
        openwork_escrow::accounts::AcceptAgreement {
            freelancer: *freelancer,
            agreement: agreement_pda(client, nonce),
        }
        .to_account_metas(None),
    )
}

fn load_agreement(svm: &LiteSVM, key: &Pubkey) -> Agreement {
    let account = svm.get_account(key).expect("agreement account");
    Agreement::try_deserialize(&mut account.data.as_slice()).expect("agreement decode")
}

fn load_milestone(svm: &LiteSVM, key: &Pubkey) -> Milestone {
    let account = svm.get_account(key).expect("milestone account");
    Milestone::try_deserialize(&mut account.data.as_slice()).expect("milestone decode")
}

/// Creates a draft agreement with one milestone and returns its PDA.
fn draft_with_milestone(
    svm: &mut LiteSVM,
    client: &Keypair,
    freelancer: &Keypair,
    mint: Pubkey,
) -> Pubkey {
    let nonce = 1u64;
    let agreement = agreement_pda(&client.pubkey(), nonce);
    expect_ok(
        svm,
        &[create_agreement_ix(
            &client.pubkey(),
            &freelancer.pubkey(),
            &mint,
            nonce,
        )],
        client,
        &[client],
    );
    expect_ok(
        svm,
        &[create_milestone_ix(
            &client.pubkey(),
            &agreement,
            0,
            1_000_000,
        )],
        client,
        &[client],
    );
    agreement
}

#[test]
fn creates_agreement_at_deterministic_pda() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let nonce = 42u64;
    let agreement_key = agreement_pda(&client.pubkey(), nonce);

    expect_ok(
        &mut svm,
        &[create_agreement_ix(
            &client.pubkey(),
            &freelancer.pubkey(),
            &mint,
            nonce,
        )],
        &client,
        &[&client],
    );

    let agreement = load_agreement(&svm, &agreement_key);
    assert_eq!(agreement.client, client.pubkey());
    assert_eq!(agreement.freelancer, freelancer.pubkey());
    assert_eq!(agreement.mint, mint);
    assert_eq!(agreement.agreement_nonce, nonce);
    assert_eq!(agreement.terms_hash, [7u8; 32]);
    assert_eq!(agreement.state, AgreementState::Draft);
    assert_eq!(agreement.milestone_count, 0);
    assert_eq!(agreement.total_amount, 0);
    assert_eq!(agreement.schema_version, 1);
    assert_eq!(agreement.created_at, agreement.created_at.max(0));
    assert_eq!(agreement.accepted_at, 0);
    assert_eq!(agreement_key, agreement_pda(&client.pubkey(), nonce));
}

#[test]
fn rejects_duplicate_agreement_creation() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let ix = create_agreement_ix(&client.pubkey(), &freelancer.pubkey(), &mint, 7);
    expect_ok(&mut svm, std::slice::from_ref(&ix), &client, &[&client]);
    expect_fail(&mut svm, &[ix], &client, &[&client]);
}

#[test]
fn rejects_self_dealing_and_default_wallets() {
    let (mut svm, client, _) = setup();
    let mint = Pubkey::new_unique();
    expect_fail(
        &mut svm,
        &[create_agreement_ix(
            &client.pubkey(),
            &client.pubkey(),
            &mint,
            1,
        )],
        &client,
        &[&client],
    );
    expect_fail(
        &mut svm,
        &[create_agreement_ix(
            &client.pubkey(),
            &Pubkey::default(),
            &mint,
            2,
        )],
        &client,
        &[&client],
    );
}

#[test]
fn enforces_contiguous_milestones_and_amount_bounds() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let nonce = 9u64;
    let agreement = agreement_pda(&client.pubkey(), nonce);
    expect_ok(
        &mut svm,
        &[create_agreement_ix(
            &client.pubkey(),
            &freelancer.pubkey(),
            &mint,
            nonce,
        )],
        &client,
        &[&client],
    );

    // Zero amount is rejected.
    expect_fail(
        &mut svm,
        &[create_milestone_ix(&client.pubkey(), &agreement, 0, 0)],
        &client,
        &[&client],
    );

    // Index must be contiguous.
    expect_fail(
        &mut svm,
        &[create_milestone_ix(&client.pubkey(), &agreement, 1, 10)],
        &client,
        &[&client],
    );

    expect_ok(
        &mut svm,
        &[create_milestone_ix(&client.pubkey(), &agreement, 0, 10)],
        &client,
        &[&client],
    );

    // Duplicate index cannot be created twice.
    expect_fail(
        &mut svm,
        &[create_milestone_ix(&client.pubkey(), &agreement, 0, 10)],
        &client,
        &[&client],
    );

    let state = load_agreement(&svm, &agreement);
    assert_eq!(state.milestone_count, 1);
    assert_eq!(state.total_amount, 10);
}

#[test]
fn only_the_client_can_author_terms() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let nonce = 3u64;
    let agreement = agreement_pda(&client.pubkey(), nonce);
    expect_ok(
        &mut svm,
        &[create_agreement_ix(
            &client.pubkey(),
            &freelancer.pubkey(),
            &mint,
            nonce,
        )],
        &client,
        &[&client],
    );

    expect_fail(
        &mut svm,
        &[create_milestone_ix(
            &freelancer.pubkey(),
            &agreement,
            0,
            5_000,
        )],
        &freelancer,
        &[&freelancer],
    );
}

#[test]
fn caps_milestone_count() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let nonce = 11u64;
    let agreement = agreement_pda(&client.pubkey(), nonce);
    expect_ok(
        &mut svm,
        &[create_agreement_ix(
            &client.pubkey(),
            &freelancer.pubkey(),
            &mint,
            nonce,
        )],
        &client,
        &[&client],
    );

    for index in 0..MAX_MILESTONES {
        expect_ok(
            &mut svm,
            &[create_milestone_ix(
                &client.pubkey(),
                &agreement,
                index,
                1_000,
            )],
            &client,
            &[&client],
        );
    }
    expect_fail(
        &mut svm,
        &[create_milestone_ix(
            &client.pubkey(),
            &agreement,
            MAX_MILESTONES,
            1_000,
        )],
        &client,
        &[&client],
    );
}

#[test]
fn acceptance_requires_the_freelancer_and_the_exact_terms_hash() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let agreement = draft_with_milestone(&mut svm, &client, &freelancer, mint);

    // Wrong signer.
    expect_fail(
        &mut svm,
        &[accept_ix(
            &freelancer.pubkey(),
            &client.pubkey(),
            1,
            [7u8; 32],
        )],
        &client,
        &[&client],
    );

    // Tampered terms hash.
    expect_fail(
        &mut svm,
        &[accept_ix(
            &freelancer.pubkey(),
            &client.pubkey(),
            1,
            [9u8; 32],
        )],
        &freelancer,
        &[&freelancer],
    );

    expect_ok(
        &mut svm,
        &[accept_ix(
            &freelancer.pubkey(),
            &client.pubkey(),
            1,
            [7u8; 32],
        )],
        &freelancer,
        &[&freelancer],
    );

    let state = load_agreement(&svm, &agreement);
    assert_eq!(state.state, AgreementState::Active);
    assert_eq!(state.accepted_at, FIXED_NOW);

    // Terms are frozen after acceptance.
    expect_fail(
        &mut svm,
        &[create_milestone_ix(&client.pubkey(), &agreement, 1, 5)],
        &client,
        &[&client],
    );
    // A second acceptance is rejected.
    expect_fail(
        &mut svm,
        &[accept_ix(
            &freelancer.pubkey(),
            &client.pubkey(),
            1,
            [7u8; 32],
        )],
        &freelancer,
        &[&freelancer],
    );
}

#[test]
fn acceptance_requires_at_least_one_milestone() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let agreement = agreement_pda(&client.pubkey(), 5);
    expect_ok(
        &mut svm,
        &[create_agreement_ix(
            &client.pubkey(),
            &freelancer.pubkey(),
            &mint,
            5,
        )],
        &client,
        &[&client],
    );
    expect_fail(
        &mut svm,
        &[accept_ix(
            &freelancer.pubkey(),
            &client.pubkey(),
            5,
            [7u8; 32],
        )],
        &freelancer,
        &[&freelancer],
    );
    assert_eq!(
        load_agreement(&svm, &agreement).state,
        AgreementState::Draft
    );
}

#[test]
fn milestone_pda_substitution_is_rejected() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let nonce = 13u64;
    let agreement = agreement_pda(&client.pubkey(), nonce);
    expect_ok(
        &mut svm,
        &[create_agreement_ix(
            &client.pubkey(),
            &freelancer.pubkey(),
            &mint,
            nonce,
        )],
        &client,
        &[&client],
    );
    expect_ok(
        &mut svm,
        &[create_milestone_ix(&client.pubkey(), &agreement, 0, 1_000)],
        &client,
        &[&client],
    );

    // A milestone PDA belonging to a different agreement must not be accepted.
    let foreign_agreement = agreement_pda(&client.pubkey(), 99);
    let mut ix = Instruction::new_with_bytes(
        openwork_escrow::id(),
        &openwork_escrow::instruction::SubmitMilestone {
            deliverable_hash: [3u8; 32],
        }
        .data(),
        openwork_escrow::accounts::SubmitMilestone {
            freelancer: freelancer.pubkey(),
            agreement,
            milestone: milestone_pda(&foreign_agreement, 0),
        }
        .to_account_metas(None),
    );
    ix.accounts[1].is_writable = true;
    expect_fail(&mut svm, &[ix], &freelancer, &[&freelancer]);
}

#[test]
fn only_the_freelancer_can_submit_and_only_when_funded() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let agreement = draft_with_milestone(&mut svm, &client, &freelancer, mint);
    let milestone = milestone_pda(&agreement, 0);

    let submit = |signer: &Pubkey| {
        Instruction::new_with_bytes(
            openwork_escrow::id(),
            &openwork_escrow::instruction::SubmitMilestone {
                deliverable_hash: [1u8; 32],
            }
            .data(),
            openwork_escrow::accounts::SubmitMilestone {
                freelancer: *signer,
                agreement,
                milestone,
            }
            .to_account_metas(None),
        )
    };

    // Not funded yet.
    expect_fail(
        &mut svm,
        &[submit(&freelancer.pubkey())],
        &freelancer,
        &[&freelancer],
    );

    // Accept, but still unfunded.
    expect_ok(
        &mut svm,
        &[accept_ix(
            &freelancer.pubkey(),
            &client.pubkey(),
            1,
            [7u8; 32],
        )],
        &freelancer,
        &[&freelancer],
    );
    expect_fail(
        &mut svm,
        &[submit(&freelancer.pubkey())],
        &freelancer,
        &[&freelancer],
    );
    assert_eq!(
        load_milestone(&svm, &milestone).state,
        MilestoneState::Pending
    );

    // The client is not the freelancer.
    expect_fail(&mut svm, &[submit(&client.pubkey())], &client, &[&client]);
}

#[test]
fn release_requires_client_and_submitted_state() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let agreement = draft_with_milestone(&mut svm, &client, &freelancer, mint);
    let milestone = milestone_pda(&agreement, 0);
    expect_ok(
        &mut svm,
        &[accept_ix(
            &freelancer.pubkey(),
            &client.pubkey(),
            1,
            [7u8; 32],
        )],
        &freelancer,
        &[&freelancer],
    );

    let release = |signer: &Pubkey, freelancer_account: &Pubkey| {
        Instruction::new_with_bytes(
            openwork_escrow::id(),
            &openwork_escrow::instruction::ApproveAndRelease {}.data(),
            openwork_escrow::accounts::ApproveAndRelease {
                client: *signer,
                agreement,
                milestone,
                freelancer: *freelancer_account,
                mint,
                freelancer_token_account: Pubkey::new_unique(),
                vault: Pubkey::new_unique(),
                vault_authority: Pubkey::find_program_address(
                    &[
                        openwork_escrow::constants::VAULT_AUTHORITY_SEED,
                        agreement.as_ref(),
                    ],
                    &openwork_escrow::id(),
                )
                .0,
                token_program: anchor_spl::token::ID,
            }
            .to_account_metas(None),
        )
    };

    // Wrong signer fails at the has_one constraint.
    expect_fail(
        &mut svm,
        &[release(&freelancer.pubkey(), &freelancer.pubkey())],
        &freelancer,
        &[&freelancer],
    );
    // Correct signer but the milestone is not submitted yet.
    expect_fail(
        &mut svm,
        &[release(&client.pubkey(), &freelancer.pubkey())],
        &client,
        &[&client],
    );
}

#[test]
fn dispute_freezes_release_and_only_participants_can_raise_it() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let agreement = draft_with_milestone(&mut svm, &client, &freelancer, mint);
    let milestone = milestone_pda(&agreement, 0);
    expect_ok(
        &mut svm,
        &[accept_ix(
            &freelancer.pubkey(),
            &client.pubkey(),
            1,
            [7u8; 32],
        )],
        &freelancer,
        &[&freelancer],
    );

    let dispute = |signer: &Pubkey| {
        Instruction::new_with_bytes(
            openwork_escrow::id(),
            &openwork_escrow::instruction::RaiseDispute {}.data(),
            openwork_escrow::accounts::RaiseDispute {
                participant: *signer,
                agreement,
                milestone,
            }
            .to_account_metas(None),
        )
    };

    // Not funded yet: dispute is not allowed on a pending milestone.
    expect_fail(
        &mut svm,
        &[dispute(&freelancer.pubkey())],
        &freelancer,
        &[&freelancer],
    );

    // A stranger is not a participant.
    let stranger = Keypair::new();
    svm.airdrop(&stranger.pubkey(), 1_000_000_000).unwrap();
    expect_fail(
        &mut svm,
        &[dispute(&stranger.pubkey())],
        &stranger,
        &[&stranger],
    );
}

#[test]
fn draft_cancellation_is_client_only() {
    let (mut svm, client, freelancer) = setup();
    let mint = Pubkey::new_unique();
    let agreement = draft_with_milestone(&mut svm, &client, &freelancer, mint);

    let cancel = |signer: &Pubkey| {
        Instruction::new_with_bytes(
            openwork_escrow::id(),
            &openwork_escrow::instruction::CancelDraftAgreement {}.data(),
            openwork_escrow::accounts::CancelDraftAgreement {
                client: *signer,
                agreement,
            }
            .to_account_metas(None),
        )
    };

    expect_fail(
        &mut svm,
        &[cancel(&freelancer.pubkey())],
        &freelancer,
        &[&freelancer],
    );
    expect_ok(&mut svm, &[cancel(&client.pubkey())], &client, &[&client]);
    assert_eq!(
        load_agreement(&svm, &agreement).state,
        AgreementState::Cancelled
    );
}
