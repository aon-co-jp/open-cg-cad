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

- Structural calculation / seismic simulation is not implemented (requires
  dedicated professional engineering software, see
  `cg-cad-core::structural`).
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
  (`generate_building`).
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
- [`open-cuda`](https://github.com/aon-co-jp/open-cuda) /
  [`aruaru-llm`](https://github.com/aon-co-jp/aruaru-llm): candidates for
  future GPU compute backing and a natural-language design-change
  interface, but no direct technical dependency was added this session.
