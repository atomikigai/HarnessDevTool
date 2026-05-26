# syntax=docker/dockerfile:1.7
# ---- Builder ----
FROM rust:1-alpine AS builder

RUN apk add --no-cache musl-dev pkgconfig

WORKDIR /build

# Copy workspace manifests first to cache deps
COPY Cargo.toml ./
COPY rust-toolchain.toml ./
COPY clippy.toml ./
COPY rustfmt.toml ./
COPY crates/harness-server/Cargo.toml crates/harness-server/Cargo.toml
COPY crates/harness-core/Cargo.toml crates/harness-core/Cargo.toml

# Create dummy sources for dep cache
RUN mkdir -p crates/harness-server/src crates/harness-core/src && \
    echo "fn main() {}" > crates/harness-server/src/main.rs && \
    echo "" > crates/harness-core/src/lib.rs

RUN cargo build --release --bin harness-server || true

# Now copy real sources
COPY crates ./crates

RUN touch crates/harness-server/src/main.rs crates/harness-core/src/lib.rs && \
    cargo build --release --bin harness-server

# ---- Runtime ----
FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=builder /build/target/release/harness-server /usr/local/bin/harness-server

EXPOSE 7777

ENV HARNESS_HOME=/data/.harness \
    HARNESS_BIND=0.0.0.0:7777

ENTRYPOINT ["/usr/local/bin/harness-server"]
