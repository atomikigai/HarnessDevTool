FROM rust:1.95-slim AS builder

RUN cargo install --locked pdf_oxide_mcp

FROM debian:bookworm-slim

COPY --from=builder /usr/local/cargo/bin/pdf-oxide-mcp /usr/local/bin/pdf-oxide-mcp

ENTRYPOINT ["pdf-oxide-mcp"]
