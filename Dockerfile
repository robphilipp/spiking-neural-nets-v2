# syntax=docker/dockerfile:1
#
# spikes-server with the web UI, as one image. The same image runs a node agent (`--node`).
#
#   docker build -t spikes .
#   echo -n "$(openssl rand -hex 16)" > token
#   docker run --rm -p 7070:7070 -v "$PWD/token:/run/secrets/spikes-token:ro" -v spikes-data:/data spikes
#
# Clusters: node agents and the coordinator open connections to each other on ports chosen at build time, so run
# them with host networking (`--network host`); see docs/cluster.md and docker-compose.cluster.yml.

# ----- the web UI -----
FROM node:22-bookworm-slim AS ui
WORKDIR /src
COPY package.json package-lock.json ./
COPY packages/spikes-client/package.json packages/spikes-client/
COPY apps/spikes-ui/package.json apps/spikes-ui/
RUN npm ci --ignore-scripts
COPY packages packages
COPY apps/spikes-ui apps/spikes-ui
# The UI bundles the example networks.
COPY test-data test-data
RUN npm run build -w @spikes/ui

# ----- the server -----
FROM rust:1.97-bookworm AS server
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p spikes-server \
    && cp target/release/spikes-server /usr/local/bin/spikes-server

# ----- the image -----
FROM debian:bookworm-slim
RUN useradd --system --create-home --home-dir /data spikes
COPY --from=server /usr/local/bin/spikes-server /usr/local/bin/spikes-server
COPY --from=ui /src/apps/spikes-ui/dist /opt/spikes/ui
USER spikes
WORKDIR /data
VOLUME /data
# The API and UI; a node agent listens on 7400 by convention.
EXPOSE 7070 7400
ENTRYPOINT ["spikes-server"]
# Listening beyond this machine needs an access token (mount it as a secret).
CMD ["--listen", "0.0.0.0:7070", "--ui", "/opt/spikes/ui", "--recordings", "/data/recordings", "--token-file", "/run/secrets/spikes-token"]
