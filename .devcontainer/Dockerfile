FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Process fixtures use /usr/bin/python3 explicitly. No Docker daemon/socket is
# needed for development: all default tests run against disposable doubles.
USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 \
    && rm -rf /var/lib/apt/lists/*
USER vscode
