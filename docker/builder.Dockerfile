# Multi stage docker build using cargo chef.
# https://github.com/LukeMathWalker/cargo-chef
# https://lpalmieri.com/posts/fast-rust-docker-builds/
# the whole idea is to build dependencies in a separate stage and let docker cache them
# so that we don't have to recompile all dependencies on every code change.

# Stage 1: Builder. The base image's glibc is the oldest the library can be
# loaded with, so it is the support floor: AlmaLinux 9 has glibc 2.34, the
# oldest of the supported distros (RHEL 9, Amazon Linux 2023; Ubuntu 22.04
# has 2.35, Debian 13 2.41). Built natively for amd64 and arm64.
FROM almalinux:9 AS chef

RUN dnf install -y --setopt=install_weak_deps=False gcc git \
    && dnf clean all

# Install rustup only; the toolchain itself is the one pinned in
# rust-toolchain.toml, so the release library is built with the same compiler
# CI lints with. A toolchain bump invalidates this layer and everything below.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /app
COPY rust-toolchain.toml ./
RUN rustup toolchain install
RUN cargo install cargo-chef@0.1.78 --locked


# Stage 2: Planner
FROM chef AS planner
COPY Cargo.toml Cargo.lock build.rs ./
COPY crates ./crates
COPY vendor ./vendor
COPY src ./src
COPY examples ./examples
RUN cargo chef prepare --recipe-path recipe.json


# Stage 3: Run final Build
FROM chef AS flyline-builder
COPY --from=planner /app/recipe.json recipe.json
# The vendored forks are path dependencies outside the workspace, which the
# recipe does not skeletonize; cook builds them with the other dependencies.
COPY vendor ./vendor
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock build.rs ./
COPY crates ./crates
COPY src ./src
COPY examples ./examples
COPY tests ./tests
RUN cargo build --release
# Guard the floor: no symbol may need a glibc newer than the base image's.
RUN newest="$(objdump -T target/release/libflyline.so | grep -o 'GLIBC_2\.[0-9.]*' | sort -uV | tail -1)" \
    && echo "newest glibc symbol version: $newest" \
    && [ "$(printf '%s\nGLIBC_2.34\n' "$newest" | sort -V | tail -1)" = "GLIBC_2.34" ]

FROM flyline-builder AS flyline-lib-tests
RUN cargo test --release --lib


# Build image with output. This won't have anything in the file system apart from the built library
# this makes it convenient to copy the built library without creating a container
FROM scratch AS flyline-built-artifact
COPY --from=flyline-builder /app/target/release/libflyline.so /libflyline.so
