# Сборка отдельным слоем: в финальный образ едет один бинарник.
FROM rust:1-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends nodejs npm gzip \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN sh build.sh

# Ассеты и шрифты вшиты внутрь, поэтому рантайму не нужно ничего, кроме libc.
FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/uttt /usr/local/bin/uttt
ENV PORT=80 DATA_DIR=/data
EXPOSE 80
# Процесс работает на переднем плане: иначе Amvera считает сборку зависшей.
CMD ["/usr/local/bin/uttt"]
