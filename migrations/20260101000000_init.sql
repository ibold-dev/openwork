-- OpenWork metadata schema.
--
-- Invariants encoded here:
--   * amounts are exact decimal text (never floating point),
--   * every chain-bound row carries its cluster,
--   * chain events are deduplicated by (chain, signature, event index),
--   * the API only stores metadata and reconciliation results.

CREATE TABLE users (
    id uuid PRIMARY KEY,
    wallet_address text NOT NULL UNIQUE,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT users_wallet_address_len CHECK (char_length(wallet_address) BETWEEN 32 AND 44)
);

CREATE TABLE login_challenges (
    id uuid PRIMARY KEY,
    address text NOT NULL,
    cluster text NOT NULL,
    nonce text NOT NULL UNIQUE,
    domain text NOT NULL,
    origin text NOT NULL,
    statement text NOT NULL,
    message text NOT NULL,
    issued_at timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT login_challenges_expiry CHECK (expires_at > issued_at),
    CONSTRAINT login_challenges_address_len CHECK (char_length(address) BETWEEN 32 AND 44)
);

CREATE INDEX login_challenges_address_idx ON login_challenges (address, expires_at DESC);
CREATE INDEX login_challenges_expires_idx ON login_challenges (expires_at);

CREATE TABLE wallet_sessions (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash text NOT NULL UNIQUE,
    csrf_token text NOT NULL,
    cluster text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    last_seen_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz,
    ip_address text,
    user_agent text,
    CONSTRAINT wallet_sessions_token_hash_hex CHECK (token_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT wallet_sessions_expiry CHECK (expires_at > created_at)
);

CREATE INDEX wallet_sessions_user_idx ON wallet_sessions (user_id, expires_at DESC);
CREATE INDEX wallet_sessions_active_idx ON wallet_sessions (expires_at)
    WHERE revoked_at IS NULL;

CREATE TABLE contracts (
    id uuid PRIMARY KEY,
    chain text NOT NULL,
    client_address text NOT NULL,
    freelancer_address text NOT NULL,
    mint text NOT NULL,
    title text NOT NULL,
    description text NOT NULL,
    terms_hash text NOT NULL,
    agreement_pda text NOT NULL,
    agreement_nonce text NOT NULL,
    state text NOT NULL DEFAULT 'draft',
    total_amount text NOT NULL DEFAULT '0',
    accepted_intent_at timestamptz,
    chain_state text,
    chain_milestone_count integer,
    chain_settled_count integer,
    chain_refunded_count integer,
    chain_total_amount text,
    chain_released_amount text,
    chain_slot bigint,
    chain_synced_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT contracts_state CHECK (state IN ('draft', 'active', 'completed', 'cancelled')),
    CONSTRAINT contracts_chain_state CHECK (
        chain_state IS NULL
        OR chain_state IN ('draft', 'active', 'completed', 'cancelled')
    ),
    CONSTRAINT contracts_terms_hash CHECK (terms_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT contracts_total_amount CHECK (total_amount ~ '^(0|[1-9][0-9]*)$'),
    CONSTRAINT contracts_chain_total_amount CHECK (
        chain_total_amount IS NULL OR chain_total_amount ~ '^(0|[1-9][0-9]*)$'
    ),
    CONSTRAINT contracts_chain_released_amount CHECK (
        chain_released_amount IS NULL OR chain_released_amount ~ '^(0|[1-9][0-9]*)$'
    ),
    CONSTRAINT contracts_nonce CHECK (agreement_nonce ~ '^(0|[1-9][0-9]*)$'),
    CONSTRAINT contracts_distinct_parties CHECK (client_address <> freelancer_address),
    CONSTRAINT contracts_chain_pda UNIQUE (chain, agreement_pda)
);

CREATE INDEX contracts_client_idx ON contracts (chain, client_address, created_at DESC);
CREATE INDEX contracts_freelancer_idx ON contracts (chain, freelancer_address, created_at DESC);
CREATE INDEX contracts_sync_idx ON contracts (chain, chain_synced_at);

CREATE TABLE milestones (
    id uuid PRIMARY KEY,
    contract_id uuid NOT NULL REFERENCES contracts (id) ON DELETE CASCADE,
    chain text NOT NULL,
    "index" integer NOT NULL,
    title text NOT NULL,
    description text NOT NULL,
    amount text NOT NULL,
    state text NOT NULL DEFAULT 'pending',
    milestone_pda text NOT NULL,
    deliverable_uri text,
    deliverable_note text,
    deliverable_hash text,
    funded_at timestamptz,
    submitted_at timestamptz,
    settled_at timestamptz,
    chain_state text,
    chain_deliverable_hash text,
    chain_funded_at timestamptz,
    chain_submitted_at timestamptz,
    chain_settled_at timestamptz,
    chain_slot bigint,
    chain_synced_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT milestones_index_range CHECK ("index" >= 0 AND "index" < 12),
    CONSTRAINT milestones_amount CHECK (amount ~ '^[1-9][0-9]*$'),
    CONSTRAINT milestones_state CHECK (
        state IN ('pending', 'funded', 'submitted', 'released', 'refunded', 'disputed')
    ),
    CONSTRAINT milestones_chain_state CHECK (
        chain_state IS NULL
        OR chain_state IN ('pending', 'funded', 'submitted', 'released', 'refunded', 'disputed')
    ),
    CONSTRAINT milestones_deliverable_hash CHECK (
        deliverable_hash IS NULL OR deliverable_hash ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT milestones_chain_deliverable_hash CHECK (
        chain_deliverable_hash IS NULL OR chain_deliverable_hash ~ '^[0-9a-f]{64}$'
    ),
    CONSTRAINT milestones_contract_index UNIQUE (contract_id, "index")
);

CREATE INDEX milestones_contract_idx ON milestones (contract_id, "index");
CREATE INDEX milestones_pda_idx ON milestones (chain, milestone_pda);

CREATE TABLE intent_quotes (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    contract_id uuid REFERENCES contracts (id) ON DELETE SET NULL,
    mode text NOT NULL,
    provider text NOT NULL,
    external_id text NOT NULL,
    invoice text NOT NULL,
    amount_in text NOT NULL,
    amount_out text,
    origin_asset text NOT NULL,
    destination_asset text NOT NULL,
    recipient text NOT NULL,
    refund_to text NOT NULL,
    deposit_address text NOT NULL,
    deposit_memo text,
    status text NOT NULL,
    simulated boolean NOT NULL,
    deadline timestamptz NOT NULL,
    expires_at timestamptz NOT NULL,
    raw_response jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT intent_quotes_mode CHECK (mode IN ('mock', 'oneclick')),
    CONSTRAINT intent_quotes_status CHECK (
        status IN (
            'requested', 'quoted', 'source_transaction_detected', 'routing',
            'arrived_in_client_wallet', 'escrow_funding_pending', 'escrow_funded',
            'failed', 'expired', 'refund_pending', 'refunded'
        )
    ),
    CONSTRAINT intent_quotes_amount CHECK (amount_in ~ '^[1-9][0-9]*$'),
    CONSTRAINT intent_quotes_expires CHECK (expires_at >= deadline)
);

CREATE INDEX intent_quotes_user_idx ON intent_quotes (user_id, created_at DESC);
CREATE INDEX intent_quotes_deposit_idx ON intent_quotes (provider, deposit_address);
CREATE INDEX intent_quotes_contract_idx ON intent_quotes (contract_id)
    WHERE contract_id IS NOT NULL;

CREATE TABLE intent_transfers (
    id uuid PRIMARY KEY,
    quote_id uuid NOT NULL UNIQUE REFERENCES intent_quotes (id) ON DELETE CASCADE,
    chain text NOT NULL,
    deposit_address text NOT NULL,
    status text NOT NULL,
    source_tx_signature text,
    observed_amount text,
    last_checked_at timestamptz,
    raw_response jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT intent_transfers_status CHECK (
        status IN (
            'requested', 'quoted', 'source_transaction_detected', 'routing',
            'arrived_in_client_wallet', 'escrow_funding_pending', 'escrow_funded',
            'failed', 'expired', 'refund_pending', 'refunded'
        )
    ),
    CONSTRAINT intent_transfers_observed_amount CHECK (
        observed_amount IS NULL OR observed_amount ~ '^(0|[1-9][0-9]*)$'
    )
);

CREATE INDEX intent_transfers_deposit_idx ON intent_transfers (chain, deposit_address);

CREATE TABLE onchain_transactions (
    id uuid PRIMARY KEY,
    chain text NOT NULL,
    tx_signature text NOT NULL,
    kind text NOT NULL,
    contract_id uuid NOT NULL REFERENCES contracts (id) ON DELETE CASCADE,
    milestone_id uuid REFERENCES milestones (id) ON DELETE SET NULL,
    milestone_index integer,
    claimed_by_address text NOT NULL,
    verification_status text NOT NULL DEFAULT 'unverified',
    slot bigint,
    block_time timestamptz,
    error jsonb,
    checked_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT onchain_transactions_status CHECK (
        verification_status IN ('unverified', 'verified', 'failed', 'not_found')
    ),
    CONSTRAINT onchain_transactions_kind CHECK (
        kind IN (
            'create_agreement', 'create_milestone', 'accept_agreement',
            'cancel_draft_agreement', 'fund_milestone', 'submit_milestone',
            'approve_and_release', 'raise_dispute', 'mutual_cancel_and_refund'
        )
    ),
    CONSTRAINT onchain_transactions_signature_len CHECK (char_length(tx_signature) BETWEEN 64 AND 88),
    CONSTRAINT onchain_transactions_dedupe UNIQUE (chain, tx_signature, contract_id, kind)
);

CREATE INDEX onchain_transactions_contract_idx ON onchain_transactions (contract_id, created_at DESC);
CREATE INDEX onchain_transactions_pending_idx ON onchain_transactions (chain, created_at)
    WHERE verification_status = 'unverified';

CREATE TABLE chain_events (
    id bigserial PRIMARY KEY,
    chain text NOT NULL,
    tx_signature text NOT NULL,
    event_index integer NOT NULL,
    event_name text NOT NULL,
    contract_id uuid REFERENCES contracts (id) ON DELETE SET NULL,
    agreement_pda text NOT NULL,
    milestone_pda text,
    slot bigint,
    block_time timestamptz,
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT chain_events_event_index CHECK (event_index >= 0),
    CONSTRAINT chain_events_dedupe UNIQUE (chain, tx_signature, event_index)
);

CREATE INDEX chain_events_agreement_idx ON chain_events (chain, agreement_pda, id DESC);
CREATE INDEX chain_events_milestone_idx ON chain_events (chain, milestone_pda)
    WHERE milestone_pda IS NOT NULL;

CREATE TABLE outbox_jobs (
    id uuid PRIMARY KEY,
    kind text NOT NULL,
    payload jsonb NOT NULL,
    dedupe_key text,
    status text NOT NULL DEFAULT 'pending',
    attempts integer NOT NULL DEFAULT 0,
    run_after timestamptz NOT NULL DEFAULT now(),
    locked_at timestamptz,
    locked_by text,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT outbox_jobs_status CHECK (status IN ('pending', 'running', 'done', 'failed')),
    CONSTRAINT outbox_jobs_kind CHECK (kind IN ('sync_contract', 'verify_signature')),
    CONSTRAINT outbox_jobs_attempts CHECK (attempts >= 0)
);

CREATE UNIQUE INDEX outbox_jobs_dedupe_idx ON outbox_jobs (dedupe_key)
    WHERE dedupe_key IS NOT NULL AND status IN ('pending', 'running');
CREATE INDEX outbox_jobs_ready_idx ON outbox_jobs (run_after)
    WHERE status = 'pending';

CREATE TABLE idempotency_keys (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    endpoint text NOT NULL,
    key text NOT NULL,
    request_hash text NOT NULL,
    response_status integer NOT NULL,
    response_body jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    CONSTRAINT idempotency_keys_unique UNIQUE (user_id, endpoint, key)
);

CREATE INDEX idempotency_keys_expiry_idx ON idempotency_keys (expires_at);

CREATE TABLE audit_events (
    id bigserial PRIMARY KEY,
    actor_address text,
    action text NOT NULL,
    object_type text NOT NULL,
    object_id text,
    request_id text,
    ip_address text,
    user_agent text,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX audit_events_actor_idx ON audit_events (actor_address, created_at DESC);
CREATE INDEX audit_events_object_idx ON audit_events (object_type, object_id, created_at DESC);

CREATE TABLE worker_checkpoints (
    worker text NOT NULL,
    key text NOT NULL,
    value text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (worker, key)
);
