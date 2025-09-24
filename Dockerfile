# Build stage
FROM rust:1.88.0-slim AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
  pkg-config \
  libssl-dev \
  && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy source code and dependencies
COPY ai-vitals/Cargo.toml ai-vitals/Cargo.lock ./
COPY ai-vitals/src ./src

# Build the application
RUN cargo build --release

FROM node
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

# Install newman globally
RUN npm install -g newman

# Copy the binary from builder stage
COPY --from=builder /app/target/release/ai-vitals /app/ai-vitals

# Set working directory
WORKDIR /app

# Run the application
ENTRYPOINT ["./ai-vitals"]
CMD []