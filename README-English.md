# open-cg-cad

*日本語*: [README.md](README.md)

A CAD system for "AI Construction Company & AI Building" features. Given
multiple land parcels from real-estate search results, it checks zoning
regulations (right to sunlight, building coverage ratio, floor area ratio,
etc.), auto-generates 3D building volumes as CAD data, applies basic
coloring, and can present multiple construction methods.

**Added 2026-08-27**: A redesign feature for old semiconductor (CPU/NPU/GPU)
and vehicle (automobile/motorcycle/shinkansen/maglev/aircraft, including
gasoline/diesel/EV/PHEV engines) drawings — compare an old drawing's text
description against a new instruction or re-uploaded spec/instruction
document, and have the AI generate a redesign proposal — plus GitHub and
VPS/rental-server read-write agents. See [CLAUDE.md](CLAUDE.md) for the full
scope and honest disclosure of what is (and isn't) implemented.
`https://easy-web.tokyo/open-cg-cad/` is live; a demo + download entry
point (for running it locally on your own drive) is at
`https://easy-web.tokyo/open-cg-cad/demo`. **Verified on a real running
instance**: both the redesign and merge success paths have been confirmed
end-to-end with a live aruaru-llm actually returning generated text (see
CLAUDE.md for details).

## ⚠️ Important disclaimer (must read)

**This system is a draft/sample generation tool intended to hand off to a
licensed professional (architect, real-estate transaction specialist, etc.)
one step before their final review — it does NOT produce a legally
finalized design or official drawings usable for a building permit
application. A licensed professional's review is always required.**

- Structural calculation / seismic simulation is implemented as an MVP
  (`cg-cad-core::structural`): a simplified shear-building frame model
  with linear static analysis. It is not a substitute for full FEM or
  dedicated professional engineering software. For more rigorous
  analysis, `cg-cad-core::solver_bridge` can drive CalculiX/OpenFOAM if
  the user installs them separately.
- Zoning parameters (building coverage ratio, floor area ratio caps, etc.)
  are treated as **user-supplied input values** — automatic retrieval of
  up-to-date regulations via web search is out of scope for this session
  (future work).
- Generated building shapes are limited to axis-aligned boxes (or simple
  combinations thereof) — a rough massing draft, not a detailed design.

See [CLAUDE.md](CLAUDE.md) for details.

## Layout

- [`crates/cg-cad-core`](crates/cg-cad-core): core logic for site
  (`Site`), zoning (`ZoningRules`), building type (`BuildingType`), mesh
  (`Mesh`), materials, and building-volume generation
  (`generate_building`). Also includes (all key types/functions are
  re-exported at `cg_cad_core::` root):
  - `structural`: simplified frame-model structural analysis and
    inter-story drift checks.
  - `crash_sim`: mass-spring-damper crash simulation for vehicles
    (Car/Motorcycle/Maglev/Bus/Truck/Train). Also provides a simplified
    bridge (`EnvironmentConditions`/`apply_environment_conditions`) that
    maps `crosswind`/`tire_wet_grip` reference indices onto crash
    conditions ("windy day" / "wet-road slip" scenarios).
  - `airbag`: simplified deployment-timing and occupant-protection
    simulation for four airbag types (driver frontal, passenger frontal,
    curtain, side), approximating the occupant as a single mass-spring-
    damper degree of freedom driven by `crash_sim`'s body acceleration
    time series.
  - `tunnel_noise`: train nose-shape design calculation for tunnel
    micro-pressure wave (boom) reduction.
  - `solver_bridge`: I/O bridge to existing OSS solvers (CalculiX /
    OpenFOAM); honestly reports an error when the solver binary isn't
    installed rather than faking success.
  - `aero`: simplified vehicle/train aerodynamic (lift/downforce)
    estimation.
  - `crosswind`: simplified crosswind overturning-moment / critical wind
    speed estimation.
  - `tire_wet_grip`: simplified reference index for tire tread wet-grip
    performance.
  - `nwd_control`: simplified 2WD–8WD+ drive-torque distribution (torque
    vectoring) simulation.
  - `manufacturing_export`: manufacturing data export — STL (for 3D
    printers) and robot-arm waypoint JSON.
  - `llm_explain`: natural-language explanation of computed results via
    [`aruaru-llm`](https://github.com/aon-co-jp/aruaru-llm).
  - All of these are simplified approximations; each module's doc
    comment honestly states its simplifications and out-of-scope items
    (see the source files for details).
- [`crates/cg-cad-render`](crates/cg-cad-render): bridges
  `cg-cad-core`-generated meshes into
  [`open-directx`](https://github.com/aon-co-jp/open-directx)'s existing
  real Vulkan graphics pipeline (verified on real NVIDIA GT 730 hardware)
  for actual rendering. No new Vulkan/shader code is written here — the
  existing implementation is reused.

## Running it

```sh
cargo build --workspace --release
cargo test --workspace --release
```

`cg-cad-render`'s tests require real Vulkan hardware (they skip, not fake
a pass, when none is present).

## Dependencies on existing projects

- [`open-directx`](https://github.com/aon-co-jp/open-directx): 3D
  rendering backend (`directx-graphics-vulkan` reused via a path
  dependency).
- [`open-cuda`](https://github.com/aon-co-jp/open-cuda): `structural`,
  `crash_sim`, and `tunnel_noise` route their matrix math through
  `opencuda-blas`'s GEMM implementation (via `CpuDevice`). Switching to
  the real Vulkan device path was deferred as not worth verifying at the
  current problem scale (see CLAUDE.md's HANDOFF log).
- [`aruaru-llm`](https://github.com/aon-co-jp/aruaru-llm): the
  `llm_explain` module fetches natural-language explanations of
  computed results via `POST /v1/generate`. Real-server verification
  exists as an `#[ignore]`d test that doesn't run in CI by default (see
  CLAUDE.md's HANDOFF log).


---

## Update 2026-08-23 — Tunnel micro-pressure-wave index: O(n^2) → O(n) (measured 105.6x)

`micro_pressure_wave_index` in
`crates/cg-cad-core/src/tunnel_noise.rs` computed the cross-section
derivative `dA/dx` by **materializing a dense forward-difference matrix**
— a matrix with only two non-zeros per row — and running it through
`opencuda_blas::sgemm`. At `num_samples = 2000` that meant building a
~4-million-element (16 MiB) matrix on every call, with O(n^2) allocation
and multiply-accumulate work.

Mathematically it is just a difference of adjacent samples, so it was
replaced with an O(n) formulation built from two `open_cpu::axpy_f32`
calls (`d = -inv_dx * A[..n-1]`, then `+= inv_dx * A[1..]`). Both are
contiguous, so they vectorize under AVX2+FMA3 and fall back to scalar
automatically on CPUs without it.

Measured on a Ryzen 9 3950X:

```
n=  500  gemm(O(n^2)): 0.605 ms   fast(O(n)): 0.0213 ms   ( 28.4x)
n= 1000  gemm(O(n^2)): 1.452 ms   fast(O(n)): 0.0419 ms   ( 34.6x)
n= 2000  gemm(O(n^2)): 6.769 ms   fast(O(n)): 0.0641 ms   (105.6x)
```

The old implementation is retained as `micro_pressure_wave_index_gemm`,
and a test asserts the two agree across 5 power values x 6 sample counts,
guaranteeing the results did not change. **All 101 tests pass.**

**Honest disclosure**: the speedup comes primarily from the **algorithmic
fix** (O(n^2) → O(n)), not from SIMD. The `axpy_f32` vectorization is a
secondary effect on top of it. Do not read this as "SIMD gave us 105x".

### CPU-load audit of the rest of the repository

No other SIMD candidates were found. `crash_sim.rs` and `structural.rs`
iterate many times but each step operates on only a handful of elements
(lumped masses, storeys); the dominant cost there is the per-call `Vec`
allocation and `CpuDevice::new(0)` construction, so **reducing allocations,
not SIMD, is the right fix** (not done in this session). The airbag
occupant integrator is sequentially dependent and cannot be vectorized.
Mesh generation only ever handles cuboids (8 vertices, 36 indices).
