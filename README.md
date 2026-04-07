# universe-wasm 🌌⚙️

**Rust/WASM Barnes-Hut N-body simulator with QJL force caching — now with Moiré Parallax Engine.**

The native benchmark for the STCP unified theory, and the cognitive visual substrate for Baby Zero.

---

## What Is This

Two physics engines in one WASM binary, both built on the same core principle: **quantized spatial leverage**.

### Engine 1 — Barnes-Hut N-body Simulator

A full 3D gravitational N-body simulation using an arena-based octree. Three simulation modes:

| Mode | Description |
|------|-------------|
| `0` — Exact | Direct Barnes-Hut traversal. No approximation. |
| `1` — QJL | Quantized Joint Leverage: spherical force quantization. |
| `2` — QJL+Cache | QJL + integer-keyed HashMap cache. Maximum speed. |

The QJL mode quantizes force vectors into spherical buckets (radius × theta × phi), keyed by a packed `u64`. Cache hit rates consistently exceed 85% on galaxy formations.

**Zero-copy JS bridge:**
```js
const universe = new Universe(5000);
universe.init_galaxy();
const ptr = universe.particles_ptr();
const buf = new Float32Array(wasm.memory.buffer, ptr, universe.buffer_len());
// buf is a live view — no copies, no GC pressure
universe.step(); // buf updates in-place
```

### Engine 2 — Moiré Parallax Engine

**New in v0.2.0** — A physics-based interference pattern generator for cognitive visual learning.

Instead of Perlin noise, Baby Zero's visual substrate is now driven by real physics: two high-density offset particle grids create a dual-layer translucent parallax mesh. The QJL inverse-square falloff principle is repurposed as a **spatial lens** — computing pixel-level displacement of background nodes based on foreground particle density and motion.

The resulting displacement map IS the moiré interference pattern. It's not rendered — it's physics.

```
[Foreground Grid]    [Background Grid]
       ↓ QJL inverse-square falloff
  [Displacement Map — the spatial lens]
       ↓ 15-20% alpha blend
  [Moiré Canvas]
       ↓ WASM-to-JS bridge
  [Baby Zero Shadow Canvas → .geo_parallax_warp]
```

**Zero-copy JS bridge:**
```js
const moire = new MoireEngine(
  32, 32,   // grid_w, grid_h
  1.5,      // fg_density (particles per node)
  0.3, 0.3, // offset_x, offset_y (creates interference)
  0.2       // influence_radius (THETA analog)
);

moire.tick(0.016); // advance one frame

const ptr = moire.displacement_ptr();
const displace = new Float32Array(wasm.memory.buffer, ptr, moire.displacement_len());
// displace[i*2] = dx, displace[i*2+1] = dy for node i

// GEO grammar bridge — Baby Zero reads this
const quadrant = moire.get_dominant_warp_region(); // 0=TL, 1=TR, 2=BL, 3=BR
const energy   = moire.get_interference_energy();  // 0.0–1.0
```

---

## The GEO Grammar Bridge

The key insight: `get_dominant_warp_region()` returns a GEO quadtree address. Baby Zero's `.geo_parallax_warp` grammar rule reads this value and routes its attention to the quadrant with the highest physical interference.

Physical reality → pixel displacement → quadtree address → cognitive pattern cache.

The math and the mind converge at the same geometry.

---

## Architecture

```
universe-wasm/
├── src/lib.rs       — Barnes-Hut Universe (wasm_bindgen) + Moiré Engine
├── pkg/             — Compiled WASM + JS bindings (wasm-pack output)
├── Cargo.toml       — universe-wasm v0.2.0, js-sys + wasm-bindgen
└── LICENSE          — MIT
```

**Rust structs:**
- `Universe` — N-body simulation, 7-stride flat buffer `[x,y,z,vx,vy,vz,mass]`
- `MoireEngine` — Dual-grid interference, 5-stride fg `[x,y,vx,vy,influence]` + 2-stride displacement
- `morton_key_3d()` — Z-order curve encoding for spatial indexing

---

## Building

```bash
# Install wasm-pack if you don't have it
cargo install wasm-pack

# Build optimized WASM
wasm-pack build --target web --release

# Output goes to pkg/
```

The release profile uses `opt-level=3, lto=true, codegen-units=1, panic=abort` — maximum size reduction.

---

## STCP / QJL Theory

QJL (Quantized Joint Leverage) is the force approximation scheme at the heart of both engines:

**In N-body:** Forces are quantized into spherical buckets. Two particles with similar relative positions get the same cached force vector. Cache key = `node_id(34 bits) | radius(10) | theta(10) | phi(10)`.

**In Moiré:** The same inverse-square falloff law governs how foreground particles displace background nodes. `falloff = 1.0 / (dist² + 0.01)`. The THETA radius threshold (`influence_radius`) acts exactly like Barnes-Hut's THETA criterion — particles beyond it are ignored.

Same math. Same physics. Different domains. One WASM binary.

---

## Part of Momentum Lab

This engine is the WASM backbone of [Momentum Lab](https://github.com/sfdimarco/Momentum-Lab) — VS Code for babies. The Moiré Parallax Engine feeds Baby Zero's shadow canvas, giving the spatial AI agent a physically-grounded visual noise substrate to explore and cache as GEO grammar patterns.

---

**Built by Mook.** MIT License.
