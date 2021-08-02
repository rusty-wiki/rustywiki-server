FROM rust:latest as build

ENV TZ=Asia/Seoul
ENV HOME=/home/root

ADD . /rustywiki-server
WORKDIR /rustywiki-server
RUN apt-get update -y
RUN apt-get upgrade -y
RUN cargo build --release

ENTRYPOINT ["/rustywiki-server/target/release/rustywiki"]
