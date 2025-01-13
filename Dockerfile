FROM rust:latest AS builder

WORKDIR /workspace

COPY . .

RUN cargo run --release

FROM alpine:latest

WORKDIR /app

COPY --from=builder --chown=rusty:rusty /workspace/target/release/conduit .

RUN adduser -D rusty
USER rusty

CMD ["./conduit"]