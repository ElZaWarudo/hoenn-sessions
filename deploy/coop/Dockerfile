FROM rust:1.93.1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY coop/crates ./coop/crates
COPY dist/bridge_manifest.json ./dist/bridge_manifest.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --locked --release -j 2 -p coop-server --bin coop-server \
    && cp target/release/coop-server /tmp/coop-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 10001 --no-create-home --shell /usr/sbin/nologin coop
COPY --from=build /tmp/coop-server /usr/local/bin/coop-server
USER 10001:10001
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/coop-server"]
