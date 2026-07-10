# syntax=docker/dockerfile:1
#
# Build context must be the repository root so this image uses the canonical
# workspace lockfile shared by local development and CI.

FROM node:24-slim AS base

ENV PNPM_HOME="/pnpm"
ENV PATH="$PNPM_HOME:$PATH"

RUN corepack enable && corepack prepare pnpm@10.28.1 --activate

WORKDIR /app

FROM base AS deps

COPY pnpm-lock.yaml pnpm-workspace.yaml package.json ./
COPY client/package.json ./client/
COPY cloud/package.json ./cloud/
COPY landing/package.json ./landing/
COPY packages/ui/package.json ./packages/ui/
RUN pnpm install --frozen-lockfile --filter landing...

FROM base AS builder

ENV NEXT_TELEMETRY_DISABLED=1

COPY --from=deps /app/node_modules ./node_modules
COPY --from=deps /app/landing/node_modules ./landing/node_modules
COPY pnpm-lock.yaml pnpm-workspace.yaml package.json ./
COPY landing ./landing
WORKDIR /app/landing
RUN pnpm build

FROM node:24-slim AS runner

ENV HOSTNAME=0.0.0.0
ENV NEXT_TELEMETRY_DISABLED=1
ENV NODE_ENV=production
ENV PORT=3000

WORKDIR /app

RUN groupadd --system --gid 1001 nodejs \
  && useradd --system --uid 1001 nextjs

COPY --from=builder /app/landing/public ./public
COPY --from=builder --chown=nextjs:nodejs /app/landing/.next/standalone ./
COPY --from=builder --chown=nextjs:nodejs /app/landing/.next/static ./.next/static

USER nextjs

EXPOSE 3000

CMD ["node", "server.js"]
