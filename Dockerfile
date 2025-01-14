FROM rust:latest AS builder

RUN apt update && apt install binutils

WORKDIR /workspace

COPY . .

RUN cargo build --release
RUN strip target/release/conduit

FROM debian:12-slim

WORKDIR /app

RUN adduser rusty

COPY --chown=rusty:rusty keys .
COPY --chown=rusty:rusty .env .
COPY --from=builder --chown=rusty:rusty /workspace/target/release/conduit .

USER rusty

CMD ["./conduit"]