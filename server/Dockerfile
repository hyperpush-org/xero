# syntax=docker/dockerfile:1

ARG ELIXIR_VERSION=1.15.7
ARG OTP_VERSION=26.2.1
ARG DEBIAN_VERSION=bookworm-20231009-slim

ARG BUILDER_IMAGE=hexpm/elixir:${ELIXIR_VERSION}-erlang-${OTP_VERSION}-debian-${DEBIAN_VERSION}
ARG RUNNER_IMAGE=debian:${DEBIAN_VERSION}

FROM ${BUILDER_IMAGE} AS builder

RUN apt-get update -y \
  && apt-get install -y build-essential git curl ca-certificates \
  && apt-get clean \
  && rm -f /var/lib/apt/lists/*_*

ENV MIX_ENV=prod

WORKDIR /app

RUN mix local.hex --force \
  && mix local.rebar --force

COPY mix.exs mix.lock ./
RUN mix deps.get --only ${MIX_ENV}

RUN mkdir config
COPY config/config.exs config/prod.exs config/
RUN mix deps.compile

COPY assets assets
COPY priv priv
COPY lib lib
COPY config/runtime.exs config/

RUN mix assets.deploy
RUN mix compile
RUN mix release

FROM ${RUNNER_IMAGE}

RUN apt-get update -y \
  && apt-get install -y libstdc++6 openssl libncurses5 locales ca-certificates \
  && apt-get clean \
  && rm -f /var/lib/apt/lists/*_*

RUN sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen \
  && locale-gen

ENV LANG=en_US.UTF-8
ENV LANGUAGE=en_US:en
ENV LC_ALL=en_US.UTF-8
ENV MIX_ENV=prod
ENV PHX_SERVER=true
ENV PORT=4000

WORKDIR /app

RUN groupadd --gid 1000 elixir \
  && useradd --uid 1000 --gid elixir --shell /bin/bash --create-home elixir

COPY --from=builder --chown=elixir:elixir /app/_build/${MIX_ENV}/rel/xero ./

USER elixir

EXPOSE 4000

CMD ["/app/bin/xero", "start"]
