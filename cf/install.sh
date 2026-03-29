#!/bin/bash
set -e

REPO="doroved/proxik"
BINARY_NAME="proxik"
INSTALL_DIR="/usr/local/bin"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

printf "${BLUE}==>${NC} Detecting architecture...\n"

ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

if [ "$OS" != "linux" ]; then
    printf "${RED}Error:${NC} Proxik is only supported on Linux.\n"
    exit 1
fi

case $ARCH in
    x86_64)  TARGET="x86_64-unknown-linux-musl" ;;
    aarch64) TARGET="aarch64-unknown-linux-musl" ;;
    arm64)   TARGET="aarch64-unknown-linux-musl" ;;
    *)       printf "${RED}Error:${NC} Unsupported architecture: $ARCH\n"; exit 1 ;;
esac

printf "${BLUE}==>${NC} Fetching latest release info...\n"
RELEASE_JSON=$(curl -s https://api.github.com/repos/$REPO/releases/latest)
LATEST_TAG=$(echo "$RELEASE_JSON" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    printf "${RED}Error:${NC} Could not find latest release for $REPO\n"
    exit 1
fi

VERSION=${LATEST_TAG#v}
ASSET_NAME="${BINARY_NAME}-${VERSION}-${TARGET}.tar.gz"

printf "${BLUE}==>${NC} Downloading ${LATEST_TAG} for ${ARCH}...\n"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$LATEST_TAG/$ASSET_NAME"

TEMP_DIR=$(mktemp -d)
curl -L "$DOWNLOAD_URL" -o "$TEMP_DIR/$ASSET_NAME"

printf "${BLUE}==>${NC} Extracting...\n"
tar -xzf "$TEMP_DIR/$ASSET_NAME" -C "$TEMP_DIR"

if [ ! -f "$TEMP_DIR/$BINARY_NAME" ]; then
    printf "${RED}Error:${NC} Binary not found in the downloaded archive.\n"
    exit 1
fi

printf "${BLUE}==>${NC} Installing to $INSTALL_DIR (may require sudo)...\n"
if [ -w "$INSTALL_DIR" ]; then
    mv "$TEMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"
else
    sudo mv "$TEMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
    sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
fi

printf "${BLUE}==>${NC} Cleaning up...\n"
rm -rf "$TEMP_DIR"

printf "${GREEN}Successfully installed ${BINARY_NAME} ${LATEST_TAG}!${NC}\n"
printf "Run '${BINARY_NAME} --help' to get started.\n"
