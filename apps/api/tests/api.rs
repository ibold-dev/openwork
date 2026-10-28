//! End-to-end HTTP tests driven through `tower::ServiceExt::oneshot`.
//!
//! The router, extractors, middleware, handlers and the in-memory store all run
//! for real; only PostgreSQL and the Solana RPC endpoint are absent, so these
//! tests need no services and complete in milliseconds.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use openwork_api::auth::{build_message, STATEMENT};
use openwork_api::store::Challenge;
use openwork_api::{build_app, AppState, Config, MemoryStore, SolanaRpc, Store};

const ORIGIN: &str = "https://app.openwork.test";
const DOMAIN: &str = "app.openwork.test";
const CLUSTER: &str = "devnet";

const CLIENT_SEED: [u8; 32] = [7u8; 32];
const FREELANCER_SEED: [u8; 32] = [8u8; 32];
const OUTSIDER_SEED: [u8; 32] = [11u8; 32];

struct TestApp {
    router: axum::Router,
    store: Arc<MemoryStore>,
}

#[derive(Debug, Clone)]
struct Session {
    wallet: String,
    token: String,
    csrf: String,
}

impl Session {
    fn headers(&self) -> Vec<(&'static str, String)> {
        vec![
            ("cookie", format!("openwork_session={}", self.token)),
            ("origin", ORIGIN.to_string()),
            ("x-openwork-csrf", self.csrf.clone()),
        ]
    }
}

impl TestApp {
    fn new() -> Self {
        let store = Arc::new(MemoryStore::new());
        let config = Config::for_tests();
        let rpc = Arc::new(
            SolanaRpc::new(config.solana_rpc_url.clone(), config.rpc_timeout).expect("rpc client"),
        );
        let state = AppState::new(config, store.clone(), rpc).expect("app state");
        TestApp {
            router: build_app(state),
            store,
        }
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        headers: &[(&str, String)],
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
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

        let response = self.router.clone().oneshot(request).await.expect("router");
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes()
            .to_vec();
        (status, headers, bytes)
    }

    async fn get(&self, path: &str, session: &Session) -> (StatusCode, Value) {
        let (status, _headers, bytes) =
            self.call(Method::GET, path, None, &session.headers()).await;
        (status, decode(&bytes))
    }

    async fn post(&self, path: &str, body: &Value, session: &Session) -> (StatusCode, Value) {
        let (status, _headers, bytes) = self
            .call(Method::POST, path, Some(body), &session.headers())
            .await;
        (status, decode(&bytes))
    }

    async fn challenge(&self, wallet: &str) -> (Uuid, String) {
        let (status, _headers, bytes) = self
            .call(
                Method::POST,
                "/v1/auth/challenge",
                Some(&json!({ "address": wallet })),
                &[],
            )
            .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "challenge failed: {}",
            String::from_utf8_lossy(&bytes)
        );
        let body = decode(&bytes);
        let challenge_id = body["challengeId"]
            .as_str()
            .expect("challengeId")
            .parse()
            .expect("uuid");
        let message = body["message"].as_str().expect("message").to_string();
        (challenge_id, message)
    }

    async fn verify(&self, challenge_id: Uuid, signature: &str) -> (StatusCode, HeaderMap, Value) {
        let (status, headers, bytes) = self
            .call(
                Method::POST,
                "/v1/auth/verify",
                Some(&json!({
                    "challengeId": challenge_id,
                    "signature": signature,
                })),
                &[],
            )
            .await;
        (status, headers, decode(&bytes))
    }

    /// Full SIWS login, returning the session plus the challenge id and
    /// signature so replay tests can reuse them.
    async fn login(&self, seed: [u8; 32]) -> (Session, Uuid, String) {
        let key = SigningKey::from_bytes(&seed);
        let wallet = wallet_for(&key);
        let (challenge_id, message) = self.challenge(&wallet).await;
        let signature = sign(&key, &message);

        let (status, headers, body) = self.verify(challenge_id, &signature).await;
        assert_eq!(status, StatusCode::OK, "verify failed: {body}");

        let cookie = headers
            .get(header::SET_COOKIE)
            .expect("set-cookie")
            .to_str()
            .expect("ascii cookie");
        let token = cookie
            .split(';')
            .next()
            .expect("cookie pair")
            .strip_prefix("openwork_session=")
            .expect("session cookie name")
            .to_string();
        assert!(
            cookie.contains("HttpOnly"),
            "cookie must be HttpOnly: {cookie}"
        );
        assert!(
            cookie.contains("SameSite=Lax"),
            "cookie must be SameSite=Lax: {cookie}"
        );

        let csrf = body["csrfToken"].as_str().expect("csrfToken").to_string();
        (
            Session {
                wallet,
                token,
                csrf,
            },
            challenge_id,
            signature,
        )
    }

    async fn sign_in(&self, seed: [u8; 32]) -> Session {
        self.login(seed).await.0
    }
}

fn decode(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap_or(Value::Null)
}

fn wallet_for(key: &SigningKey) -> String {
    bs58::encode(key.verifying_key().to_bytes()).into_string()
}

fn sign(key: &SigningKey, message: &str) -> String {
    bs58::encode(key.sign(message.as_bytes()).to_bytes()).into_string()
}

fn mint() -> String {
    bs58::encode([9u8; 32]).into_string()
}

/// A syntactically valid 64-byte transaction signature.
fn tx_signature(byte: u8) -> String {
    bs58::encode([byte; 64]).into_string()
}

fn contract_payload() -> Value {
    json!({
        "freelancer": wallet_for(&SigningKey::from_bytes(&FREELANCER_SEED)),
        "mint": mint(),
        "title": "Landing page",
        "description": "Design and build the marketing site",
        "milestones": [
            { "title": "Design", "description": "Wireframes", "amount": "1000000" },
            { "title": "Build", "amount": "2500000" }
        ]
    })
}

#[tokio::test]
async fn auth_happy_path_creates_and_reads_a_contract() {
    let app = TestApp::new();
    let session = app.sign_in(CLIENT_SEED).await;

    let (status, me) = app.get("/v1/me", &session).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["user"]["wallet"], session.wallet);
    assert_eq!(me["cluster"], CLUSTER);
    assert_eq!(me["csrfToken"], session.csrf);

    let (status, contract) = app
        .post("/v1/contracts", &contract_payload(), &session)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{contract}");
    let id = contract["id"].as_str().expect("id").to_string();

    assert_eq!(contract["role"], "client");
    assert_eq!(contract["state"], "draft");
    assert_eq!(contract["draftState"], "draft");
    assert_eq!(contract["totalAmount"], "3500000");
    assert_eq!(contract["milestoneCount"], 2);
    let terms_hash = contract["termsHash"].as_str().expect("termsHash");
    assert_eq!(terms_hash.len(), 64);
    assert!(
        terms_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "termsHash must be lowercase hex: {terms_hash}"
    );
    assert_eq!(
        contract["termsHashBytes"].as_array().expect("bytes").len(),
        32
    );

    let (status, fetched) = app.get(&format!("/v1/contracts/{id}"), &session).await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["termsHash"], contract["termsHash"]);
    assert_eq!(fetched["totalAmount"], "3500000");
    let milestones = fetched["milestones"].as_array().expect("milestones");
    assert_eq!(milestones.len(), 2);
    assert_eq!(milestones[0]["index"].as_u64(), Some(0));
    assert_eq!(milestones[1]["index"].as_u64(), Some(1));
    assert_eq!(milestones[0]["amount"], "1000000");
    assert_eq!(milestones[0]["state"], "pending");

    // The same terms reconstructed by the client must hash identically.
    let expected = openwork_domain::Terms::new(
        "Landing page".to_string(),
        "Design and build the marketing site".to_string(),
        session.wallet.clone(),
        wallet_for(&SigningKey::from_bytes(&FREELANCER_SEED)),
        mint(),
        vec![
            openwork_domain::TermsMilestone {
                index: 0,
                title: "Design".to_string(),
                description: "Wireframes".to_string(),
                amount: openwork_domain::Amount::from_decimal_str("1000000").expect("amount"),
            },
            openwork_domain::TermsMilestone {
                index: 1,
                title: "Build".to_string(),
                description: String::new(),
                amount: openwork_domain::Amount::from_decimal_str("2500000").expect("amount"),
            },
        ],
    );
    assert_eq!(expected.hash_hex(), terms_hash);

    let actions: Vec<String> = app
        .store
        .audit_events()
        .into_iter()
        .map(|event| event.action)
        .collect();
    assert!(actions.iter().any(|action| action == "auth.login"));
    assert!(actions.iter().any(|action| action == "contract.create"));
}

#[tokio::test]
async fn a_challenge_cannot_be_used_twice() {
    let app = TestApp::new();
    let (_session, challenge_id, signature) = app.login(CLIENT_SEED).await;

    let (status, _headers, body) = app.verify(challenge_id, &signature).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "nonce_replayed");
    assert!(
        body["error"]["requestId"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "errors always carry a request id"
    );
}

#[tokio::test]
async fn expired_challenges_are_rejected_before_verification() {
    let app = TestApp::new();
    let key = SigningKey::from_bytes(&[12u8; 32]);
    let address = wallet_for(&key);
    let now = Utc::now();
    let issued_at = now - Duration::minutes(10);
    let expires_at = now - Duration::minutes(5);
    let nonce = "expired-test-nonce".to_string();
    let message = build_message(
        DOMAIN, &address, STATEMENT, ORIGIN, CLUSTER, &nonce, issued_at, expires_at,
    );

    let challenge = Challenge {
        id: Uuid::new_v4(),
        address,
        cluster: CLUSTER.to_string(),
        nonce,
        domain: DOMAIN.to_string(),
        origin: ORIGIN.to_string(),
        statement: STATEMENT.to_string(),
        message: message.clone(),
        issued_at,
        expires_at,
        consumed_at: None,
        created_at: issued_at,
    };
    app.store
        .insert_challenge(&challenge)
        .await
        .expect("insert challenge");

    let (status, _headers, body) = app.verify(challenge.id, &sign(&key, &message)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "challenge_expired");
}

#[tokio::test]
async fn cookie_mutations_require_the_csrf_header() {
    let app = TestApp::new();
    let session = app.sign_in(CLIENT_SEED).await;
    let payload = contract_payload();

    let missing: Vec<(&str, String)> = session
        .headers()
        .into_iter()
        .filter(|(name, _)| *name != "x-openwork-csrf")
        .collect();
    let (status, _headers, bytes) = app
        .call(Method::POST, "/v1/contracts", Some(&payload), &missing)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode(&bytes)["error"]["code"], "csrf_missing");

    let mut wrong = session.headers();
    for header in wrong.iter_mut() {
        if header.0 == "x-openwork-csrf" {
            header.1 = "not-the-session-token".to_string();
        }
    }
    let (status, _headers, bytes) = app
        .call(Method::POST, "/v1/contracts", Some(&payload), &wrong)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode(&bytes)["error"]["code"], "csrf_invalid");

    // The correct header succeeds, proving the two failures above were about
    // CSRF and not about the payload.
    let (status, contract) = app.post("/v1/contracts", &payload, &session).await;
    assert_eq!(status, StatusCode::CREATED, "{contract}");
}

#[tokio::test]
async fn unsafe_requests_without_an_origin_are_rejected() {
    let app = TestApp::new();
    let session = app.sign_in(CLIENT_SEED).await;
    let headers: Vec<(&str, String)> = session
        .headers()
        .into_iter()
        .filter(|(name, _)| *name != "origin")
        .collect();

    let (status, _headers, bytes) = app
        .call(
            Method::POST,
            "/v1/contracts",
            Some(&contract_payload()),
            &headers,
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(decode(&bytes)["error"]["code"], "origin_required");
}

#[tokio::test]
async fn unauthenticated_requests_are_rejected() {
    let app = TestApp::new();

    let (status, headers, bytes) = app.call(Method::GET, "/v1/me", None, &[]).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let body = decode(&bytes);
    assert_eq!(body["error"]["code"], "unauthorized");
    let request_id = body["error"]["requestId"].as_str().expect("requestId");
    assert!(!request_id.is_empty());
    assert_eq!(
        headers
            .get("x-request-id")
            .expect("x-request-id")
            .to_str()
            .expect("ascii"),
        request_id,
        "the envelope and the header must agree"
    );

    // A forged cookie is not a session.
    let (status, _headers, bytes) = app
        .call(
            Method::GET,
            "/v1/me",
            None,
            &[("cookie", "openwork_session=deadbeef".to_string())],
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(decode(&bytes)["error"]["code"], "unauthorized");
}

#[tokio::test]
async fn contracts_are_scoped_to_their_parties() {
    let app = TestApp::new();
    let client = app.sign_in(CLIENT_SEED).await;
    let (status, contract) = app
        .post("/v1/contracts", &contract_payload(), &client)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{contract}");
    let id = contract["id"].as_str().expect("id").to_string();

    let outsider = app.sign_in(OUTSIDER_SEED).await;
    let (status, body) = app.get(&format!("/v1/contracts/{id}"), &outsider).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "forbidden");

    let (status, list) = app.get("/v1/contracts", &outsider).await;
    assert_eq!(status, StatusCode::OK);
    assert!(list["contracts"].as_array().expect("list").is_empty());

    let freelancer = app.sign_in(FREELANCER_SEED).await;
    let (status, body) = app.get(&format!("/v1/contracts/{id}"), &freelancer).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["role"], "freelancer");
    assert_eq!(body["termsHash"], contract["termsHash"]);
}

#[tokio::test]
async fn settlement_claims_are_idempotent() {
    let app = TestApp::new();
    let session = app.sign_in(CLIENT_SEED).await;
    let (status, contract) = app
        .post("/v1/contracts", &contract_payload(), &session)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{contract}");
    let contract_id = contract["id"].as_str().expect("id").to_string();

    let body = json!({
        "contractId": contract_id,
        "signature": tx_signature(21),
        "kind": "create_agreement",
    });
    let mut headers = session.headers();
    headers.push(("idempotency-key", "settlement-1".to_string()));

    let (status, _headers, bytes) = app
        .call(Method::POST, "/v1/settlements", Some(&body), &headers)
        .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let first = decode(&bytes);
    assert_eq!(first["created"], true);
    assert_eq!(first["settlement"]["kind"], "create_agreement");
    assert_eq!(first["settlement"]["verificationStatus"], "unverified");
    assert_eq!(first["settlement"]["claimedBy"], session.wallet);
    assert!(first["settlement"]["milestoneIndex"].is_null());
    let settlement_id = first["settlement"]["id"].clone();

    // Replaying an Idempotency-Key returns the recorded response verbatim,
    // including the original status and body.
    let (status, _headers, bytes) = app
        .call(Method::POST, "/v1/settlements", Some(&body), &headers)
        .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "recorded status: {}",
        String::from_utf8_lossy(&bytes)
    );
    let replay = decode(&bytes);
    assert_eq!(replay["settlement"]["id"], settlement_id);
    assert_eq!(
        replay["settlement"]["createdAt"],
        first["settlement"]["createdAt"]
    );

    // Without an idempotency key the claim is deduplicated at the settlement
    // level instead, which reports the row as pre-existing.
    let (status, _headers, bytes) = app
        .call(
            Method::POST,
            "/v1/settlements",
            Some(&body),
            &session.headers(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let duplicate = decode(&bytes);
    assert_eq!(duplicate["created"], false);
    assert_eq!(duplicate["settlement"]["id"], settlement_id);

    // Replaying the key with different content must never silently succeed.
    let mut changed = body.clone();
    changed["signature"] = json!(tx_signature(22));
    let (status, _headers, bytes) = app
        .call(Method::POST, "/v1/settlements", Some(&changed), &headers)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(decode(&bytes)["error"]["code"], "idempotency_key_reuse");
}

#[tokio::test]
async fn settlement_claims_require_the_expected_role() {
    let app = TestApp::new();
    let session = app.sign_in(CLIENT_SEED).await;
    let (status, contract) = app
        .post("/v1/contracts", &contract_payload(), &session)
        .await;
    assert_eq!(status, StatusCode::CREATED, "{contract}");
    let contract_id = contract["id"].as_str().expect("id").to_string();

    // `accept_agreement` is signed by the freelancer, not by the client.
    let body = json!({
        "contractId": contract_id,
        "signature": tx_signature(23),
        "kind": "accept_agreement",
    });
    let (status, error) = app.post("/v1/settlements", &body, &session).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "forbidden_role");

    // Milestone-level kinds must name an existing index.
    let body = json!({
        "contractId": contract_id,
        "signature": tx_signature(24),
        "kind": "fund_milestone",
    });
    let (status, error) = app.post("/v1/settlements", &body, &session).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error["error"]["code"], "validation_error");

    let body = json!({
        "contractId": contract_id,
        "milestoneIndex": 9,
        "signature": tx_signature(25),
        "kind": "fund_milestone",
    });
    let (status, error) = app.post("/v1/settlements", &body, &session).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["error"]["code"], "not_found");
}

#[tokio::test]
async fn quote_creation_is_simulated_and_owner_scoped() {
    let app = TestApp::new();
    let session = app.sign_in(CLIENT_SEED).await;

    let payload = json!({
        "mode": "mock",
        "invoice": "invoice-1",
        "amount": "1000000",
        "originAsset": "nep141:usdt.tether-token.near",
        "destinationAsset": "solana:mainnet/spl:USDC",
        "recipient": session.wallet,
        "refundTo": "client.near",
    });

    let (status, quote) = app.post("/v1/quotes", &payload, &session).await;
    assert_eq!(status, StatusCode::CREATED, "{quote}");
    assert_eq!(quote["mode"], "mock");
    assert_eq!(quote["provider"], "mock");
    assert_eq!(quote["simulated"], true);
    assert_eq!(quote["status"], "requested");
    assert_eq!(quote["amountIn"], "1000000");
    assert_eq!(quote["recipient"], session.wallet);
    let id = quote["id"].as_str().expect("id").to_string();

    let (status, fetched) = app.get(&format!("/v1/quotes/{id}"), &session).await;
    assert_eq!(status, StatusCode::OK, "{fetched}");
    assert_eq!(fetched["id"], quote["id"]);
    assert_eq!(fetched["depositAddress"], quote["depositAddress"]);

    let outsider = app.sign_in(OUTSIDER_SEED).await;
    let (status, error) = app.get(&format!("/v1/quotes/{id}"), &outsider).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(error["error"]["code"], "forbidden");

    // This deployment runs in mock mode, so asking for the real provider must
    // fail closed instead of silently returning a simulated deposit address.
    let mut mainnet = payload.clone();
    mainnet["mode"] = json!("mainnet");
    let (status, error) = app.post("/v1/quotes", &mainnet, &session).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error["error"]["code"], "intents_unavailable");
}

#[tokio::test]
async fn liveness_is_unauthenticated_and_stable() {
    let app = TestApp::new();
    let (status, headers, bytes) = app.call(Method::GET, "/health/live", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode(&bytes)["status"], "ok");
    assert!(headers.get("set-cookie").is_none());
}

#[tokio::test]
async fn client_supplied_nonce_derives_the_same_pda_and_hash() {
    let app = TestApp::new();
    let session = app.sign_in(CLIENT_SEED).await;

    // The browser computes the hash from its canonical terms and commits it on
    // chain before asking the index to record the agreement.
    let terms = openwork_domain::Terms::new(
        "Landing page".to_string(),
        "Design and build the marketing site".to_string(),
        session.wallet.clone(),
        wallet_for(&SigningKey::from_bytes(&FREELANCER_SEED)),
        mint(),
        vec![
            openwork_domain::TermsMilestone {
                index: 0,
                title: "Design".to_string(),
                description: "Wireframes".to_string(),
                amount: openwork_domain::Amount::from_decimal_str("1000000").expect("amount"),
            },
            openwork_domain::TermsMilestone {
                index: 1,
                title: "Build".to_string(),
                description: String::new(),
                amount: openwork_domain::Amount::from_decimal_str("2500000").expect("amount"),
            },
        ],
    );

    let mut payload = contract_payload();
    payload["agreementNonce"] = json!("18446744073709551615");
    payload["termsHash"] = json!(terms.hash_hex());

    let (status, contract) = app.post("/v1/contracts", &payload, &session).await;
    assert_eq!(status, StatusCode::CREATED, "{contract}");
    assert_eq!(contract["agreementNonce"], "18446744073709551615");
    assert_eq!(contract["termsHash"], terms.hash_hex());

    let program_id =
        openwork_domain::pda::parse_pubkey("GT6deu8TWzjUzu5grXg4zZiAxPosK7tyoykaLtVeLjR")
            .expect("program id");
    let client = openwork_domain::pda::parse_pubkey(&session.wallet).expect("client");
    let (expected, _) = openwork_domain::pda::agreement_pda(&program_id, &client, u64::MAX);
    assert_eq!(contract["agreementPda"], expected.to_string());

    // Re-indexing the same agreement is a conflict, not a new nonce.
    let (status, body) = app.post("/v1/contracts", &payload, &session).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error"]["code"], "agreement_already_indexed");

    // A hash that disagrees with the terms is rejected before storage.
    let mut tampered = contract_payload();
    tampered["agreementNonce"] = json!("7");
    tampered["termsHash"] = json!("00".repeat(32));
    let (status, body) = app.post("/v1/contracts", &tampered, &session).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

    // Nonce without a hash is ambiguous and rejected.
    let mut half = contract_payload();
    half["agreementNonce"] = json!("7");
    let (status, _body) = app.post("/v1/contracts", &half, &session).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn the_openapi_document_covers_every_route() {
    let app = TestApp::new();
    let (status, headers, bytes) = app.call(Method::GET, "/openapi.yaml", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get(header::CONTENT_TYPE)
            .expect("content-type")
            .to_str()
            .expect("ascii"),
        "application/yaml"
    );

    let spec = String::from_utf8(bytes).expect("utf8 spec");
    for path in [
        "/openapi.yaml",
        "/health/live",
        "/health/ready",
        "/v1/auth/challenge",
        "/v1/auth/verify",
        "/v1/auth/logout",
        "/v1/me",
        "/v1/contracts",
        "/v1/contracts/{id}",
        "/v1/contracts/{id}/milestones",
        "/v1/contracts/{id}/accept-intent",
        "/v1/milestones/{id}/submit",
        "/v1/milestones/{id}/transactions",
        "/v1/settlements",
        "/v1/quotes",
        "/v1/quotes/{id}",
    ] {
        assert!(
            spec.contains(&format!("\n  {path}:\n")),
            "OpenAPI document is missing {path}"
        );
    }
}
