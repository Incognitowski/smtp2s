# ---- Build stage ----
FROM rust:slim AS builder

# Install musl toolchain
RUN apt-get update && apt-get install -y musl-tools && rm -rf /var/lib/apt/lists/*

# Add musl target to rustup
RUN rustup target add x86_64-unknown-linux-musl

WORKDIR /usr/src/app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build static binary
RUN cargo build --release --target x86_64-unknown-linux-musl

# ---- Runtime stage ----
FROM scratch

# Copy statically linked binary
COPY --from=builder /usr/src/app/target/x86_64-unknown-linux-musl/release/smtp2s /smtp2s

# Mount points for logs and configs
VOLUME ["/logs", "/configs"]

EXPOSE 8080 9090

ENTRYPOINT ["/smtp2s"]
CMD ["--help"]
