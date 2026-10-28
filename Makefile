# OpenWork — common tasks.
#
# `make verify` is the gate CI runs: program tests, SDK unit tests, API tests,
# IDL drift check, typecheck and production build. Integration tests need a
# local validator and are a separate target.

SHELL := /bin/bash
.DEFAULT_GOAL := help

.PHONY: help install build typecheck test test-program test-sdk test-api test-integration \
	check-idl format format-check verify devnet-smoke devnet-info api web clean

help: ## List targets
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) |
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'

install: ## Install JS dependencies
	pnpm install --frozen-lockfile

build: ## Build the program, SDK and web app
	anchor build
	pnpm --filter @openwork/sdk build
	pnpm --filter @openwork/web build

typecheck: ## TypeScript and Rust type checks
	cargo check --workspace
	pnpm typecheck

test: test-program test-sdk test-api ## Unit tests (no validator, no network)
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

test-program: ## Escrow program tests (LiteSVM)
	cargo test -p openwork-escrow

test-sdk: ## SDK unit and fixture tests
	pnpm --filter @openwork/sdk test

test-api: ## API tests (in-memory store)
	cargo test -p openwork-api

test-integration: ## SDK lifecycle suite against a throwaway local validator
	./scripts/test-integration.sh

check-idl: ## Fail if the committed IDL and SDK discriminators drift
	node scripts/check-idl.mjs

format: ## Format Rust and TypeScript/Markdown sources
	cargo fmt --all
	pnpm format

format-check: ## Verify formatting without writing
	cargo fmt --all -- --check
	pnpm format:check

verify: check-idl test typecheck ## Full pre-push gate

devnet-smoke: ## Real Devnet end-to-end run (spends Devnet SOL)
	./scripts/devnet-smoke.sh

devnet-info: ## Print Devnet addresses and write apps/web/.env.local
	./scripts/devnet-setup.sh

api: ## Run the metadata API against DATABASE_URL
	cargo run -p openwork-api --bin api

web: ## Run the web app on http://localhost:3000
	pnpm --filter @openwork/web dev

clean: ## Remove build outputs (keeps node_modules)
	cargo clean
	rm -rf apps/web/.next packages/sdk/dist
