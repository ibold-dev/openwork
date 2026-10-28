//! PostgreSQL integration tests.
//!
//! These are ignored by default because they need a live database:
//!
//! ```sh
//! DATABASE_URL=postgres://openwork:openwork@localhost:5432/openwork \
//!   cargo test -p openwork-api --test postgres -- --ignored
//! ```
//!
//! Every test uses fresh random identifiers, so the suite can be run
//! repeatedly against the same database.

use chrono::{Duration, Utc};
use serde_json::json;
use uuid::Uuid;

use openwork_api::error::StoreError;
use openwork_api::store::pg::{connect, migrate};
use openwork_api::store::worker;
use openwork_api::store::{Challenge, NewContract, NewMilestone, NewSettlement, PgStore, Store};

async fn store() -> Option<(PgStore, sqlx::PgPool)> {
    let url = match std::env::var("DATABASE_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => {
            eprintln!("skipping: DATABASE_URL is not set");
            return None;
        }
    };
    let pool = connect(&url, 4).await.expect("connect to DATABASE_URL");
    migrate(&pool).await.expect("apply migrations");
    Some((PgStore::new(pool.clone()), pool))
}

fn unique_address() -> String {
    let uuid = Uuid::new_v4();
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(uuid.as_bytes());
    bytes[16..].copy_from_slice(uuid.as_bytes());
    bs58::encode(bytes).into_string()
}

fn unique_signature() -> String {
    let uuid = Uuid::new_v4();
    let mut bytes = [0u8; 64];
    for chunk in bytes.chunks_mut(16) {
        chunk.copy_from_slice(uuid.as_bytes());
    }
    bs58::encode(bytes).into_string()
}

fn new_milestone(index: i32, amount: &str) -> NewMilestone {
    NewMilestone {
        id: Uuid::new_v4(),
        index,
        title: format!("Milestone {index}"),
        description: format!("Deliverable {index}"),
        amount: amount.to_string(),
        milestone_pda: unique_address(),
    }
}

fn new_contract() -> NewContract {
    NewContract {
        id: Uuid::new_v4(),
        chain: "devnet".to_string(),
        client_address: unique_address(),
        freelancer_address: unique_address(),
        mint: unique_address(),
        title: "Database roundtrip".to_string(),
        description: "Created by the ignored integration test".to_string(),
        terms_hash: "a".repeat(64),
        agreement_pda: unique_address(),
        agreement_nonce: "123456789".to_string(),
        total_amount: "3000000".to_string(),
        milestones: vec![new_milestone(0, "1000000"), new_milestone(1, "2000000")],
    }
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing at an empty PostgreSQL database"]
async fn challenges_are_single_use() {
    let Some((store, _pool)) = store().await else {
        return;
    };
    let now = Utc::now();
    let challenge = Challenge {
        id: Uuid::new_v4(),
        address: unique_address(),
        cluster: "devnet".to_string(),
        nonce: format!("nonce-{}", Uuid::new_v4()),
        domain: "app.openwork.test".to_string(),
        origin: "https://app.openwork.test".to_string(),
        statement: "test".to_string(),
        message: "test message".to_string(),
        issued_at: now,
        expires_at: now + Duration::minutes(5),
        consumed_at: None,
        created_at: now,
    };

    store.insert_challenge(&challenge).await.expect("insert");
    let loaded = store
        .get_challenge(challenge.id)
        .await
        .expect("get")
        .expect("challenge exists");
    assert_eq!(loaded.nonce, challenge.nonce);
    assert!(loaded.consumed_at.is_none());

    assert!(store
        .consume_challenge(challenge.id, now)
        .await
        .expect("consume"));
    assert!(
        !store
            .consume_challenge(challenge.id, now + Duration::seconds(1))
            .await
            .expect("consume again"),
        "a challenge must never be consumable twice"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing at an empty PostgreSQL database"]
async fn contracts_roundtrip_and_milestones_stay_contiguous() {
    let Some((store, _pool)) = store().await else {
        return;
    };
    let draft = new_contract();
    let expected_id = draft.id;
    let client = draft.client_address.clone();

    let contract = store.insert_contract(draft).await.expect("insert contract");
    assert_eq!(contract.id, expected_id);
    assert_eq!(contract.state, "draft");
    assert_eq!(contract.total_amount, "3000000");

    let milestones = store
        .list_milestones(contract.id)
        .await
        .expect("list milestones");
    assert_eq!(milestones.len(), 2);
    assert_eq!(milestones[0].index, 0);
    assert_eq!(milestones[1].index, 1);
    assert_eq!(milestones[0].state, "pending");

    let by_party = store
        .list_contracts("devnet", &client)
        .await
        .expect("list contracts");
    assert_eq!(by_party.len(), 1);

    let terms_hash = "b".repeat(64);
    let (updated, added) = store
        .add_milestone(contract.id, 2, new_milestone(2, "500000"), &terms_hash)
        .await
        .expect("add milestone");
    assert_eq!(added.index, 2);
    assert_eq!(updated.total_amount, "3500000");
    assert_eq!(updated.terms_hash, terms_hash);

    let error = store
        .add_milestone(contract.id, 2, new_milestone(2, "1"), &terms_hash)
        .await
        .expect_err("stale index must be rejected");
    assert!(matches!(error, StoreError::Conflict(_)), "got {error:?}");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing at an empty PostgreSQL database"]
async fn settlement_claims_are_deduplicated_and_verifiable() {
    let Some((store, pool)) = store().await else {
        return;
    };
    let contract = store.insert_contract(new_contract()).await.expect("insert");

    let claim = NewSettlement {
        id: Uuid::new_v4(),
        chain: "devnet".to_string(),
        tx_signature: unique_signature(),
        kind: "create_agreement".to_string(),
        contract_id: contract.id,
        milestone_id: None,
        milestone_index: None,
        claimed_by_address: contract.client_address.clone(),
    };

    let (first, created) = store
        .insert_settlement(claim.clone())
        .await
        .expect("insert settlement");
    assert!(created);
    assert_eq!(first.verification_status, "unverified");

    let (second, created) = store
        .insert_settlement(claim)
        .await
        .expect("insert duplicate");
    assert!(!created, "the second claim must be deduplicated");
    assert_eq!(second.id, first.id);

    worker::mark_settlement(
        &pool,
        first.id,
        "verified",
        Some(42),
        Some(Utc::now()),
        None,
    )
    .await
    .expect("mark settlement");
    let verified = worker::settlement(&pool, first.id)
        .await
        .expect("read settlement")
        .expect("settlement exists");
    assert_eq!(verified.verification_status, "verified");
    assert_eq!(verified.slot, Some(42));
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing at an empty PostgreSQL database"]
async fn outbox_dedupes_running_jobs_and_allows_reenqueue_after_done() {
    let Some((_store, pool)) = store().await else {
        return;
    };
    let dedupe_key = format!("sync:{}", Uuid::new_v4());
    let payload = json!({ "contractId": Uuid::new_v4() });

    assert!(worker::enqueue(
        &pool,
        "sync_contract",
        &payload,
        Some(&dedupe_key),
        Utc::now()
    )
    .await
    .expect("enqueue"));
    assert!(
        !worker::enqueue(
            &pool,
            "sync_contract",
            &payload,
            Some(&dedupe_key),
            Utc::now()
        )
        .await
        .expect("enqueue duplicate"),
        "a pending job with the same key must be deduplicated"
    );

    let jobs = worker::lease(&pool, "test-worker", 100)
        .await
        .expect("lease");
    let job = jobs
        .into_iter()
        .find(|job| job.payload == payload)
        .expect("leased the enqueued job");
    assert!(!job.kind.is_empty());

    assert!(
        !worker::enqueue(
            &pool,
            "sync_contract",
            &payload,
            Some(&dedupe_key),
            Utc::now()
        )
        .await
        .expect("enqueue while running"),
        "a running job must still suppress duplicates"
    );

    worker::complete(&pool, job.id).await.expect("complete");
    assert!(
        worker::enqueue(
            &pool,
            "sync_contract",
            &payload,
            Some(&dedupe_key),
            Utc::now()
        )
        .await
        .expect("reenqueue"),
        "a completed job must not block a new run"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL pointing at an empty PostgreSQL database"]
async fn chain_events_are_idempotent_and_checkpoints_persist() {
    let Some((store, pool)) = store().await else {
        return;
    };
    let contract = store.insert_contract(new_contract()).await.expect("insert");
    let signature = unique_signature();
    let payload = json!({ "delta": 1 });

    let event = worker::NewChainEvent {
        chain: "devnet",
        tx_signature: &signature,
        event_index: 0,
        event_name: "MilestoneFunded",
        contract_id: Some(contract.id),
        agreement_pda: &contract.agreement_pda,
        milestone_pda: None,
        slot: Some(7),
        block_time: None,
        payload: &payload,
    };
    assert!(worker::insert_chain_event(&pool, event)
        .await
        .expect("insert event"));
    let duplicate = worker::NewChainEvent {
        chain: "devnet",
        tx_signature: &signature,
        event_index: 0,
        event_name: "MilestoneFunded",
        contract_id: Some(contract.id),
        agreement_pda: &contract.agreement_pda,
        milestone_pda: None,
        slot: Some(7),
        block_time: None,
        payload: &payload,
    };
    assert!(
        !worker::insert_chain_event(&pool, duplicate)
            .await
            .expect("insert duplicate"),
        "(chain, signature, event index) must be unique"
    );
    assert!(
        worker::has_events_for_signature(&pool, "devnet", &signature)
            .await
            .expect("has events")
    );

    let worker_name = format!("test-{}", Uuid::new_v4());
    worker::set_checkpoint(&pool, &worker_name, "slot", "123")
        .await
        .expect("set checkpoint");
    worker::set_checkpoint(&pool, &worker_name, "slot", "456")
        .await
        .expect("update checkpoint");
    assert_eq!(
        worker::get_checkpoint(&pool, &worker_name, "slot")
            .await
            .expect("get checkpoint")
            .as_deref(),
        Some("456")
    );
}

/// A full authenticated HTTP flow against the real `PgStore`: login, draft a
/// contract, request a simulated funding quote and claim a signature. This is
/// the test that catches SQL/schema drift the in-memory store cannot see.
#[tokio::test]
#[ignore = "requires DATABASE_URL pointing at an empty PostgreSQL database"]
async fn http_flow_works_against_postgres() {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use ed25519_dalek::{Signer, SigningKey};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use openwork_api::{build_app, AppState, Config, SolanaRpc};

    let Some((store, _pool)) = store().await else {
        return;
    };

    let config = Config::for_tests();
    let rpc = Arc::new(
        SolanaRpc::new(config.solana_rpc_url.clone(), config.rpc_timeout).expect("rpc client"),
    );
    let router = build_app(AppState::new(config, Arc::new(store), rpc).expect("state"));

    async fn call(
        router: &axum::Router,
        method: Method,
        path: &str,
        body: Option<&Value>,
        headers: &[(&str, String)],
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(path);
        for (name, value) in headers {
            builder = builder.header(*name, value);
        }
        let request = match body {
            Some(value) => builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(value.to_string()))
                .expect("request"),
            None => builder.body(Body::empty()).expect("request"),
        };
        let response = router.clone().oneshot(request).await.expect("router");
        let status = response.status();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    let key = SigningKey::from_bytes(&[31u8; 32]);
    let wallet = bs58::encode(key.verifying_key().to_bytes()).into_string();

    let (status, challenge) = call(
        &router,
        Method::POST,
        "/v1/auth/challenge",
        Some(&json!({ "address": wallet })),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{challenge}");

    let message = challenge["message"].as_str().expect("message");
    let signature = bs58::encode(key.sign(message.as_bytes()).to_bytes()).into_string();
    let (status, headers, body) = {
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/auth/verify")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "challengeId": challenge["challengeId"],
                    "signature": signature,
                })
                .to_string(),
            ))
            .expect("request");
        let response = router.clone().oneshot(request).await.expect("router");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        (
            status,
            headers,
            serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null),
        )
    };
    assert_eq!(status, StatusCode::OK, "{body}");

    let cookie = headers
        .get(header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .expect("cookie")
        .split(';')
        .next()
        .expect("pair")
        .to_string();
    let session = vec![
        ("cookie", cookie),
        ("origin", "https://app.openwork.test".to_string()),
        (
            "x-openwork-csrf",
            body["csrfToken"].as_str().expect("csrf").to_string(),
        ),
    ];

    let freelancer = bs58::encode([32u8; 32]).into_string();
    let (status, contract) = call(
        &router,
        Method::POST,
        "/v1/contracts",
        Some(&json!({
            "freelancer": freelancer,
            "mint": bs58::encode([33u8; 32]).into_string(),
            "title": "Postgres flow",
            "description": "Exercises every table the HTTP API writes",
            "milestones": [
                { "title": "One", "description": "", "amount": "1000000" }
            ]
        })),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{contract}");
    let contract_id = contract["id"].clone();

    let (status, quote) = call(
        &router,
        Method::POST,
        "/v1/quotes",
        Some(&json!({
            "mode": "mock",
            "contractId": contract_id,
            "invoice": format!("invoice-{}", Uuid::new_v4()),
            "amount": "1000000",
            "originAsset": "nep141:usdt.tether-token.near",
            "destinationAsset": "solana:mainnet/spl:USDC",
            "recipient": wallet,
            "refundTo": "client.near",
        })),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{quote}");
    assert_eq!(quote["simulated"], true);

    let (status, fetched) = call(
        &router,
        Method::GET,
        &format!("/v1/quotes/{}", quote["id"].as_str().expect("quote id")),
        None,
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{fetched}");

    let (status, settlement) = call(
        &router,
        Method::POST,
        "/v1/settlements",
        Some(&json!({
            "contractId": contract_id,
            "signature": unique_signature(),
            "kind": "create_agreement",
        })),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{settlement}");
    assert_eq!(settlement["created"], true);
}
