# syntax=docker/dockerfile:1
#
# API and reconciliation worker image.
#
# Build from the repository root:
#   docker build -f infra/docker/api.Dockerfile -t openwork-api .
#
# SQLx queries are checked at runtime (no `query!` macros), so the build needs no
# database. Migrations and the OpenAPI document are embedded with `include_str!`.

FROM rust:1.89-bookworm AS builder
WORKDIR /build

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY apps/api ./apps/api
COPY programs ./programs
COPY migrations ./migrations

# Touch the entry points so Docker's layer cache does not reuse a stale build
# after only a source change.
RUN cargo build --release -p openwork-api --bins

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --create-home openwork

WORKDIR /app
COPY --from=builder /build/target/release/api /usr/local/bin/api
COPY --from=builder /build/target/release/worker /usr/local/bin/worker

USER openwork
EXPOSE 8080
ENV RUST_LOG=info
HEALTHCHECK --interval=15s --timeout=3s --start-period=20s --retries=5 \
  CMD curl -fsS http://127.0.0.1:8080/health/live || exit 1

CMD ["api"]
