#!/bin/bash
# Build with SIMD enabled for auto-vectorization
RUSTFLAGS="-C target-feature=+simd128" wasm-pack build --target web --release
echo "Build complete. Serve with: python -m http.server 8080"
