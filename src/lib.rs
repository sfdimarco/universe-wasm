// ============================================================
// universe-wasm: Native WASM N-Body Engine with QJL + Cache
// ============================================================
// Arena-based octree, zero-copy JS bridge,
// integer-keyed force cache (HashMap<u64, [f32;3]>)
// ============================================================

use wasm_bindgen::prelude::*;
use std::collections::HashMap;

// --- Constants matching Universe1/2/3 JS implementations ---
const G: f32 = 1.2;
const THETA: f32 = 0.7;
const SOFTENING_SQ: f32 = 50.0 * 50.0;
const DT: f32 = 0.01;
const QUANT_LEVEL: f32 = 20.0;
const QUANT_ANGLE: f32 = 0.1;
const PHI_OFFSET: i32 = 64;

// 7-stride flat buffer: [x, y, z, vx, vy, vz, mass]
const STRIDE: usize = 7;
const X: usize = 0;
const Y: usize = 1;
const Z: usize = 2;
const VX: usize = 3;
const VY: usize = 4;
const VZ: usize = 5;
const MASS: usize = 6;

const MAX_DEPTH: u32 = 20;

// ============================================================
// OctNode — Arena-indexed
// ============================================================
#[derive(Clone)]
struct OctNode {
    center_x: f32,
    center_y: f32,
    center_z: f32,
    half_size: f32,
    mass: f32,
    com_x: f32,
    com_y: f32,
    com_z: f32,
    particle_index: i32,     // -1 = empty
    children_start: usize,   // 0 = leaf, >0 = arena index of first of 8 children
    id: u32,
}

impl OctNode {
    fn new(cx: f32, cy: f32, cz: f32, half_size: f32, id: u32) -> Self {
        OctNode {
            center_x: cx, center_y: cy, center_z: cz, half_size,
            mass: 0.0, com_x: 0.0, com_y: 0.0, com_z: 0.0,
            particle_index: -1, children_start: 0, id,
        }
    }
    #[inline(always)] fn is_leaf(&self) -> bool { self.children_start == 0 }
    #[inline(always)] fn is_empty(&self) -> bool { self.particle_index < 0 && self.children_start == 0 }
}

// ============================================================
// Universe
// ============================================================
#[wasm_bindgen]
pub struct Universe {
    particles: Vec<f32>,
    n: usize,
    mode: u32, // 0=Exact, 1=QJL, 2=QJL+Cache
    arena: Vec<OctNode>,
    next_node_id: u32,
    cache: HashMap<u64, [f32; 3]>,
    cache_hits: u32,
    cache_misses: u32,
    last_force_time_ms: f32,
    frame_count: u32,
}

#[wasm_bindgen]
impl Universe {
    #[wasm_bindgen(constructor)]
    pub fn new(n: usize) -> Universe {
        Universe {
            particles: vec![0.0f32; n * STRIDE],
            n,
            mode: 0,
            arena: Vec::with_capacity(n * 4),
            next_node_id: 0,
            cache: HashMap::with_capacity(n * 2),
            cache_hits: 0,
            cache_misses: 0,
            last_force_time_ms: 0.0,
            frame_count: 0,
        }
    }

    /// Zero-copy bridge: JS creates Float32Array view over this pointer
    pub fn particles_ptr(&self) -> *const f32 { self.particles.as_ptr() }
    pub fn buffer_len(&self) -> usize { self.particles.len() }
    pub fn num_particles(&self) -> usize { self.n }

    pub fn init_galaxy(&mut self) {
        let tau = std::f32::consts::TAU;
        for i in 0..self.n {
            let fi = i as f32;
            let r1 = (fi * 12.9898).sin() * 43758.5453;
            let r = r1.fract().abs().powf(1.5) * 600.0 + 20.0;
            let r2 = (fi * 78.233).sin() * 43758.5453;
            let angle = r2.fract().abs() * tau;
            let x = r * angle.cos();
            let y = r * angle.sin();
            let r3 = (fi * 45.164).sin() * 43758.5453;
            let z = r3.fract() * r * 0.2;
            let v_orbital = (300.0 / r).sqrt() * 4.0;
            let vx = -v_orbital * angle.sin();
            let vy = v_orbital * angle.cos();
            let idx = i * STRIDE;
            self.particles[idx + X] = x;
            self.particles[idx + Y] = y;
            self.particles[idx + Z] = z;
            self.particles[idx + VX] = vx;
            self.particles[idx + VY] = vy;
            self.particles[idx + VZ] = 0.0;
            self.particles[idx + MASS] = 1.0;
        }
    }

    pub fn set_mode(&mut self, mode: u32) { self.mode = mode.min(2); }
    pub fn get_mode(&self) -> u32 { self.mode }

    pub fn step(&mut self) -> f32 {
        let perf = js_sys::Date::now();

        self.cache.clear();
        self.cache_hits = 0;
        self.cache_misses = 0;

        // Build octree
        self.build_octree();

        let force_start = js_sys::Date::now();

        // Compute all forces first (leapfrog)
        // Destructure to satisfy borrow checker: arena (immut) + cache (mut) are disjoint fields
        let mut forces = vec![0.0f32; self.n * 3];
        let mode = self.mode;
        let n = self.n;
        let mut total_hits = 0u32;
        let mut total_misses = 0u32;

        {
            let arena = &self.arena;
            let cache = &mut self.cache;
            let particles = &self.particles;

            for i in 0..n {
                let idx = i * STRIDE;
                let px = particles[idx + X];
                let py = particles[idx + Y];
                let pz = particles[idx + Z];

                let (fx, fy, fz) = match mode {
                    0 => traverse_exact(arena, i, 0, px, py, pz),
                    1 => {
                        let mut h = 0u32;
                        let mut m = 0u32;
                        let f = traverse_qjl(arena, cache, i, 0, px, py, pz, false, &mut h, &mut m);
                        total_hits += h;
                        total_misses += m;
                        f
                    }
                    2 | _ => {
                        let mut h = 0u32;
                        let mut m = 0u32;
                        let f = traverse_qjl(arena, cache, i, 0, px, py, pz, mode == 2, &mut h, &mut m);
                        total_hits += h;
                        total_misses += m;
                        f
                    }
                };

                forces[i * 3] = fx;
                forces[i * 3 + 1] = fy;
                forces[i * 3 + 2] = fz;
            }
        }
        self.cache_hits = total_hits;
        self.cache_misses = total_misses;

        let force_end = js_sys::Date::now();
        self.last_force_time_ms = (force_end - force_start) as f32;

        // Integrate
        for i in 0..self.n {
            let idx = i * STRIDE;
            let m = self.particles[idx + MASS];
            self.particles[idx + VX] += (forces[i * 3] / m) * DT;
            self.particles[idx + VY] += (forces[i * 3 + 1] / m) * DT;
            self.particles[idx + VZ] += (forces[i * 3 + 2] / m) * DT;
            self.particles[idx + X] += self.particles[idx + VX] * DT;
            self.particles[idx + Y] += self.particles[idx + VY] * DT;
            self.particles[idx + Z] += self.particles[idx + VZ] * DT;
        }

        self.frame_count += 1;
        (js_sys::Date::now() - perf) as f32
    }

    // Stats
    pub fn force_time_ms(&self) -> f32 { self.last_force_time_ms }
    pub fn cache_hits(&self) -> u32 { self.cache_hits }
    pub fn cache_misses(&self) -> u32 { self.cache_misses }
    pub fn cache_size(&self) -> u32 { self.cache.len() as u32 }
    pub fn cache_hit_rate(&self) -> f32 {
        let t = self.cache_hits + self.cache_misses;
        if t == 0 { 0.0 } else { self.cache_hits as f32 / t as f32 }
    }
    pub fn frame_count(&self) -> u32 { self.frame_count }
    pub fn tree_node_count(&self) -> u32 { self.arena.len() as u32 }

    pub fn compute_ke(&self) -> f64 {
        let mut ke: f64 = 0.0;
        for i in 0..self.n {
            let idx = i * STRIDE;
            let vx = self.particles[idx + VX] as f64;
            let vy = self.particles[idx + VY] as f64;
            let vz = self.particles[idx + VZ] as f64;
            let m = self.particles[idx + MASS] as f64;
            ke += 0.5 * m * (vx * vx + vy * vy + vz * vz);
        }
        ke
    }

    pub fn reset_stats(&mut self) {
        self.frame_count = 0;
        self.cache_hits = 0;
        self.cache_misses = 0;
    }
}

// ============================================================
// Octree building (methods on Universe)
// ============================================================
impl Universe {
    fn build_octree(&mut self) {
        if self.n == 0 { return; }
        self.arena.clear();
        self.next_node_id = 0;

        let mut min_x = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_z = f32::NEG_INFINITY;
        for i in 0..self.n {
            let idx = i * STRIDE;
            let (x, y, z) = (self.particles[idx], self.particles[idx+1], self.particles[idx+2]);
            min_x = min_x.min(x); max_x = max_x.max(x);
            min_y = min_y.min(y); max_y = max_y.max(y);
            min_z = min_z.min(z); max_z = max_z.max(z);
        }

        let hs = (max_x-min_x).max(max_y-min_y).max(max_z-min_z) * 0.5 + 1.0;
        let cx = (min_x+max_x)*0.5;
        let cy = (min_y+max_y)*0.5;
        let cz = (min_z+max_z)*0.5;

        let rid = self.alloc_id();
        self.arena.push(OctNode::new(cx, cy, cz, hs, rid));

        for i in 0..self.n {
            let idx = i * STRIDE;
            let (x, y, z, m) = (
                self.particles[idx], self.particles[idx+1],
                self.particles[idx+2], self.particles[idx+MASS]
            );
            self.insert(0, x, y, z, m, i as i32, 0);
        }
        self.compute_com(0);
    }

    fn alloc_id(&mut self) -> u32 { let id = self.next_node_id; self.next_node_id += 1; id }

    fn insert(&mut self, ni: usize, x: f32, y: f32, z: f32, mass: f32, pi: i32, depth: u32) {
        if depth > MAX_DEPTH {
            let node = &mut self.arena[ni];
            let t = node.mass + mass;
            if t > 0.0 {
                node.com_x = (node.com_x * node.mass + x * mass) / t;
                node.com_y = (node.com_y * node.mass + y * mass) / t;
                node.com_z = (node.com_z * node.mass + z * mass) / t;
            }
            node.mass = t;
            return;
        }

        let is_leaf = self.arena[ni].is_leaf();
        let is_empty = self.arena[ni].is_empty();

        if is_empty && is_leaf {
            let node = &mut self.arena[ni];
            node.particle_index = pi;
            node.mass = mass;
            node.com_x = x; node.com_y = y; node.com_z = z;
            return;
        }

        if is_leaf {
            // Occupied leaf — subdivide
            let old_pi = self.arena[ni].particle_index;
            let old_x = self.arena[ni].com_x;
            let old_y = self.arena[ni].com_y;
            let old_z = self.arena[ni].com_z;
            let old_m = self.arena[ni].mass;

            self.subdivide(ni);
            self.arena[ni].particle_index = -1;

            // Re-insert old particle
            let oct_old = octant_of(&self.arena[ni], old_x, old_y, old_z);
            let ci_old = self.arena[ni].children_start + oct_old;
            self.insert(ci_old, old_x, old_y, old_z, old_m, old_pi, depth + 1);

            // Insert new particle
            let oct_new = octant_of(&self.arena[ni], x, y, z);
            let ci_new = self.arena[ni].children_start + oct_new;
            self.insert(ci_new, x, y, z, mass, pi, depth + 1);
            return;
        }

        // Internal node — recurse
        let oct = octant_of(&self.arena[ni], x, y, z);
        let ci = self.arena[ni].children_start + oct;
        self.insert(ci, x, y, z, mass, pi, depth + 1);
    }

    fn subdivide(&mut self, ni: usize) {
        let cx = self.arena[ni].center_x;
        let cy = self.arena[ni].center_y;
        let cz = self.arena[ni].center_z;
        let qs = self.arena[ni].half_size * 0.5;
        let cs = self.arena.len();
        self.arena[ni].children_start = cs;

        for oct in 0..8u32 {
            let dx = if (oct & 1) != 0 { qs } else { -qs };
            let dy = if (oct & 2) != 0 { qs } else { -qs };
            let dz = if (oct & 4) != 0 { qs } else { -qs };
            let id = self.alloc_id();
            self.arena.push(OctNode::new(cx+dx, cy+dy, cz+dz, qs, id));
        }
    }

    fn compute_com(&mut self, ni: usize) {
        if self.arena[ni].is_leaf() { return; }
        let cs = self.arena[ni].children_start;
        let mut tm = 0.0f32;
        let mut wx = 0.0f32;
        let mut wy = 0.0f32;
        let mut wz = 0.0f32;
        for c in 0..8 {
            let ci = cs + c;
            if ci < self.arena.len() {
                self.compute_com(ci);
                let cm = self.arena[ci].mass;
                if cm > 0.0 {
                    tm += cm;
                    wx += self.arena[ci].com_x * cm;
                    wy += self.arena[ci].com_y * cm;
                    wz += self.arena[ci].com_z * cm;
                }
            }
        }
        if tm > 0.0 {
            self.arena[ni].mass = tm;
            self.arena[ni].com_x = wx / tm;
            self.arena[ni].com_y = wy / tm;
            self.arena[ni].com_z = wz / tm;
        }
    }
}

// ============================================================
// Free functions for force traversal (avoids borrow conflicts)
// ============================================================

#[inline(always)]
fn octant_of(node: &OctNode, x: f32, y: f32, z: f32) -> usize {
    let mut i = 0usize;
    if x >= node.center_x { i |= 1; }
    if y >= node.center_y { i |= 2; }
    if z >= node.center_z { i |= 4; }
    i
}

/// Exact Barnes-Hut traversal (read-only arena, no cache)
fn traverse_exact(
    arena: &[OctNode], p_idx: usize, ni: usize,
    px: f32, py: f32, pz: f32,
) -> (f32, f32, f32) {
    let node = &arena[ni];
    if node.mass == 0.0 { return (0.0, 0.0, 0.0); }
    if node.is_leaf() && node.particle_index == p_idx as i32 { return (0.0, 0.0, 0.0); }

    let dx = node.com_x - px;
    let dy = node.com_y - py;
    let dz = node.com_z - pz;
    let dist_sq = dx*dx + dy*dy + dz*dz + SOFTENING_SQ;
    let dist = dist_sq.sqrt();

    if node.is_leaf() || (node.half_size * 2.0 / dist < THETA) {
        let f = G * node.mass / dist_sq;
        let inv = 1.0 / dist;
        return (f * dx * inv, f * dy * inv, f * dz * inv);
    }

    let cs = node.children_start;
    let mut fx = 0.0f32;
    let mut fy = 0.0f32;
    let mut fz = 0.0f32;
    for c in 0..8 {
        let (cfx, cfy, cfz) = traverse_exact(arena, p_idx, cs + c, px, py, pz);
        fx += cfx; fy += cfy; fz += cfz;
    }
    (fx, fy, fz)
}

/// QJL traversal with optional cache (arena borrowed immutably, cache mutably)
fn traverse_qjl(
    arena: &[OctNode],
    cache: &mut HashMap<u64, [f32; 3]>,
    p_idx: usize, ni: usize,
    px: f32, py: f32, pz: f32,
    use_cache: bool,
    hits: &mut u32, misses: &mut u32,
) -> (f32, f32, f32) {
    let node = &arena[ni];
    if node.mass == 0.0 { return (0.0, 0.0, 0.0); }
    if node.is_leaf() && node.particle_index == p_idx as i32 { return (0.0, 0.0, 0.0); }

    let dx = node.com_x - px;
    let dy = node.com_y - py;
    let dz = node.com_z - pz;
    let dist_sq = dx*dx + dy*dy + dz*dz + SOFTENING_SQ;
    let dist = dist_sq.sqrt();

    if node.is_leaf() || (node.half_size * 2.0 / dist < THETA) {
        return qjl_force(cache, dx, dy, dz, dist, node.mass, node.id, use_cache, hits, misses);
    }

    let cs = node.children_start;
    let mut fx = 0.0f32;
    let mut fy = 0.0f32;
    let mut fz = 0.0f32;
    for c in 0..8 {
        let (cfx, cfy, cfz) = traverse_qjl(arena, cache, p_idx, cs+c, px, py, pz, use_cache, hits, misses);
        fx += cfx; fy += cfy; fz += cfz;
    }
    (fx, fy, fz)
}

/// Compute force using QJL spherical quantization, with optional cache
#[inline]
fn qjl_force(
    cache: &mut HashMap<u64, [f32; 3]>,
    dx: f32, dy: f32, dz: f32, dist: f32,
    node_mass: f32, node_id: u32,
    use_cache: bool,
    hits: &mut u32, misses: &mut u32,
) -> (f32, f32, f32) {
    let q_rad_bucket = ((dist / QUANT_LEVEL).round() as u32).max(1);
    let theta = (dz / dist).max(-1.0).min(1.0).acos();
    let q_theta_bucket = (theta / QUANT_ANGLE).round() as u32;
    let phi = dy.atan2(dx);
    let q_phi_bucket = ((phi / QUANT_ANGLE).round() as i32 + PHI_OFFSET) as u32;

    // Pack: node_id(upper 34 bits) | rad(10) | theta(10) | phi(10)
    let key: u64 = ((node_id as u64) << 30)
        | ((q_rad_bucket as u64 & 0x3FF) << 20)
        | ((q_theta_bucket as u64 & 0x3FF) << 10)
        | (q_phi_bucket as u64 & 0x3FF);

    if use_cache {
        if let Some(&cached) = cache.get(&key) {
            *hits += 1;
            return (cached[0], cached[1], cached[2]);
        }
    }

    let q_rad = q_rad_bucket as f32 * QUANT_LEVEL;
    let q_theta = q_theta_bucket as f32 * QUANT_ANGLE;
    let q_phi = (q_phi_bucket as i32 - PHI_OFFSET) as f32 * QUANT_ANGLE;

    let fmag = G * node_mass / (q_rad * q_rad);
    let st = q_theta.sin();
    let ct = q_theta.cos();
    let sp = q_phi.sin();
    let cp = q_phi.cos();

    let fx = fmag * st * cp;
    let fy = fmag * st * sp;
    let fz = fmag * ct;

    if use_cache {
        *misses += 1;
        cache.insert(key, [fx, fy, fz]);
    }

    (fx, fy, fz)
}

// ============================================================
// Morton encoding (Z-order curve, exposed for JS debug)
// ============================================================
#[inline(always)]
fn spread_bits_10(mut v: u32) -> u32 {
    v &= 0x3FF;
    v = (v | (v << 16)) & 0x030000FF;
    v = (v | (v << 8))  & 0x0300F00F;
    v = (v | (v << 4))  & 0x030C30C3;
    v = (v | (v << 2))  & 0x09249249;
    v
}

#[wasm_bindgen]
pub fn morton_key_3d(x: u32, y: u32, z: u32) -> u32 {
    spread_bits_10(x) | (spread_bits_10(y) << 1) | (spread_bits_10(z) << 2)
}
