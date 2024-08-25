FROM ubuntu:22.04 AS build

RUN apt-get update && apt-get -y upgrade
RUN apt-get -y install \
    gcc \
    curl \
    libpq-dev \
    libsqlite3-dev \
    protobuf-compiler

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

COPY . /hold

WORKDIR /hold
RUN cargo build --release

FROM scratch AS binaries

COPY --from=build /hold/target/release/hold /
