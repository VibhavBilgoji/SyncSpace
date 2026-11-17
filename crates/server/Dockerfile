# The API server image. Build from the repository root:
#   docker build -f crates/server/Dockerfile -t syncspace-api .
# The Rust version is the one pinned in rust-toolchain.toml and ci.yml (bump them together).
# rust-toolchain.toml itself is removed in the build stages: it also asks for rustfmt, clippy,
# llvm-tools and the wasm32 target, none of which an image build needs.
FROM rust:1.100.0-bookworm AS chef
RUN cargo install cargo-chef --locked --version 0.1.78
WORKDIR /app

FROM chef AS planner
COPY . .
RUN rm -f rust-toolchain.toml && cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
# .cargo/config.toml limits the laptop to two jobs; a build machine can use all its cores.
ENV CARGO_BUILD_JOBS=4
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p server
COPY . .
RUN rm -f rust-toolchain.toml && cargo build --release -p server --bin server

FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /app/target/release/server /server
ENV RUST_LOG=info
# Render sets PORT (10000 by default); the server binds 0.0.0.0:$PORT.
EXPOSE 10000
USER nonroot
ENTRYPOINT ["/server"]
