# syntax=docker/dockerfile:1
########## build stage ##########
FROM rust:1-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates make cmake pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# icann-rdap revision to build (public repo; https so no SSH keys are needed).
# The CI workflow resolves the live `dev` HEAD SHA and passes it here, so each
# image is built from current `dev` AND the Docker layer cache keys on the SHA
# (a plain `git clone --branch dev` layer would silently reuse a stale snapshot).
# The default below is a known-good pinned commit for local `docker build`;
# override with --build-arg ICANN_RDAP_REF=<sha-or-branch>.
ARG ICANN_RDAP_REF=1ab10f0a63a3842cc7cf6d3a51ca0afe2333542e
RUN git init src \
    && git -C src remote add origin https://github.com/icann/icann-rdap.git \
    && git -C src fetch --depth 1 origin "${ICANN_RDAP_REF}" \
    && git -C src checkout FETCH_HEAD

WORKDIR /build/src

# build only the rdap-srv binary
RUN --mount=type=cache,target=/build/src/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    cargo build --release -p icann-rdap-srv --bin rdap-srv \
    && cp target/release/rdap-srv /rdap-srv

########## runtime stage ##########
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 rdap

COPY --from=builder /rdap-srv /usr/local/bin/rdap-srv

# Bake the synthesized redirect templates + help listing (committed under ./data)
# into the data directory. COPY --chown creates /data owned by the runtime user;
# VOLUME after COPY captures the baked content so a fresh container with no bind
# mount still serves it. At startup rdap-srv writes its own bootstrap_* / cache
# files alongside these (different prefixes, no clash).
COPY --chown=rdap:rdap data/ /data/
VOLUME ["/data"]

# run as a bootstrap server
ENV RDAP_SRV_BOOTSTRAP=true \
    RDAP_SRV_DATA_DIR=/data \
    RDAP_SRV_STORAGE=memory \
    RDAP_SRV_LISTEN_ADDR=0.0.0.0 \
    RDAP_SRV_LISTEN_PORT=3000 \
    RDAP_SRV_LOG=info \
    RDAP_SRV_ACCESS_LOG=true \
    RDAP_SRV_TLD_BOOTSTRAP=true \
    RDAP_SRV_INR_BACKUP_BOOTSTRAP=true \
    RDAP_SRV_BOOTSTRAP_EXEMPT_OBJECT_TAGS=RCODE3 \
    RDAP_SRV_IP_RDAP_UP=true \
    RDAP_SRV_IP_RDAP_DOWN=true \
    RDAP_SRV_IP_RDAP_TOP=true \
    RDAP_SRV_IP_RDAP_BOTTOM=true \
    RDAP_SRV_AUTNUM_RDAP_TOP=true \
    RDAP_SRV_AUTNUM_RDAP_UP=true \
    RDAP_SRV_AUTNUM_RDAP_DOWN=true \
    RDAP_SRV_AUTNUM_RDAP_BOTTOM=true \
    RDAP_SRV_DOMAIN_RDAP_UP=true \
    RDAP_SRV_DOMAIN_RDAP_DOWN=true \
    RDAP_SRV_DOMAIN_RDAP_TOP=true \
    RDAP_SRV_DOMAIN_RDAP_BOTTOM=true

USER rdap
EXPOSE 3000

CMD ["rdap-srv"]
