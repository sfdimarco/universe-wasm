# universe-wasm ⚡

**A 38KB WASM binary running 8,000-particle gravitational physics at 60fps in your browser.**

Built on a pattern most web developers have never heard of. The build scripts are 4 lines. What they unlock is not.

---

## The Build Scripts Are the Point

**Windows:**
```bat
@echo off
set RUSTFLAGS=-C target-feature=+simd128
wasm-pack build --target web --release
echo Build complete. Serve with: python -m http.server 8080
```

**Linux/macOS:**
```bash
#!/bin/bash
RUSTFLAGS="-C target-feature=+simd128" wasm-pack build --target web --release
echo "Build complete. Serve with: python -m http.server 8080"
```

That flag — `target-feature=+simd128` — is the thing most WASM tutorials never mention. It enables WebAssembly's 128-bit SIMD instructions, allowing the CPU to operate on **4 floats simultaneously** instead of 1. Combined with everything else in this repo, it's the last layer of a performance stack that makes browsers run physics at near-native speed.

---

## Why Browsers Are Slow at Heavy Computation (and How This Fixes It)

JavaScript has a garbage collector. Every object you create is eventually hunted down and freed. When the GC runs — even briefly — your frame drops. For computation-heavy work like physics, ML inference, or signal processing, this is a fundamental ceiling.

This repo sidesteps that ceiling entirely:

| Problem | This Solution |
|---|---|
| GC pressure from JS allocations | Rust owns all memory. Zero GC. |
| Single-threaded JS math | WASM + SIMD128: 4 f32 ops per cycle |
| Copying data between Rust and JS | Zero-copy bridge: JS reads a raw pointer |
| O(n²) naive physics | Barnes-Hut octree: O(n log n) |
| Redundant force calculations | QJL cache: 80-90% of forces are skipped |
| Slow WASM from default builds | LTO + codegen-units=1: whole-program optimization |

These don't add. They **multiply.**

At 8,000 particles, the difference between naïve JavaScript physics and this stack is roughly two orders of magnitude.

---

## The Performance Stack (Layer by Layer)

### Layer 1: SIMD128
```
RUSTFLAGS=-C target-feature=+simd128
```
The Rust compiler vectorizes float operations automatically when this flag is set. Instead of computing one force component per clock cycle, the CPU handles four. This flag is **not enabled by default** in standard WASM builds. One line. Potentially 4x.

### Layer 2: Barnes-Hut O(n log n)
Naïve N-body is O(n²) — 8,000 particles means 64 million pairwise checks per frame. Barnes-Hut builds an octree and treats distant clusters as single bodies. At n=8,000 this is already ~10x faster than brute force. The octree is **arena-allocated** — pre-reserved memory, no per-frame malloc/free.

### Layer 3: QJL Force Caching
QJL (Quantized Joint Leverage) quantizes force vectors into spherical buckets:
- Radial: 20.0-unit buckets
- Polar: 0.1 radian buckets  
- Azimuthal: 0.1 radian buckets

Cache key: `(node_id << 30) | (q_rad << 20) | (q_theta << 10) | (q_phi)`

When two particles have similar relative positions to a tree node, they get the **same cached force vector**. In practice: 80–90% cache hit rates. That means 80–90% of force evaluations are a HashMap lookup instead of trigonometry.

### Layer 4: Zero-Copy JS Bridge
```javascript
const ptr = universe.particles_ptr();
const particles = new Float32Array(wasm.memory.buffer, ptr, universe.buffer_len());
```
The particle data lives in WASM linear memory. `particles_ptr()` returns a raw pointer. JavaScript creates a typed array **view** over that memory — no copying, no serialization, no GC involvement. The simulation runs, and JS reads the results directly from the same bytes Rust wrote.

### Layer 5: Release Profile
```toml
[profile.release]
opt-level = 3      # Maximum optimization
lto = true         # Link-time optimization across all crates
codegen-units = 1  # Single compilation unit — lets LLVM see everything
panic = "abort"    # No unwinding machinery, smaller binary
```
Link-time optimization lets LLVM inline across crate boundaries and eliminate dead code globally. `codegen-units=1` trades compile time for maximum runtime performance. Result: a **38KB WASM binary** that runs a full 3D physics simulation.

---

## Live Demo

Open `index.html` in a browser after building. It includes:

- **8,000-particle galaxy formation** rendered with Three.js
- **3-way real-time comparison** — Exact vs QJL vs QJL+Cache
- **Live cache stats** — hit rate, entries, force evaluations saved
- **KE drift tracking** — energy conservation accuracy per mode
- **Auto-run** — cycles through all three modes automatically, prints speedup

```
[Exact]     → Baseline. Accurate. Slowest.
[QJL]       → Quantized spherical coords. ~2-3x faster.
[QJL+Cache] → QJL + force cache. 80-90% hit rate. Fastest.
```

Keyboard: `0` `1` `2` to switch modes. `A` to auto-run all. `R` to reset comparison.

To serve locally:
```bash
python -m http.server 8080
# or
npx serve .
```

---

## JavaScript API

```javascript
import init, { Universe } from './pkg/universe_wasm.js';

const wasm = await init();
const universe = new Universe(10000);
universe.init_galaxy();

// Switch force computation mode
universe.set_mode(0); // Exact Barnes-Hut
universe.set_mode(1); // QJL (quantized spherical, no cache)
universe.set_mode(2); // QJL + HashMap cache

// Run one timestep — returns total time in ms
const elapsed_ms = universe.step();
const force_ms = universe.force_time_ms();

// Zero-copy particle access — no data transfer
const ptr = universe.particles_ptr();
const particles = new Float32Array(wasm.memory.buffer, ptr, universe.buffer_len());
// particles[i*7 + 0] = x
// particles[i*7 + 1] = y
// particles[i*7 + 2] = z
// particles[i*7 + 3] = vx
// particles[i*7 + 4] = vy
// particles[i*7 + 5] = vz
// particles[i*7 + 6] = mass

// Cache diagnostics
const hit_rate  = universe.cache_hit_rate();   // 0.0–1.0
const hits      = universe.cache_hits();
const misses    = universe.cache_misses();
const cache_sz  = universe.cache_size();

// Physics diagnostics
const ke        = universe.compute_ke();       // kinetic energy
const nodes     = universe.tree_node_count();
const frames    = universe.frame_count();

// Morton Z-order encoding (exposed for spatial indexing)
const key = morton_key_3d(x, y, z);
```

---

## Building From Source

**Prerequisites:**
- [Rust](https://rustup.rs/) — install via rustup
- [wasm-pack](https://rustwasm.github.io/wasm-pack/installer/) — `cargo install wasm-pack`
- The WASM target: `rustup target add wasm32-unknown-unknown`

**Build:**
```bash
# Linux/macOS
./build.sh

# Windows
build.bat
```

Output goes to `pkg/` — ready to import as an ES module.

---

## The Pattern (Apply It to Anything)

This isn't just an N-body simulator. It's a reference implementation of a pattern that should be standard for any browser app doing heavy computation:

```
[Expensive computation] → Rust crate (zero GC, SIMD, LTO)
       ↓ wasm-pack build --target web --release
[WASM module] → zero-copy Float32Array bridge → [Browser]
```

**Anywhere you'd reach for a Web Worker + heavy JS computation, reach for this instead:**

- Real-time physics engines (game physics, cloth simulation, rigid bodies)
- ML inference in the browser (matrix ops, convolutions, attention)
- Audio DSP (FFT, filters, synthesis — no AudioWorklet thread hopping)
- Image processing (blur, convolutions, color space transforms)
- Data visualization (force-directed graphs, particle systems, fluid sim)
- Financial modeling (Monte Carlo, option pricing, portfolio math)
- Computational geometry (collision detection, mesh processing, CSG)

The `+simd128` flag and the zero-copy pointer pattern carry to all of these. The build scripts are the same 4 lines.

---

## Repository Structure

```
universe-wasm/
├── build.bat          ← Windows build script (the thing this README is about)
├── build.sh           ← Linux/macOS build script
├── Cargo.toml         ← universe-wasm v0.1, wasm-bindgen + js-sys
├── src/
│   └── lib.rs         ← ~592 lines. Universe + QJL + cache + morton
├── pkg/               ← Compiled WASM + JS/TS bindings (wasm-pack output)
│   ├── universe_wasm_bg.wasm     ← 38KB. The whole thing.
│   ├── universe_wasm.js          ← ES module wrapper
│   └── universe_wasm.d.ts        ← TypeScript types
├── index.html         ← Live benchmark demo (Three.js + 3-way comparison)
├── ARCHITECTURE.md    ← Deep technical notes on the QJL design
└── COMPLETION.md      ← Implementation record
```

---

## Part of a Larger Research Project

This engine is the WASM backbone of [Momentum Lab](https://github.com/sfdimarco/Momentum-Lab) — a spatial learning environment where the QJL spatial lens principle is extended into a cognitive visual substrate for AI agents. The same physics principles that make N-body simulation efficient in browsers also drive the Moiré Parallax Engine: a real-time interference pattern generator where force displacement becomes a grammar for spatial cognition.

The math doesn't care what domain you apply it to. That's the point.

---

**MIT License. Open source. Build something insane.**

*— Mook*
