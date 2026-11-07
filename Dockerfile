# ---- build ----
FROM rust:1-slim-bookworm AS build
WORKDIR /app
# cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && echo "" > src/lib.rs \
    && cargo build --release && rm -rf src
COPY migrations ./migrations
COPY src ./src
RUN touch src/main.rs src/lib.rs && cargo build --release

# ---- runtime ----
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 app \
    && mkdir -p /data/slips && chown app /data/slips
COPY --from=build /app/target/release/h-ticket-backend /usr/local/bin/h-ticket-backend
USER app
ENV PORT=8080 SLIP_DIR=/data/slips
EXPOSE 8080
CMD ["h-ticket-backend"]
