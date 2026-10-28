//! Contract drafts and milestone proposals.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use rand::RngCore;
use uuid::Uuid;

use openwork_domain::pda::{agreement_pda, milestone_pda, parse_pubkey};
use openwork_domain::terms::TermsMilestone;
use openwork_domain::{Amount, Terms};

use crate::auth::Auth;
use crate::ctx::ReqCtx;
use crate::error::{ApiError, ApiJson};
use crate::routes::dto::{
    AddMilestoneRequest, ContractView, ContractsResponse, CreateContractRequest,
};
use crate::store::{Contract, Milestone, NewAuditEvent, NewContract, NewMilestone};
use crate::AppState;

/// Loads a contract and its milestones, or 404.
pub(crate) async fn load_contract(
    state: &AppState,
    id: Uuid,
) -> Result<(Contract, Vec<Milestone>), ApiError> {
    let contract = state
        .store
        .get_contract(id)
        .await?
        .ok_or_else(|| ApiError::not_found("contract not found"))?;
    let milestones = state.store.list_milestones(contract.id).await?;
    Ok((contract, milestones))
}

/// Object-level authorization: only client and freelancer may see a contract.
pub(crate) fn ensure_party(contract: &Contract, wallet: &str) -> Result<(), ApiError> {
    if contract.is_party(wallet) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "forbidden",
            "you are not a party to this contract",
        ))
    }
}

pub(crate) fn require_role(contract: &Contract, wallet: &str, role: &str) -> Result<(), ApiError> {
    if contract.role_of(wallet) == Some(role) {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "forbidden_role",
            format!("only the {role} may perform this action"),
        ))
    }
}

/// Rebuilds the canonical terms from stored rows, optionally appending a
/// milestone that is not persisted yet.
fn terms_from_rows(
    contract: &Contract,
    rows: &[Milestone],
    extra: Option<(&str, &str, Amount)>,
) -> Result<Terms, ApiError> {
    let mut milestones = Vec::with_capacity(rows.len() + usize::from(extra.is_some()));
    for row in rows {
        let amount = Amount::from_decimal_str(&row.amount)?;
        milestones.push(TermsMilestone {
            index: u16::try_from(row.index)
                .map_err(|_| ApiError::validation("stored milestone index out of range"))?,
            title: row.title.clone(),
            description: row.description.clone(),
            amount,
        });
    }
    if let Some((title, description, amount)) = extra {
        let index = u16::try_from(milestones.len())
            .map_err(|_| ApiError::validation("too many milestones"))?;
        milestones.push(TermsMilestone {
            index,
            title: title.to_string(),
            description: description.to_string(),
            amount,
        });
    }

    Ok(Terms::new(
        contract.title.clone(),
        contract.description.clone(),
        contract.client_address.clone(),
        contract.freelancer_address.clone(),
        contract.mint.clone(),
        milestones,
    ))
}

/// `POST /v1/contracts`
pub async fn create_contract(
    State(state): State<AppState>,
    ctx: ReqCtx,
    auth: Auth,
    ApiJson(body): ApiJson<CreateContractRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let client = auth.wallet().to_string();
    let freelancer = body.freelancer.trim().to_string();
    let mint = body.mint.trim().to_string();

    parse_pubkey(&freelancer)
        .map_err(|_| ApiError::validation("freelancer must be a base58 Solana address"))?;
    parse_pubkey(&mint)
        .map_err(|_| ApiError::validation("mint must be a base58 Solana address"))?;
    if freelancer == client {
        return Err(ApiError::validation(
            "client and freelancer must be different wallets",
        ));
    }

    let mut terms_milestones = Vec::with_capacity(body.milestones.len());
    for (index, input) in body.milestones.iter().enumerate() {
        terms_milestones.push(TermsMilestone {
            index: u16::try_from(index).map_err(|_| ApiError::validation("too many milestones"))?,
            title: input.title.clone(),
            description: input.description.clone(),
            amount: input.amount,
        });
    }

    let terms = Terms::new(
        body.title,
        body.description,
        client.clone(),
        freelancer.clone(),
        mint.clone(),
        terms_milestones,
    );
    terms.validate()?;
    let terms_hash = terms.hash_hex();
    let total_amount = terms.total_amount()?;

    let program_id = state.program_id;

    // A browser that already created the agreement on chain knows the nonce it
    // used, so the index must derive the same PDA. Without one, the server
    // allocates a nonce and the client is expected to create that agreement.
    let supplied_nonce = match body.agreement_nonce.as_deref() {
        Some(value) => Some(
            value
                .trim()
                .parse::<u64>()
                .map_err(|_| ApiError::validation("agreementNonce must be a u64"))?,
        ),
        None => None,
    };
    if supplied_nonce.is_some() != body.terms_hash.is_some() {
        return Err(ApiError::validation(
            "agreementNonce and termsHash must be supplied together",
        ));
    }
    if let Some(supplied) = body.terms_hash.as_deref() {
        let normalized = supplied
            .trim()
            .trim_start_matches("0x")
            .to_ascii_lowercase();
        if normalized != terms_hash {
            return Err(ApiError::validation(format!(
                "termsHash does not match the canonical terms (expected {terms_hash})"
            )));
        }
    }

    // Nonce collisions are astronomically unlikely, but the unique
    // (chain, agreement_pda) constraint makes a retry safe. A client-supplied
    // nonce is never retried: a conflict means the agreement is already
    // indexed, which is a client error rather than a collision.
    let nonces: Vec<u64> = match supplied_nonce {
        Some(nonce) => vec![nonce],
        None => (0..4).map(|_| rand::rngs::OsRng.next_u64()).collect(),
    };

    let mut last_error = None;
    for nonce in nonces {
        let (agreement, _bump) =
            agreement_pda(&program_id, &parse_pubkey(&client).expect("client"), nonce);

        let mut milestones = Vec::with_capacity(terms.milestones.len());
        for milestone in &terms.milestones {
            let (pda, _) = milestone_pda(&program_id, &agreement, milestone.index);
            milestones.push(NewMilestone {
                id: Uuid::new_v4(),
                index: i32::from(milestone.index),
                title: milestone.title.clone(),
                description: milestone.description.clone(),
                amount: milestone.amount.to_string(),
                milestone_pda: pda.to_string(),
            });
        }

        let draft = NewContract {
            id: Uuid::new_v4(),
            chain: auth.cluster().to_string(),
            client_address: client.clone(),
            freelancer_address: freelancer.clone(),
            mint: mint.clone(),
            title: terms.title.clone(),
            description: terms.description.clone(),
            terms_hash: terms_hash.clone(),
            agreement_pda: agreement.to_string(),
            agreement_nonce: nonce.to_string(),
            total_amount: total_amount.to_string(),
            milestones,
        };

        match state.store.insert_contract(draft).await {
            Ok(contract) => {
                let milestones = state.store.list_milestones(contract.id).await?;
                state
                    .store
                    .insert_audit(NewAuditEvent {
                        actor_address: Some(client.clone()),
                        action: "contract.create".to_string(),
                        object_type: "contract".to_string(),
                        object_id: Some(contract.id.to_string()),
                        request_id: Some(ctx.request_id.clone()),
                        ip_address: ctx.client_ip.clone(),
                        user_agent: ctx.user_agent.clone(),
                        metadata: serde_json::json!({
                            "chain": contract.chain,
                            "termsHash": contract.terms_hash,
                            "milestoneCount": milestones.len(),
                        }),
                    })
                    .await?;
                return Ok((
                    StatusCode::CREATED,
                    Json(ContractView::new(&contract, &milestones, &client)),
                ));
            }
            Err(crate::error::StoreError::Conflict(message)) => {
                last_error = Some(message);
            }
            Err(err) => return Err(err.into()),
        }
    }

    let (code, message) = match supplied_nonce {
        Some(_) => (
            "agreement_already_indexed",
            "this agreement is already indexed for this wallet".to_string(),
        ),
        None => (
            "agreement_nonce_collision",
            last_error.unwrap_or_else(|| "could not allocate an agreement nonce".to_string()),
        ),
    };
    Err(ApiError::conflict(code, message))
}

/// `GET /v1/contracts`
pub async fn list_contracts(
    State(state): State<AppState>,
    auth: Auth,
) -> Result<impl IntoResponse, ApiError> {
    let contracts = state
        .store
        .list_contracts(auth.cluster(), auth.wallet())
        .await?;

    let mut views = Vec::with_capacity(contracts.len());
    for contract in contracts {
        let milestones = state.store.list_milestones(contract.id).await?;
        views.push(ContractView::new(&contract, &milestones, auth.wallet()));
    }

    Ok((StatusCode::OK, Json(ContractsResponse { contracts: views })))
}

/// `GET /v1/contracts/:id`
pub async fn get_contract(
    State(state): State<AppState>,
    auth: Auth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let (contract, milestones) = load_contract(&state, id).await?;
    ensure_party(&contract, auth.wallet())?;
    if contract.chain != auth.cluster() {
        return Err(ApiError::forbidden(
            "forbidden_cluster",
            "contract belongs to a different cluster",
        ));
    }

    Ok((
        StatusCode::OK,
        Json(ContractView::new(&contract, &milestones, auth.wallet())),
    ))
}

/// `POST /v1/contracts/:id/milestones`
pub async fn add_milestone(
    State(state): State<AppState>,
    ctx: ReqCtx,
    auth: Auth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    ApiJson(body): ApiJson<AddMilestoneRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let (contract, milestones) = load_contract(&state, id).await?;
    ensure_party(&contract, auth.wallet())?;
    require_role(&contract, auth.wallet(), "client")?;

    if contract.effective_state() != "draft" {
        return Err(ApiError::conflict(
            "terms_frozen",
            "milestones can only be added while the agreement is a draft",
        ));
    }
    if contract.accepted_intent_at.is_some() {
        return Err(ApiError::conflict(
            "terms_frozen",
            "the freelancer already accepted these terms",
        ));
    }

    let expected_index = milestones.len();
    if let Some(requested) = body.index {
        if usize::from(requested) != expected_index {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "non_contiguous_milestone",
                format!("expected milestone index {expected_index}, got {requested}"),
            ));
        }
    }
    if expected_index >= usize::from(openwork_domain::limits::MAX_MILESTONES) {
        return Err(ApiError::conflict(
            "too_many_milestones",
            format!(
                "an agreement may hold at most {} milestones",
                openwork_domain::limits::MAX_MILESTONES
            ),
        ));
    }

    let terms = terms_from_rows(
        &contract,
        &milestones,
        Some((&body.title, &body.description, body.amount)),
    )?;
    terms.validate()?;
    let terms_hash = terms.hash_hex();

    let program_id = state.program_id;
    let agreement = parse_pubkey(&contract.agreement_pda)
        .map_err(|_| ApiError::internal("stored agreement PDA is invalid"))?;
    let (pda, _) = milestone_pda(&program_id, &agreement, expected_index as u16);

    let new_milestone = NewMilestone {
        id: Uuid::new_v4(),
        index: expected_index as i32,
        title: body.title,
        description: body.description,
        amount: body.amount.to_string(),
        milestone_pda: pda.to_string(),
    };

    let (contract, milestone) = state
        .store
        .add_milestone(
            contract.id,
            expected_index as i32,
            new_milestone,
            &terms_hash,
        )
        .await?;
    let milestones = state.store.list_milestones(contract.id).await?;

    state
        .store
        .insert_audit(NewAuditEvent {
            actor_address: Some(auth.wallet().to_string()),
            action: "milestone.create".to_string(),
            object_type: "milestone".to_string(),
            object_id: Some(milestone.id.to_string()),
            request_id: Some(ctx.request_id),
            ip_address: ctx.client_ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({
                "contractId": contract.id,
                "index": milestone.index,
                "termsHash": contract.terms_hash,
            }),
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(ContractView::new(&contract, &milestones, auth.wallet())),
    ))
}

/// `POST /v1/contracts/:id/accept-intent`
pub async fn accept_intent(
    State(state): State<AppState>,
    ctx: ReqCtx,
    auth: Auth,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<impl IntoResponse, ApiError> {
    let (contract, milestones) = load_contract(&state, id).await?;
    ensure_party(&contract, auth.wallet())?;
    require_role(&contract, auth.wallet(), "freelancer")?;

    if contract.effective_state() != "draft" {
        return Err(ApiError::conflict(
            "agreement_not_draft",
            "the agreement is no longer a draft",
        ));
    }
    if milestones.is_empty() {
        return Err(ApiError::conflict(
            "no_milestones",
            "the client has not proposed any milestones yet",
        ));
    }

    let contract = state
        .store
        .set_contract_accepted_intent(contract.id, Utc::now())
        .await?;

    state
        .store
        .insert_audit(NewAuditEvent {
            actor_address: Some(auth.wallet().to_string()),
            action: "contract.accept_intent".to_string(),
            object_type: "contract".to_string(),
            object_id: Some(contract.id.to_string()),
            request_id: Some(ctx.request_id),
            ip_address: ctx.client_ip,
            user_agent: ctx.user_agent,
            metadata: serde_json::json!({ "termsHash": contract.terms_hash }),
        })
        .await?;

    Ok((
        StatusCode::OK,
        Json(ContractView::new(&contract, &milestones, auth.wallet())),
    ))
}
