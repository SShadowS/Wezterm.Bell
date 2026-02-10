#!/bin/bash
set -e

DEPLOY_DIR="$USERPROFILE/.claude/bell"

echo "Building release..."
cargo build --release

echo "Copying files to $DEPLOY_DIR..."
mkdir -p "$DEPLOY_DIR/src"
cp Cargo.toml "$DEPLOY_DIR/Cargo.toml"
cp Cargo.lock "$DEPLOY_DIR/Cargo.lock"
cp src/main.rs "$DEPLOY_DIR/src/main.rs"

echo "Rebuilding in deploy directory..."
cd "$DEPLOY_DIR"
cargo build --release

echo "Done. Deployed bell.exe at $DEPLOY_DIR/target/release/bell.exe"
