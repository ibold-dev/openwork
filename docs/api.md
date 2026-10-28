# API

Wallet-authenticated metadata and reconciliation for OpenWork. The API **never
signs, never custodies and never holds a balance**. Its job is to remember titles
and quotes, and to confirm that the signatures users claim actually landed.

- Base URL (local): `http://127.0.0.1:8080`
- Contract: [`apps/api/openapi.yaml`](../apps/api/openapi.yaml) (OpenAPI 3.0.3, 16 paths, 12 schemas)
- Storage: Postgres 16; migrations are embedded and run on startup
- Health: `GET /health/live`, `GET /health/ready`

## Authentication

Sign-In-With-Solana:

```http
POST /v1/auth/challenge
{ "address": "<base58>", "cluster": "devnet" }
```

The response contains the exact SIWS `message`. The wallet signs those UTF-8
bytes with `solana:signMessage` (not a transaction). Then:

```http
POST /v1/auth/verify
{ "challengeId": "<uuid>", "signature": "<base58 64-byte signature>" }
```

A successful verify sets an HttpOnly `openwork_session` cookie and returns the
user plus a CSRF token. Unsafe methods require:

- the session cookie,
- `x-openwork-csrf: <token>`,
- an `Origin` header on `CORS_ALLOWED_ORIGINS`.

Challenges are single-use and expire. Session tokens are stored hashed.

## Endpoints

| Method | Path                               | Purpose                               |
| ------ | ---------------------------------- | ------------------------------------- |
| `POST` | `/v1/auth/challenge`               | Create a sign-in challenge            |
| `POST` | `/v1/auth/verify`                  | Verify the signature, start a session |
| `POST` | `/v1/auth/logout`                  | Revoke the session                    |
| `GET`  | `/v1/me`                           | Current user and CSRF token           |
| `GET`  | `/v1/contracts`                    | Contracts the wallet is party to      |
| `POST` | `/v1/contracts`                    | Index a contract draft                |
| `GET`  | `/v1/contracts/{id}`               | Read one contract                     |
| `POST` | `/v1/contracts/{id}/milestones`    | Append a milestone while draft        |
| `POST` | `/v1/contracts/{id}/accept-intent` | Off-chain acceptance note             |
| `POST` | `/v1/milestones/{id}/submit`       | Record a deliverable claim            |
| `GET`  | `/v1/milestones/{id}/transactions` | Signatures observed for a milestone   |
| `POST` | `/v1/settlements`                  | Claim an on-chain signature           |
| `POST` | `/v1/quotes`                       | Request a funding quote               |
| `GET`  | `/v1/quotes/{id}`                  | Read a quote and its status           |

### Browser-created agreements

When the browser has already created the agreement on chain, `POST /v1/contracts`
accepts `agreementNonce` and `termsHash` together. The server derives the same
PDA and verifies the hash instead of allocating a nonce. Supplying only one is a
validation error; re-indexing the same agreement returns `409` with
`agreement_already_indexed`.

## Errors

All errors use one envelope:

```json
{
  "error": {
    "code": "validation_error",
    "message": "termsHash does not match the canonical terms",
    "requestId": "8f2c…"
  }
}
```

| Status | When                                  |
| ------ | ------------------------------------- |
| `400`  | Malformed session, consumed challenge |
| `401`  | No valid session                      |
| `403`  | Authenticated but not a party         |
| `404`  | Unknown contract, milestone or quote  |
| `409`  | Idempotency or agreement conflict     |
| `415`  | Body is not `application/json`        |
| `422`  | Validation failure                    |
| `429`  | Rate limited                          |
| `503`  | Database or RPC unavailable           |

Validation is `422`, not `400`. Do not branch on message text; branch on `code`.

## Idempotency

Unsafe requests may send `Idempotency-Key`. The API stores a hash of the request
with the response:

- same key + same body → the stored response is replayed;
- same key + different body → `409`;
- no key → the request runs normally.

Settlement claims are additionally idempotent per `(contract, signature)`.

## Settlements and reconciliation

`POST /v1/settlements` records a claim and enqueues an outbox job. The response is
`pending`; it is **not** evidence that a payment happened. A separate worker:

1. leases the job (`FOR UPDATE SKIP LOCKED`),
2. checks `getSignatureStatuses`,
3. fetches the transaction and parses Anchor events,
4. decodes the agreement and milestone accounts,
5. verifies the event kind and actor against the claim,
6. records `chain_events` (deduplicated) and updates `chain_*` snapshots.

Only a confirmed transaction with a matching event moves a claim to `verified`.

## Quotes

`POST /v1/quotes` requires `recipient` to equal the authenticated wallet. OpenWork
never routes funds to a third party, so a quote for someone else's address is
rejected.

On this deployment the adapter is the mock:

- every quote has `simulated: true`;
- the deposit address is derived locally and is not a real 1Click address;
- status stops at `escrow_funding_pending` and never reports delivery.

The real 1Click adapter is compiled behind a Mainnet-only configuration. There is
no runtime fallback, because a fallback would hand a Mainnet user a fake deposit
address.

## Rate limits

A per-process token bucket applies to all routes. Exceeding it returns `429`.
Counters are not shared between replicas; use an edge limiter for multi-instance
deployments.

## Example

```sh
# 1. challenge
curl -sS -X POST http://127.0.0.1:8080/v1/auth/challenge \
  -H 'content-type: application/json' \
  -d '{"address":"5ok8FfjBgwDyfAFWYPx3yQ3Ty7s962XPthsKypNVgTup","cluster":"devnet"}'

# 2. sign the returned message in a wallet, then verify
curl -sS -c cookies.txt -X POST http://127.0.0.1:8080/v1/auth/verify \
  -H 'content-type: application/json' \
  -d '{"challengeId":"<uuid>","signature":"<base58>"}'

# 3. read the session and CSRF token
curl -sS -b cookies.txt http://127.0.0.1:8080/v1/me
```

Every unsafe request additionally needs `-H 'origin: http://localhost:3000'`
and `-H "x-openwork-csrf: <token>"`.
