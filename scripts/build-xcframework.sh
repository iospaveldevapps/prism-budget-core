#!/usr/bin/env bash
#
# Builds budget_coreFFI.xcframework and regenerates the Swift bindings.
#
# Pipeline:
#   1. cargo build (staticlib) for device, simulator and macOS
#   2. uniffi-bindgen in library mode extracts the FFI metadata from the
#      built archive and emits budget_core.swift + C header + modulemap
#   3. xcodebuild -create-xcframework bundles the three slices
#   4. the generated Swift file is copied into Sources/BudgetCore
#
# The resulting xcframework and Sources/BudgetCore/budget_core.swift are
# committed, so package consumers never need a Rust toolchain.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin)
GENERATED_DIR="$REPO_ROOT/generated"
XCFRAMEWORK="$REPO_ROOT/budget_coreFFI.xcframework"

for target in "${TARGETS[@]}"; do
    rustup target add "$target" >/dev/null
    echo "▸ cargo build --release --target $target"
    cargo build --release --target "$target"
done

echo "▸ uniffi-bindgen generate (library mode)"
rm -rf "$GENERATED_DIR"
cargo run --quiet --features cli --bin uniffi-bindgen -- generate \
    --library "target/aarch64-apple-ios/release/libbudget_core.a" \
    --language swift \
    --out-dir "$GENERATED_DIR"

# xcodebuild expects a headers directory containing `module.modulemap`.
HEADERS_DIR="$GENERATED_DIR/headers/budget_coreFFI"
mkdir -p "$HEADERS_DIR"
cp "$GENERATED_DIR/budget_coreFFI.h" "$HEADERS_DIR/"
cp "$GENERATED_DIR/budget_coreFFI.modulemap" "$HEADERS_DIR/module.modulemap"

echo "▸ xcodebuild -create-xcframework"
rm -rf "$XCFRAMEWORK"
xcodebuild -create-xcframework \
    -library "target/aarch64-apple-ios/release/libbudget_core.a" \
    -headers "$GENERATED_DIR/headers" \
    -library "target/aarch64-apple-ios-sim/release/libbudget_core.a" \
    -headers "$GENERATED_DIR/headers" \
    -library "target/aarch64-apple-darwin/release/libbudget_core.a" \
    -headers "$GENERATED_DIR/headers" \
    -output "$XCFRAMEWORK"

cp "$GENERATED_DIR/budget_core.swift" "$REPO_ROOT/Sources/BudgetCore/budget_core.swift"

echo "✓ budget_coreFFI.xcframework and Sources/BudgetCore/budget_core.swift are up to date"
