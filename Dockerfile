# Build the binary, then ship it on a slim image with git + CA certs at runtime
# (the tool shells out to `git` and clones over HTTPS).
FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked && strip target/release/contributor-graphs

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/contributor-graphs /usr/local/bin/contributor-graphs
# Outputs are written here; mount your own directory onto it.
WORKDIR /work
ENTRYPOINT ["contributor-graphs"]
CMD ["--help"]
