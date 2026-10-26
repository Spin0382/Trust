# Vanilla-identical world generation

The target: **the same seed produces the same world as Terraria does**.

This is not the same thing as *working* world generation, which is done and described in
[worldgen.md](worldgen.md). A generated world here is complete and can be played through; what it
is not is byte-identical to the one Terraria would build from the same seed. This document is
about closing that second gap, which is a far larger job.

This is the largest single piece of work in the project, and this document exists mostly so that
the decision to continue or stop can be taken deliberately rather than by drift.

**Code:** [`world/worldgen/`](../crates/terrustia/src/world/worldgen).

## The oracle

Every `.wld` ends its header with a JSON manifest: all 106 generation passes, **in execution
order**, each with a `RandNext` — `WorldGen.genRand.Next()` sampled immediately after that pass
(`WorldGenerator.cs:515`).

The game reseeds the generator from the world seed at the **start of every pass**
(`WorldGenerator.cs:499`), so `RandNext` fingerprints the draws that pass made, not the whole
prefix before it. A pass that draws nothing records the seed's own first draw; the reference world
records `143337951` for both "Wavy Caves" and "Mud Caves To Grass", which is the first value for
seed 12345. So:

> Implement pass N, generate with the same seed, compare. If it matches, that pass's draw count
> and order are exactly right. If it does not, the pass that disagrees is the one that is wrong.

That turns a 106-link sequence — where nothing can be judged until all of it is done — into
something checkable one pass at a time. Without it the port is unfalsifiable.

```sh
cargo run --release -p terrustia --example genparity -- reference.wld
```

Two further properties worth knowing:

- **Sampling advances the generator.** A port has to take a draw at the same point or it diverges
  from that pass's recorded value.
- **A skipped pass records nothing and consumes zero RNG.** So generating a reference with a pass
  disabled, and stubbing that pass to draw nothing, matches exactly for that pass.
  Deferring a pass is therefore *free*, not fatal.

## What is done

- `UnifiedRandom` ported and pinned. `Random(0).Next()` gives .NET's own documented `1559595546`,
  and seven sequences match a second implementation written separately from the same source.
- `TranslateSeed`: parse as an integer, else CRC32.
- The manifest reader, hand-written to avoid a JSON dependency.
- `genparity`, which reads a reference world's seed and pass record and names the first divergence.
- `WorldGen.Reset` for an ordinary world: the seed's draws up to the first pass, including the
  tree, cave and background styles, moon, ore tiers, dungeon side and column, jungle and snow
  origins, and both beach ends. Every value it produces for the reference world matches that
  world's own header (tree/cave styles, backgrounds, moon, clouds, wind, world id, ore tiers).
- `TerrainPass`, the first pass: all five surface shapes, `SurfaceHistory`, the surface and rock
  layer calculations, `FillColumn`, `RetargetColumn`, `RetargetSurfaceHistory`,
  `GenerateWorldSurfaceOffset`, and the beach padding (`FlatBeachPadding = 5`).

Against the reference world (4200x1200, crimson, seed text `12345`) it now reports:

```
passes    106 recorded, 106 ran, 0 skipped
all 1 implemented passes match the reference.
next to write: "Dunes"
```

## What is not

The other 105 passes. Sizing, honestly: **219–372 engineer-days, plan against 300** — 14 to
20 months at full time. The earlier estimate of 80–120 was too optimistic by roughly a factor of
three.

The go/no-go is the terrain spine (passes 1–19), which costs 48–64 days to reach and tells you
whether the rest converges. The signal to stop is the fraction of passes needing guess-and-check
against the oracle rather than straight translation; above about 20% at that point, it will not
converge.

## Four findings that change the shape of the work

Recorded here because three of them contradict the obvious approach.

1. **The game ships a worldgen debugger**, behind one static bool
   (`Terraria.Testing/DebugOptions.cs:8`). With it on, `WorldGenerator.cs:128` hashes the world
   after every pass and `WorldGenSnapshot` dumps the tiles *and* all of `GenVars` as JSON per
   pass. That turns verification from a 109-link chain into 109 independently checkable units, and
   a `GenVars` differ names the wrong **variable** rather than a tile coordinate. It means
   patching a local game binary.

2. **`PlaceTile` must be ported, not tabled.** It is saturated with `genRand` — style rolls
   interleaved with placement-validity branches — so its RNG consumption is data-dependent on the
   branch taken. A frame lookup table cannot reproduce that. Conversely `TileFrameCosmetic` (3,450
   lines) has one RNG site and produces frames the `.wld` does not even store: skip it entirely.

3. **Liquid settling must be exact.** `Liquid.cs:899` draws `genRand.Next(30)` *inside* the
   cellular simulation, so the number of draws depends on the exact settling trajectory. Two
   landmines come with it: .NET's `Math.Round` is banker's rounding where Rust's is
   half-away-from-zero, and vanilla uses non-short-circuiting `&` on bools in places, which
   consumes a draw where `&&` would not.

4. **Special seeds must be stripped on day one.** 270 special-seed sites inside `AddPasses()`, of
   which **18 touch RNG on the same line**. Strip the *bodies*; keep any RNG evaluated in the
   *condition*, or parity breaks silently.

## The fallback, which is now the shipping generator

The plan named a graceful degradation: every structure present, built with our own algorithms,
beatable but not seed-identical. That has been **built** — see [worldgen.md](worldgen.md) — so the
question is no longer "what if parity does not converge" but "is parity worth its cost on top of a
working generator".

That makes the decision cleaner rather than harder. Parity buys one thing: a seed shared with
another player or a wiki reproduces their world. Everything else about playing on this server
already works. The milestones are still defined by a green `genparity` run, and the go/no-go is
still the terrain spine.

## What is needed from a human

The Terrain work was checked against a **Terraria 1.4.5.8** world (4200x1200, crimson, seed text
`12345`), the version and evil this port targets. A second world at another size and the other
evil would still be valuable: a mistake that happens to agree at 4200/crimson, such as a wrong
`GetWorldSize()` branch, would otherwise stay invisible.
