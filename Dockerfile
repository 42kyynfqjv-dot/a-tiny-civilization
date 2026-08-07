FROM rust:1.97.1-bookworm AS builder

# Keep the build on the compiler already pinned by the base image. `stable` makes
# rustup contact the network to refresh a moving channel, which turns an otherwise
# reproducible Docker build into a DNS-dependent deployment step.
ENV RUSTUP_TOOLCHAIN=1.97.1
WORKDIR /source
COPY Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml ./
COPY apps ./apps
COPY crates ./crates
COPY db ./db
RUN cargo build --locked --release --bin civilization-api --bin civilization-data --bin civilization-projector --bin civilization-runner

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 10001 civilization
WORKDIR /app
COPY --from=builder /source/target/release/civilization-api /app/civilization-api
COPY --from=builder /source/target/release/civilization-data /app/civilization-data
COPY --from=builder /source/target/release/civilization-projector /app/civilization-projector
COPY --from=builder /source/target/release/civilization-runner /app/civilization-runner

USER civilization
EXPOSE 8080
CMD ["/app/civilization-api", "serve"]
