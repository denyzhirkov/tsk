#!/bin/sh
set -e

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)
case "$ARCH" in
    aarch64) ARCH="arm64" ;;
esac

BINARY_NAME="tsk-${OS}-${ARCH}"

cargo build --release
mkdir -p dist
cp target/release/tsk "dist/${BINARY_NAME}"
cp target/release/tsk dist/tsk

echo "Built: dist/${BINARY_NAME}"
