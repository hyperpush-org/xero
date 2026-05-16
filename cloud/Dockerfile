# syntax=docker/dockerfile:1
#
# Build context must be the repo root (see cloud/fly.toml). The container
# bundles the cloud web app along with the shared `@xero/ui` workspace.

FROM node:24-slim AS base

ENV PNPM_HOME="/pnpm"
ENV PATH="$PNPM_HOME:$PATH"
RUN corepack enable && corepack prepare pnpm@10.28.1 --activate

WORKDIR /app

# ----- Dependencies (install root workspace) -----
FROM base AS deps

COPY pnpm-lock.yaml pnpm-workspace.yaml package.json ./
COPY cloud/package.json ./cloud/
COPY packages/ui/package.json ./packages/ui/
# Other workspace members are pulled in so pnpm sees the full graph, but their
# code is only mounted in the build stage.
COPY client/package.json ./client/
COPY landing/package.json ./landing/

RUN pnpm install --frozen-lockfile --filter cloud... --filter @xero/ui

# ----- Build the cloud app -----
FROM base AS builder

COPY --from=deps /app/node_modules ./node_modules
COPY pnpm-lock.yaml pnpm-workspace.yaml package.json ./
COPY cloud ./cloud
COPY packages ./packages

RUN pnpm --filter cloud build

# Slim down the production node_modules before copying into the runtime image.
RUN pnpm install --frozen-lockfile --filter cloud --prod=false --no-optional

# ----- Runtime image -----
FROM node:24-slim AS runner

ENV HOST=0.0.0.0
ENV NODE_ENV=production
ENV PORT=3000

WORKDIR /app

RUN groupadd --system --gid 1001 nodejs \
  && useradd --system --uid 1001 cloud

COPY --from=builder --chown=cloud:nodejs /app/cloud/.output ./.output

USER cloud

EXPOSE 3000

CMD ["node", ".output/server/index.mjs"]
