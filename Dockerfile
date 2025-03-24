FROM rust:1.85.1-alpine3.21 AS builder

RUN echo "https://dl-cdn.alpinelinux.org/alpine/edge/community" >> /etc/apk/repositories \
    && apk add --no-cache --update --no-progress build-base clang clang-dev rlottie-dev libressl-dev

WORKDIR /build

COPY ./ .

RUN set -ex \
    && cd /build \
    && RUSTFLAGS="-C target-feature=-crt-static" cargo build --release

FROM alpine:3.21

RUN echo "https://dl-cdn.alpinelinux.org/alpine/edge/community" >> /etc/apk/repositories \
    && apk add --no-cache --update --no-progress libressl4.0-libssl rlottie ffmpeg tzdata \
    && cp /usr/share/zoneinfo/Asia/Shanghai /etc/localtime \
    && echo "Asia/Shanghai" > /etc/timezone
#&& apk del --quiet --no-progress tzdata

COPY --from=builder /build/target/release/teleporter /usr/bin/teleporter

WORKDIR /data

ENTRYPOINT [ "/usr/bin/teleporter" ]
