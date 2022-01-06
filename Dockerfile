FROM rust:1.57-slim-buster AS builder

WORKDIR /asakusa

COPY src /asakusa/src
COPY Cargo.toml .
COPY Cargo.lock .
COPY usage.md .
COPY templates /asakusa/templates

RUN cargo build --release

FROM debian:buster-slim

RUN apt-get -y update && apt-get --no-install-recommends -y install fontconfig ca-certificates


RUN useradd --create-home --shell /bin/bash asakusa

WORKDIR /home/asakusa

COPY --from=builder /asakusa/target/release/asakusa ./

RUN chown asakusa:asakusa asakusa
RUN chmod +x asakusa

USER asakusa:asakusa

COPY opensans.otf .


CMD ["./asakusa"]