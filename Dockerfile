FROM rust:bookworm AS build

WORKDIR /app

COPY . .

RUN cargo build --release -p rustweb --bin rustweb -p tcps --bin tcps -p uc --bin uc

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates openssl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=build /app/target/release/rustweb /usr/local/bin/rustweb
COPY --from=build /app/target/release/tcps /usr/local/bin/tcps
COPY --from=build /app/target/release/uc /usr/local/bin/uc
COPY site/site.rweb /app/site.rweb
COPY tcps/tcps.container.json /app/tcps.json

EXPOSE 3000

# Put this behind a TLS reverse proxy in production. The local Byrust CA is
# intended for development; use --tls-cert/--tls-key for a real certificate.
CMD ["rustweb", "/app/site.rweb", "--host", "0.0.0.0", "--port", "3000", "--allow-http"]
