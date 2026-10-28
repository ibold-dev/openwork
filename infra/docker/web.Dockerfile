# syntax=docker/dockerfile:1
#
# Web image. Next.js is built in standalone mode; only the server bundle and
# static assets are shipped.
#
# Build from the repository root:
#   docker build -f infra/docker/web.Dockerfile \
#     --build-arg NEXT_PUBLIC_SOLANA_CLUSTER=devnet \
#     --build-arg NEXT_PUBLIC_DEFAULT_MINT=ENmQKqTu9FaWJtmwk3nXiNT3GxX6F4k7rZe8t2bqUYT8 \
#     -t openwork-web .

FROM node:22-bookworm-slim AS base
ENV PNPM_HOME=/pnpm
ENV PATH="$PNPM_HOME:$PATH"
RUN corepack enable

FROM base AS deps
WORKDIR /repo
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web/package.json apps/web/package.json
COPY packages/sdk/package.json packages/sdk/package.json
RUN pnpm install --frozen-lockfile --filter @openwork/web... --filter @openwork/sdk...

FROM base AS builder
WORKDIR /repo
COPY --from=deps /repo/node_modules ./node_modules
COPY --from=deps /repo/apps/web/node_modules ./apps/web/node_modules
COPY --from=deps /repo/packages/sdk/node_modules ./packages/sdk/node_modules
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY packages/sdk ./packages/sdk
COPY apps/web ./apps/web

ARG NEXT_PUBLIC_SOLANA_CLUSTER=devnet
ARG NEXT_PUBLIC_SOLANA_RPC_URL=https://api.devnet.solana.com
ARG NEXT_PUBLIC_DEFAULT_MINT=ENmQKqTu9FaWJtmwk3nXiNT3GxX6F4k7rZe8t2bqUYT8
ARG NEXT_PUBLIC_DEFAULT_MINT_SYMBOL=tUSDC
ENV NEXT_PUBLIC_SOLANA_CLUSTER=$NEXT_PUBLIC_SOLANA_CLUSTER
ENV NEXT_PUBLIC_SOLANA_RPC_URL=$NEXT_PUBLIC_SOLANA_RPC_URL
ENV NEXT_PUBLIC_DEFAULT_MINT=$NEXT_PUBLIC_DEFAULT_MINT
ENV NEXT_PUBLIC_DEFAULT_MINT_SYMBOL=$NEXT_PUBLIC_DEFAULT_MINT_SYMBOL
ENV NEXT_TELEMETRY_DISABLED=1

RUN pnpm --filter @openwork/sdk build && pnpm --filter @openwork/web build

FROM base AS runtime
WORKDIR /app
ENV NODE_ENV=production
ENV NEXT_TELEMETRY_DISABLED=1
ENV PORT=3000
ENV HOSTNAME=0.0.0.0

RUN useradd --system --uid 10001 --create-home nextjs
COPY --from=builder --chown=nextjs:nextjs /repo/apps/web/.next/standalone ./
COPY --from=builder --chown=nextjs:nextjs /repo/apps/web/.next/static ./apps/web/.next/static
COPY --from=builder --chown=nextjs:nextjs /repo/apps/web/public ./apps/web/public

USER nextjs
EXPOSE 3000
CMD ["node", "apps/web/server.js"]
