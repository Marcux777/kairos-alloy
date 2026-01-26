FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

# Base tooling + build deps + utilities used in this environment
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    wget \
    debianutils \
    git \
    build-essential \
    pkg-config \
    libssl-dev \
    python3 \
    python3-pip \
    python3-venv \
    unzip \
    zip \
    jq \
    ripgrep \
    poppler-utils \
    && rm -rf /var/lib/apt/lists/*

# Node.js (for Codex CLI)
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get update \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Codex CLI (npm)
RUN npm install -g @openai/codex

# Create Codex home and entrypoint to optionally sync host config
RUN mkdir -p /root/.codex
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

WORKDIR /workspaces/kairos-alloy

# Use a volume mount to bring your local Codex config into the container:
#   docker run -it -v ~/.codex:/codex-config -v $(pwd):/workspaces/kairos-alloy kairos-alloy-dev
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["bash"]
