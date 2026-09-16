# Build (single arch, local): docker build -t rctproxy .
# Build (multi-arch): docker buildx build --platform linux/amd64,linux/arm64/v8 -t rctproxy --load .
# Run: docker run --rm -p 18899:18899 rctproxy --port 18899 --host <inverter-ip>

FROM rust:1-slim AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY data ./data
COPY src ./src
COPY examples ./examples
RUN cargo build --release --features cli --example rct_proxy

FROM debian:bookworm-slim
RUN useradd -r -u 65532 rctproxy
COPY --from=builder /src/target/release/examples/rct_proxy /usr/local/bin/rct_proxy
USER rctproxy
EXPOSE 8899
ENTRYPOINT ["/usr/local/bin/rct_proxy"]
CMD ["--port", "8899"]
