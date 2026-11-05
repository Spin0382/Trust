# Repository Guidelines

Canonical instructions for contributors and AI agents working in terrustia. `CLAUDE.md` is a
symlink to this file; edit this one only.

## Project Overview

terrustia is a Terraria 1.4.5.8 dedicated server written from scratch in async Rust. Real Terraria
clients connect to it, and the worlds it serves are ordinary `.wld` files. The wire protocol is
release 326 (release 325 is also accepted). The version is deliberately `0.0.1`: world generation
is visibly unfinished, and the first release should not imply otherwise.

The goal is faithful transcription of vanilla behavior, not reinterpretation. Copy mechanics,
formulas, packet shapes, and data from the game's shipped code; cite where each transcription came
from; disclose every deliberate narrowing or gap. Do not check decompiled game source, assets, or
text into the repository. `npc_data.rs` and `placed_items.rs` are hand-maintained exceptions with
their own checkers.

## Architecture & Data Flow

Four crates:

| Crate | Role | License |
|---|---|---|
| `crates/terrustia-proto` | Wire format, packet types and IDs, tile-section coding, generated data tables. No I/O. Every packet round-trips in a unit test. | MIT |
| `crates/terrustia-client` | Headless client speaking the real protocol. Drives tests, probes real servers, runs verification bots. | AGPL-3.0-or-later |
| `crates/terrustia` | Async server: world state, game loop, connections, `.wld` I/O, console, admin, optional web panel. | AGPL-3.0-or-later |
| `crates/terrustia-codegen` | Hand-run table generator from a decompiled tree. Excluded from default members. | AGPL-3.0-or-later |

Runtime model:

- Tokio multi-threaded runtime (`#[tokio::main]`), fixed 60 Hz tick (`16_666_667 ns`,
  `game/server/tick.rs`), `MissedTickBehavior::Skip`.
- Single-writer actor: one game task exclusively owns `GameServer` and its `World`, players, NPCs,
  items, and projectiles. The hot path has no world lock and no interior mutability.
- Connections send `ServerEvent` over one bounded `mpsc` (`EVENT_QUEUE = 4096`). Server-to-client
  traffic uses a bounded per-player `mpsc<Bytes>` plus a global byte budget.
- Events carry `(slot, epoch)`. The epoch prevents recycled connection slots from delivering ghost
  packets. Unknown packet IDs are logged and dropped, never panicked on.
- Not an ECS and not an actor framework. Packet dispatch is a hand-written `match` on numeric IDs;
  packet codecs are hand-written typed functions.
- The web panel runs in separate axum tasks and never touches world state directly. Reads go
  through `ServerEvent` plus `oneshot` requests handled by the game task. Expensive hashing and
  audit I/O run on blocking tasks.
- World switching is not a hot swap: the server stops cleanly, saves, and `exec`s a replacement
  process pointed at the new world.

Packet path:

```text
TCP bytes
  -> net/codec.rs: [u16 len][u8 id][payload]
  -> net/connection.rs read_loop
  -> ServerEvent::Packet { slot, epoch, frame }
  -> game/server/mod.rs handle_event (epoch checked)
  -> game/server/dispatch.rs handle_packet: match frame.id
  -> on_* handler validates and mutates world/player/NPC state
  -> send_bytes / broadcast
  -> per-player mpsc<Bytes> -> write_loop -> TCP
```

Tick phases are ordered: clock and events, snapshot drain and autosave, auth/save completion,
liquids, growth and spread, weather, tile entities and mechanisms, dirty sections and streaming,
items, NPC/buff processing, projectiles, contact damage, spawning, housing. Keep new logic in the
appropriate phase rather than inventing a parallel loop.

World I/O:

- Load with `world/wld.rs`; save with `world/wld_save.rs` through a temp file and atomic rename.
- Loaded worlds preserve and patch their existing header byte-for-byte where possible. Generated
  worlds are written at protocol 326.
- Autosave drains an incremental world snapshot over multiple ticks, then writes on
  `spawn_blocking`. Shutdown saves synchronously.
- World generation lives in `crates/terrustia/src/world/worldgen/`. It targets complete, playable
  worlds; seed-identical vanilla parity is tracked separately in `docs/worldgen-parity.md`.

## Key Directories

| Path | Purpose |
|---|---|
| `crates/terrustia/src/main.rs` | Binary entry point, CLI, config layering, boot orchestration, stop and relaunch |
| `crates/terrustia/src/game/server/` | `GameServer`, event handling, dispatch, tick loop, systems, console, panel bridge |
| `crates/terrustia/src/game/` | Player, NPC, AI transcriptions, projectiles, events, spawning |
| `crates/terrustia/src/net/` | Framing, accept loop, per-connection tasks, traffic recording |
| `crates/terrustia/src/world/` | Authoritative world, tile store, `.wld` read/write, worldgen |
| `crates/terrustia/src/panel/`, `admin/` | Admin HTTP/WebSocket panel, accounts, permissions, bans, audit |
| `crates/terrustia/src/config.rs` | TOML config, environment overrides, validation |
| `crates/terrustia-proto/src/` | `id.rs`, `packets/`, `reader.rs`, `writer.rs`, `section.rs`, data tables |
| `crates/terrustia-client/src/` | Client handshake, actions, local world view, capture files |
| `crates/terrustia/tests/` | Out-of-process and in-process integration tests |
| `crates/terrustia/web-panel/` | Svelte 5 + TypeScript + Vite admin UI, built with bun |
| `docs/` | Protocol, world file, performance, parity, release-blocker references |
| `tools/` | Python checkers and shell verification scripts |
| `fuzz/` | Standalone `cargo-fuzz` workspace |

## Development Commands

Toolchain: Rust 1.97.1 (pinned by `rust-toolchain.toml`), edition 2024, resolver 3. Use `just` for
recipes; each wraps `cargo`.

| Command | Effect |
|---|---|
| `just install` | Fetch Rust deps and install panel deps if bun exists |
| `just run -- --world W.wld` | Release server (`cargo run --release -p terrustia`) |
| `just dev` | Build panel, then debug server with `--features embed-web` |
| `just web` | Vite panel dev server with hot reload (bun) |
| `just build` | Build panel, then the release workspace |
| `just check` | CI-equivalent: fmt, clippy (default and no-default-features), cargo-deny, packet audit, web build, tests |
| `just check-rust` | Lint and supply-chain passes without web or tests |
| `just test` | `cargo test --workspace` |
| `just test-filter NAME` | Filtered workspace tests with `--nocapture` |
| `just check-panel` | Release binary plus Playwright Chromium panel spec |
| `just soak [SECONDS]` | Real server plus three real clients (`tools/soak_ci.sh`) |
| `just fuzz [TARGET] [SECONDS]` | Fuzz `packet_decoders` or `section_stream` (nightly + cargo-fuzz) |
| `just regen` | Regenerate all data tables from `.scratch/decompiled` |
| `just check-data` | Local-only data, parity, citation, reachability, and mutation checks |
| `just roundtrip FILE OUT` | Byte-level `.wld` round-trip check |
| `just conform ADDR CAPTURE` | Re-encode a real server's captured bytes |
| `just docker-build` | Build the container image (expects a prebuilt binary) |

Examples:

```sh
cargo test -p terrustia --test gameplay
cargo test -p terrustia --test gameplay a_chest_can_be_opened -- --nocapture
cargo run --release -p terrustia-client --example verify -- 127.0.0.1:7777
cargo run --release -p terrustia --example probe -- --help
```

## Code Conventions & Common Patterns

### Compilation and errors

- Workspace lints deny all warnings; clippy `all` is denied, pedantic is intentionally not enabled
  because it fights transcribed code shapes. A dead import or unused variable fails the build.
- Errors use per-module `thiserror` enums with a local `type Result<T>`. There is no `anyhow`.
  `main` may use `Box<dyn std::error::Error>` for CLI orchestration.
- Production paths must not panic. Runtime failures degrade gracefully; autosave failures warn and
  retry. The server catches panics on the packet path only to save the world and exit non-zero, so
  release keeps `panic = "unwind"`.
- Never introduce `.unwrap()`, `.expect()`, or `panic!` outside `#[cfg(test)]` without a strong
  invariant and a comment. `tests/panic_budget.rs` encodes the allowed production panic count.

### Naming and module layout

- Handlers: `on_<packet>`; per-tick systems: `tick_<system>`; recorders: `record_*`; notes:
  `note_*`.
- Coordinator modules (`game/server/mod.rs`, `panel/mod.rs`) own shared types and wire together
  sibling files split by resource. Add a sibling module instead of growing a giant file.
- Prefer explicit code over derive-heavy convenience. Axum routes are built by hand, not with
  macros. Narrow protocols and formats are hand-rolled when a dependency would be worse.
- Keep behavioral constants in the module that uses them. Add a comment naming the vanilla source.

### Vanilla citations

Every copied formula, table entry, AI routine, or packet behavior cites the decompiled source, for
example `Main.cs:6342` or `WorldGen.cs` line ranges. State any deliberate change from vanilla in
the same comment. `just check-parity` content-keys Rust citations; `just check-doc-citations` does
the same for Markdown. After moving code or editing cited docs, rebuild the relevant index with
`just parity-update` or `just doc-citations-update` and review the diff.

### Generated data

Never hand-edit codegen output. The generated files are `recipes.rs`, `npc_drops.rs`,
`projectile_data.rs`, `banners.rs`, `buffs.rs`, `angler.rs`, `shimmer.rs`, `hurt_tiles.rs`,
`town_names.rs`, `travel_shop.rs`, `tile_death.rs`, `net_variants.rs`, `golf_physics.rs`, and
`tile_object.rs`, all under `crates/terrustia-proto/src/`. Change the generator and run
`just regen`, then review the diff. `tile_object.rs` also emits its hand-written struct and
functions from `crates/terrustia-codegen/src/tile_object.rs`; edit there, not in the output.

`npc_data.rs` and `placed_items.rs` are not generated. Correct them in place with a citation and
run `just check-npc-data` / `just check-placed-items`. Regression assertions that must survive
regeneration belong in `crates/terrustia-proto/tests/generated_tables.rs`.

### Dependencies

Hand-roll narrow, well-defined protocols. Take a dependency only when hand-rolling would be worse:
`argon2` (password hashing), `tempfile` (secure temp directory), `toml`, `tracing`,
`crossterm`, `axum`, `rust-embed`, `ureq`, and `flate2` with `zlib-rs`. Rationale for each is in
the root `Cargo.toml`; read it before adding or removing a dependency. Do not rewrite a verified
subsystem to shave crates.

### Editing and commits

- Edit files with a real editor. No `sed -i`, heredoc redirects, `>`/`>>` into source, or inline
  scripts that rewrite source. Generator scripts that produce checked-in source are fine.
- No em dashes in prose, docs, comments, or commit messages. Use hyphens, colons, parentheses, or
  separate sentences.
- Never add `Co-Authored-By` or any co-author trailer.
- Commit or push only when asked. If on the default branch, branch first. Keep messages factual
  and scoped.
- Bug fixes should start with a test that fails against the bug. Do not pin incidental behavior or
  implementation details in permanent tests.

## Important Files

| File | Why it matters |
|---|---|
| `crates/terrustia/src/main.rs` | CLI, config precedence, boot, task supervision, graceful stop |
| `crates/terrustia/src/game/server/mod.rs` | `GameServer` state, event loop integration, save, broadcast |
| `crates/terrustia/src/game/server/tick.rs` | Tick interval and phase order |
| `crates/terrustia/src/game/server/dispatch.rs` | Incoming packet `match` and handlers |
| `crates/terrustia/src/game/server/systems.rs` | Per-tick simulation systems |
| `crates/terrustia/src/net/listener.rs`, `net/connection.rs` | Accept limits and per-connection framing |
| `crates/terrustia/src/world/wld.rs`, `world/wld_save.rs` | World load and atomic save |
| `crates/terrustia/src/config.rs` | Config schema, env overrides, validation |
| `crates/terrustia-proto/src/id.rs`, `packets/`, `section.rs` | Packet IDs, typed codecs, tile sections |
| `crates/terrustia-client/src/lib.rs` | Client handshake and actions |
| `crates/terrustia/tests/support/mod.rs` | Test harness: free ports, scratch dirs, child reaper |
| `crates/terrustia/web-panel/tests/panel.spec.ts` | End-to-end panel verification |
| `justfile` | Canonical commands and data-check chain |
| `Cargo.toml` | Workspace policy, lints, dependency rationale, profiles |
| `terrustia.toml.example` | Documented config surface |
| `docs/generated-tables.md` | Table provenance and regeneration |
| `docs/release-blockers.md` | What is not done for v0.0.1; read before claiming readiness |
| `TODO.md` | Single backlog and roadmap |

## Runtime/Tooling Preferences

- Use the pinned Rust 1.97.1 toolchain. Do not change `rust-toolchain.toml` casually.
- `just` is the task runner. `cargo` works directly for individual commands.
- Web panel: `bun` only, no Node/npm. Build output is `crates/terrustia/web-panel/dist` and is
  embedded by `rust-embed` when the `embed-web` feature (default on) is enabled. Without that
  feature the panel is served from disk; this mode must still compile.
- Python 3 runs the packet audit and data checkers. `cargo-deny` checks licenses and bans. Fuzzing
  needs nightly and `cargo-fuzz`. Panel checks need Playwright Chromium. `cosign` is needed only
  for update tests and release verification.
- Configuration precedence is defaults < `terrustia.toml` < `TERRUSTIA_*` environment variables <
  explicit CLI flags. The config file defaults to `terrustia.toml` in the working directory; a
  missing file is valid. Every config field has an environment variable (see `config.rs`).
- Worlds live in a `worlds/` directory in the working directory unless `--world` or `--save`
  overrides. Admin state is `<save>.admin.toml`; audit and console history sit beside the world.
- The panel binds loopback only and is disabled by default. Do not weaken that validation.
- Docker images expect a prebuilt musl binary and run as a non-root user with `/data` as the
  volume; packaging manifests live under `packaging/`.

## Testing & QA

Test layers:

- Inline unit tests live in `#[cfg(test)] mod tests` beside the code they cover.
- Integration tests under `crates/terrustia/tests/` either run `GameServer` in process on an
  ephemeral port or spawn the compiled binary via `CARGO_BIN_EXE_terrustia` when process behavior
  matters (CLI, signals, restart, 255-player burst).
- `crates/terrustia-proto/tests/generated_tables.rs` holds regression tests for generated data.
- `crates/terrustia/tests/support/mod.rs` provides `free_port`, collision-safe scratch dirs, and
  `Reaper` for child processes. Use it instead of hard-coded ports or raw children.
- `fuzz/` is a separate workspace with `packet_decoders` and `section_stream` targets.
- Real-client verification tools are examples: `terrustia-client`'s `conform`, `verify`, `load`,
  `soak`, and `playthrough`; `terrustia`'s `probe`, `bestiary`, `stress`, `crowd`, `fuzz`, and
  `roundtrip_wld`. `conform` re-encodes a real server's own bytes and is the strongest check that
  encoder and decoder are not sharing the same misreading.

Commands:

```sh
just check                         # full local gate
just test                          # workspace tests
just test-filter gameplay          # filtered, with output
just check-panel                   # Playwright against the embedded release binary
just soak 60                       # real server and three clients
just fuzz section_stream 60        # nightly required
just check-data                    # needs .scratch/decompiled; local only
```

QA gates:

- `just check` runs `cargo fmt --all --check`, workspace clippy, clippy with
  `-p terrustia --no-default-features` (the disk-serving panel branch), `cargo deny check`,
  `tools/packet_audit.py`, the web build, and the workspace test suite.
- CI adds cross-target `cargo check`, host-native tests under `--no-default-features`, cargo-deny,
  packet audit, 60-second fuzz runs, and the 60-second soak.
- CI does not run `check-data`, parity, citation, or generated-table content checks because they
  need a decompiled tree. Run those locally before a release candidate.
- Run `cargo fmt --all` and `just check` before calling a change done.

Expectations and gotchas:

- Tests must be deterministic and isolated. Seed generation explicitly and prepare the tiles a
  test needs rather than depending on generated terrain.
- Use `support::free_port`; avoid fixed ports. `new_world_cli.rs` serializes generator-heavy tests
  with a mutex. `queue_capacity.rs` and the Playwright spec use fixed ports by design.
- `embed-web` is enabled by default. Panel tests require `web-panel/dist` to be built; without it,
  `build.rs` supplies an empty fallback and assertions fail.
- Unix-only tests (`world_switch`, signal handling) and Windows-only tests (`update_apply`) are
  marked at file level.
- Cosign-dependent update tests skip when `cosign` is absent or the transparency log is
  unreachable. Long-running helpers are `#[ignore]` and not part of the default suite.
- Use `TERRUSTIA_LOG` for diagnostics. Test-specific variables include `TERRUSTIA_TEST_WLD`,
  `TERRUSTIA_UPDATE_CHECK_ENABLED=false`, and `TERRUSTIA_UPNP_ENABLED=false`; filesystem tests
  redirect `HOME`, `XDG_DATA_HOME`, or `USERPROFILE`.
