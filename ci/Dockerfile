FROM node:20-bookworm-slim

ARG SCCACHE_VERSION=0.10.0
ARG SCCACHE_TARGET=x86_64-unknown-linux-musl

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates \
      curl \
      git \
      make \
      build-essential \
      pkg-config \
 && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH \
    RUST_VERSION=stable \
    SCCACHE_VERSION=${SCCACHE_VERSION}

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
 | sh -s -- -y --profile minimal --default-toolchain $RUST_VERSION \
 && rustup default $RUST_VERSION \
 && chmod -R a+w $RUSTUP_HOME $CARGO_HOME

# sccache — installed as a sidecar binary next to rustc so the CI job can wrap
# `rustc` via `RUSTC_WRAPPER=sccache`. Pinned to a release tag; the musl build
# keeps the image slim (no glibc dependency on top of bookworm-slim). The
# backend credentials (SCCACHE_BUCKET / SCCACHE_ENDPOINT / AWS_*) are injected
# per-job from workflow secrets — see .github/workflows/plan.yml.
#
# Integrity: sccache publishes a `.sha256` alongside each release asset. We
# fetch both the tarball and its sha256, verify, then extract. This catches
# CDN/MITM tampering and re-uploaded-asset substitution (M160 F-05).
ARG SCCACHE_SHA256=1fbb35e135660d04a2d5e42b59c7874d39b3deb17de56330b25b713ec59f849b
RUN curl --proto '=https' --tlsv1.2 -sSfL \
      "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-${SCCACHE_TARGET}.tar.gz" \
      -o /tmp/sccache.tar.gz \
 && curl --proto '=https' --tlsv1.2 -sSfL \
      "https://github.com/mozilla/sccache/releases/download/v${SCCACHE_VERSION}/sccache-v${SCCACHE_VERSION}-${SCCACHE_TARGET}.tar.gz.sha256" \
      -o /tmp/sccache.tar.gz.sha256 \
 && cd /tmp \
 && sha256sum -c sccache.tar.gz.sha256 \
 && mkdir -p sccache-extract \
 && tar -xz -C sccache-extract --strip-components=1 -f sccache.tar.gz \
 && install -m 0755 sccache-extract/sccache /usr/local/bin/sccache \
 && rm -rf /tmp/sccache.tar.gz /tmp/sccache.tar.gz.sha256 /tmp/sccache-extract \
 && sccache --version

WORKDIR /workspace
