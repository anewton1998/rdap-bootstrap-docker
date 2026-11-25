# syntax=docker/dockerfile:1
########## build stage ##########
FROM rust:1-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates make cmake pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# clone the dev branch (public repo; use https so no SSH keys are needed at build time)
RUN git clone --depth 1 --branch dev https://github.com/icann/icann-rdap.git src

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

# data directory used for the IANA bootstrap cache
RUN mkdir -p /data && chown rdap:rdap /data
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
