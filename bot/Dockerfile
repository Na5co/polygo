# polygo-bot: build the binary, then ship it on a base with git and certificates, which
# is all it needs at runtime.
#   docker build -f bot/Dockerfile -t polygo-bot .
FROM rust:1-slim AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p polygo-bot

FROM debian:stable-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/polygo-bot /usr/local/bin/polygo-bot
# The App's own settings arrive as environment variables; nothing is baked in.
ENV PORT=8080
EXPOSE 8080
USER nobody
ENTRYPOINT ["/usr/local/bin/polygo-bot"]
