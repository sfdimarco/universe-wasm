# Universe-WASM: Rust/WASM N-Body QJL Engine

## Overview
Complete Rust/WASM N-body simulation engine with three force computation modes:
- **Mode 0**: Exact Barnes-Hut (baseline)
- **Mode 1**: QJL quantized spherical coords (no cache)
- **Mode 2**: QJL with HashMap force caching

## Files

### Cargo.toml
- wasm-bindgen 0.2 (JS interop)
- js-sys 0.3 (JS Date::now() for timing)
- web-sys 0.3 (Window/Performance APIs)
- Release profile: opt-level=3, lto=true

### src/lib.rs (592 lines)

#### Constants
- **G** = 1.2 (gravitational constant)
- **THETA** = 0.7 (Barnes-Hut criterion)
- **SOFTENING** = 50.0 (collision softening)
- **QUANT_LEVEL** = 20.0 (radial quantization bucket size)
- **QUANT_ANGLE** = 0.1 rad (theta/phi quantization bucket size)
- **STRIDE** = 7 floats: [x, y, z, vx, vy, vz, mass]

#### Public API (#[wasm_bindgen])
```
Universe::new(n: usize) -> Universe
Universe::particles_ptr() -> *const f32
Universe::num_particles() -> usize
Universe::init_galaxy()
Universe::set_mode(mode: u32)
Universe::step() -> f32                    // returns timing in ms
Universe::cache_hits() -> u32
Universe::cache_misses() -> u32
Universe::cache_size() -> u32
Universe::compute_ke() -> f64
```

#### Core Data Structures
- **Universe**: holds particle buffer (7-stride), octree, cache (RefCell for interior mutability)
- **OctNode**: octree node with CoM, mass, particle refs, 8 children pointers, sequential ID

#### Force Computation Pipeline

**Mode 0: Exact Barnes-Hut**
1. `compute_force_exact()` → `traverse_exact()`
2. For each node:
   - If leaf: compute Cartesian force `F = G*m/d²` with direction `(dx/d, dy/d, dz/d)`
   - If theta < THETA: treat as body; else recurse to 8 children
3. Accumulate fx, fy, fz

**Mode 1: QJL (Quantized Spherical)**
1. `compute_force_qjl()` → `traverse_qjl(use_cache=false)`
2. After theta criterion fires:
   - Convert to spherical: r, θ=acos(dz/r), φ=atan2(dy,dx)
   - Quantize: q_r = round(r/20)*20, q_θ = round(θ/0.1)*0.1, q_φ = round(φ/0.1)*0.1
   - Force magnitude: `F = G*m / (q_r)²`
   - Convert back to Cartesian: fx, fy, fz with sin(q_θ), cos(q_φ), etc.

**Mode 2: QJL + Cache**
1. Same as Mode 1 but with HashMap<u64, [f32;3]>
2. Cache key: `(node_id << 30) | (q_r_bucket << 20) | (q_θ_bucket << 10) | (q_φ_bucket)`
   - q_r_bucket = (dist / QUANT_LEVEL) as u32
   - q_θ_bucket = (theta / QUANT_ANGLE) as u32
   - q_φ_bucket = ((phi / QUANT_ANGLE) as i32 + 64) as u32 (signed offset)
3. On hit: return cached [fx, fy, fz], increment cache_hits
4. On miss: compute, store, increment cache_misses
5. Cache cleared each frame

#### Octree Construction
1. Find AABB of all particles
2. Create root OctNode at center with size = max_dim * 1.1
3. For each particle: `insert_particle(node_idx, x, y, z, mass, particle_idx)`
   - If leaf empty: store particle, set CoM = position
   - If leaf occupied: split into 8 children (allocate new OctNode IDs), recurse
   - If internal: find child octant, recurse
4. Update CoM bottom-up via `update_octree_com()`

#### Particle Integration (Verlet-like)
```
ax, ay, az = F / mass
vx += ax * dt
vy += ay * dt
vz += az * dt
x += vx * dt
y += vy * dt
z += vz * dt
```
Fixed dt = 0.01

#### Galaxy Initialization
Spiral disk with 1.5-power radial distribution:
```
r = random^1.5 * 600 + 20
θ = random * 2π
orbital_v = sqrt(300/r) * 4
x = r*cos(θ)
y = r*sin(θ)
vx, vy = perpendicular velocity
z = random * r * 0.2 (vertical scatter)
```

### build.sh
```bash
RUSTFLAGS="-C target-feature=+simd128" wasm-pack build --target web --release
```
- Enables SIMD128 for future optimization
- Outputs to `pkg/` with TypeScript bindings
- Release mode: 3 levels of optimization + LTO

## Design Notes

1. **Interior Mutability**: Cache/cache_hits/cache_misses use RefCell<T> to allow mutation in `step()` which has &mut self
2. **Zero-Copy Bridge**: `particles_ptr()` exposes raw pointer; JS wraps in Float32Array over wasm.memory.buffer
3. **Sequential Node IDs**: Each frame resets node counter; enables simple u64 cache key packing
4. **Perturbation**: Coincident particles offset by 1e-5 to avoid division by zero
5. **Safety**: Uses safe Rust; SIMD blocks can be added later with feature flags

## Expected Performance
- JS prototype: 45% cache hit rate (but slow due to Map overhead)
- Rust version: Integer hash map should eliminate overhead, achieving actual speedup
- Three-mode benchmark shows trade-off: Exact (slow, precise) → QJL (faster) → Cached (fastest, quantized)
