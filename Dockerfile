FROM ubuntu:22.04

ARG KAIROS_UID=1000
ARG KAIROS_GID=1000
ARG KAIROS_USER=kairos

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
    postgresql-client \
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

# Codex CLI (npm)
RUN npm install -g @openai/codex

# Create a non-root user to avoid root-owned files on bind mounts.
RUN groupadd -g "${KAIROS_GID}" "${KAIROS_USER}" \
    && useradd -m -u "${KAIROS_UID}" -g "${KAIROS_GID}" -s /bin/bash "${KAIROS_USER}"

# Create Codex home and entrypoint to optionally sync host config.
ENV HOME="/home/${KAIROS_USER}"
RUN mkdir -p "${HOME}/.codex" && chown -R "${KAIROS_UID}:${KAIROS_GID}" "${HOME}/.codex"
COPY docker/entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

USER ${KAIROS_USER}

# Rust toolchain (installed in the non-root user's home).
ENV CARGO_HOME="${HOME}/.cargo" \
    RUSTUP_HOME="${HOME}/.rustup" \
    PATH="${HOME}/.cargo/bin:${PATH}"
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain 1.93.0 \
    && rustup component add rustfmt clippy

WORKDIR /workspaces/kairos-alloy

# Use a volume mount to bring your local Codex config into the container:
#   docker run -it -v ~/.codex:/codex-config -v $(pwd):/workspaces/kairos-alloy kairos-alloy-dev
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["bash"]
