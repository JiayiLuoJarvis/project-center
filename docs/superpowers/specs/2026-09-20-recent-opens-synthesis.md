# Recent opens. Synthesis

## Base

Candidate A (`/tmp/arena-pcs-recent/candidate-a/`). Thin `persist/recent.rs`, `Mode::RecentPicker`, record after `wait_spawned` `Ok`, no `App`-held list.

## Grafts from candidate B

- Single `launch::launch(data, project, group, option) -> Result<i32, String>` that runs `spawn_direct`, then `wait_spawned`, then `record_recent` on `Ok`. Migrate TUI `do_launch` and CLI spawn sites to it so no call site can forget to record.
- Keep `spawn_direct` / `wait_spawned` public enough for tests that do not want recording. Prefer `pub(crate)` with a test-only path, or leave them `pub(crate)` and only route product call sites through `launch()`.

## Rejected from B

- `domain::RecentList` newtype. `push` invariants are enough in persist. Extra aggregate without a second consumer.
- Making `spawn_direct` private in the same PR if that forces large test churn. Prefer routing product callers first.

## Cross-judge

Parent judgment (no separate judge spawn). Agreement with A on `recent.json` sidecar and Mode popup. B wins only on the choke-point record rule.

## Verification plan

`cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, plus unit tests named in candidate A Verification section.
