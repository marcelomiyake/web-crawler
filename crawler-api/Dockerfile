FROM rust:1.98-bookworm AS builder
WORKDIR /workspace
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY crawler-api ./crawler-api
COPY crawler-worker ./crawler-worker
COPY database ./database
RUN cargo build --locked --release --package crawler-api

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /workspace/target/release/crawler-api /usr/local/bin/crawler-api
EXPOSE 8081
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/crawler-api"]
