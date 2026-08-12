FROM rust:1.97.1-bookworm@sha256:14bc9c5966e7b3a385794b3d5389a8765668342025fbcc7b2e3d2866ac4bd8c3 AS builder

# Keep the build on the compiler already pinned by the base image. `stable` makes
# rustup contact the network to refresh a moving channel, which turns an otherwise
# reproducible Docker build into a DNS-dependent deployment step.
ENV RUSTUP_TOOLCHAIN=1.97.1
WORKDIR /source
COPY Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml ./
COPY apps ./apps
COPY crates ./crates
COPY db ./db
COPY data/cancer-research ./data/cancer-research
RUN cargo build --locked --release --bin civilization-api --bin civilization-data --bin civilization-projector --bin civilization-runner

FROM debian:bookworm-slim@sha256:abd67ffcfa541b485a3dff59865ab629aa048a6c613e639d36e7456b0b229241 AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 10001 civilization
WORKDIR /app
RUN install -d -o civilization -g civilization -m 0500 /app/qualification \
    && install -d -o civilization -g civilization -m 0555 /app/data /app/data/cancer-research
COPY --from=builder --chown=10001:10001 --chmod=0555 /source/target/release/civilization-api /app/civilization-api
COPY --from=builder --chown=10001:10001 --chmod=0555 /source/target/release/civilization-data /app/civilization-data
COPY --from=builder --chown=10001:10001 --chmod=0555 /source/target/release/civilization-projector /app/civilization-projector
COPY --from=builder --chown=10001:10001 --chmod=0555 /source/target/release/civilization-runner /app/civilization-runner
# The catalogue is deliberately prompt-safe and public. The corresponding
# answer key is never baked into the image; only the qualification worker gets
# that file through its dedicated read-only bind mount.
COPY --from=builder --chown=10001:10001 --chmod=0444 /source/data/cancer-research/nci-cellminer-2-15-cns-challenge-catalogue-v1.json /app/data/cancer-research/nci-cellminer-2-15-cns-challenge-catalogue-v1.json

USER civilization
EXPOSE 8080
CMD ["/app/civilization-api", "serve"]
