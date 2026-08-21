# open-cg-cad

*日本語*: [README.md](README.md)

A CAD system for "AI Construction Company & AI Building" features. Given
multiple land parcels from real-estate search results, it checks zoning
regulations (right to sunlight, building coverage ratio, floor area ratio,
etc.), auto-generates 3D building volumes as CAD data, applies basic
coloring, and can present multiple construction methods.

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
  - `crash_sim`: mass-spring-damper crash simulation for vehicles.
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
