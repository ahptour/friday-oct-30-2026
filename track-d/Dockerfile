FROM python:3.13-slim@sha256:d98cc29d3266e535dd9da1099688d9cb3ca4753813571e071763d17725460962 AS training
COPY generators/d_traverse.py /generator.py
RUN set -eu; for seed in 7 19 103; do \
    python3 /generator.py --seed "$seed" --scale sample --out "/training/$seed"; \
    done

FROM halloumee/speed-demon-rust:1.99.1-trixie-llvm-tools@sha256:13e4ee7135d8eac6c56476c7d1aee201f36479f4745fd5d0e6b4529f4e13251a AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY build.sh ./
COPY --from=training /training /training
RUN sh build.sh

FROM debian:13-slim@sha256:62bd423aeee912639ca8ee681c2b762cb3e25137e0a92b20be455898a9f034ad
COPY --from=build /src/target/release/solution /app/solution
USER 65534:65534
ENTRYPOINT ["/app/solution"]
