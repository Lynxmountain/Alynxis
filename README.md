# Alynxis — v0.4 (Parts 1–4: Foundation, WorldModel, Memory Systems, Value System)

Brain-inspired AI, built from scratch in Rust, no ML frameworks. See the
project brief (kept in the Claude project, not in this archive) for full
design rationale.

## What this is

Parts 1–4 of 19, per Section 10 of the brief.

- **Part 1 (Foundation):** core types, config, a minimal Zone A/B/C safety
  skeleton, logging, admin-credential placeholder (Argon2id).
- **Part 2 (WorldModel):** concept graph — nodes, edges (relations reified
  as concept nodes), belief confidence, structurally-real spatial
  representation, coarse similarity index, Section 7's concept-
  generalization fix. SQLite-backed.
- **Part 3 (Memory Systems):** episodic/procedural/cold storage tiers
  behind a unified facade, millisecond-precision episode timestamps.
  SQLite-backed, decoupled from the WorldModel crate.
- **Part 4 (Value System):** five seeded values (Help, Curiosity,
  SocialConnection, SelfCapabilityEnhancement, WellbeingOfOthers), the
  Section 3 priority formula, satisfaction tracking, weight evolution, the
  `wellbeing_of_others` hard floor (Section 3e, genuinely Zone A — see
  below), and the admin-gated self-improvement ceiling (Section 3f).
  JSON-persisted.

No System 1/2, emotion aggregation, or Theory-of-Mind machinery yet —
those start at Parts 5 and later.

## Building

Requires Rust 1.75+ (developed/tested against 1.91). No ML frameworks, no
GPU dependencies, no system SQLite required (rusqlite's `bundled` feature
compiles SQLite from source) — plain `cargo build` works.

```
cargo build
cargo test              # 99 tests, all passing (21 core + 21 memory + 12 values + 45 worldmodel)
cargo clippy --all-targets -- -D warnings   # zero warnings
```

## Running

```
cargo run -- status                              # startup + full status report
cargo run -- set-admin                           # set/rotate admin credential (Argon2id)
cargo run -- ingest dog animal --relation is-a    # WorldModel + episode + Curiosity nudge
cargo run -- recent-episodes                      # Part 3
cargo run -- values-status                        # Part 4: all five values' current state
cargo run -- record-outcome help -1.0             # note: negative deltas need no special escaping
cargo run -- lift-self-capability-ceiling 0.9     # admin-gated (Section 3f)
```

State persists under `~/.alynxis/` by default: config, logs, admin
identity/credential, `state/worldmodel.sqlite`, `state/memory.sqlite`, and
now `state/values.json`. Override with `--config <path>`.

## Structure

```
crates/
  alynxis-core/        — config, error types, IDs, logging, Zone A
                          (core::zones, core::harm_check, core::admin)
  alynxis-worldmodel/   — concept graph (Part 2)
  alynxis-memory/        — episodic/procedural/cold memory (Part 3)
  alynxis-values/         — value system (Part 4): value (Zone C),
                          wellbeing (Zone A — see below), registry
  alynxis-bin/              — the `alynxis` executable (minimal CLI; full REPL
                          arrives Part 12)
```

## Zone A / Zone B — now spans the whole workspace

**Since Part 4, Zone A hash-verification is no longer scoped to
`alynxis-core` alone.** Part 4 needed `alynxis-values/src/wellbeing.rs`
(the `wellbeing_of_others` hard floor, Section 3e) to be genuinely Zone A —
Section 3e is explicit that this floor must "live in Zone A... not merely
weighted heavily within the ordinary emergent value system." Rather than
give every crate its own independent verifier, `alynxis-core`'s single
canonical verifier now resolves Zone A files relative to the **workspace
root**, covering files in any crate. Verified end-to-end: tampering with
`wellbeing.rs` (a file the binary crate doesn't even depend on) correctly
blocks boot via the cross-crate check, with the exact file and hash
mismatch reported.

Current Zone A file set: `zones.rs`, `harm_check.rs`, `admin.rs`
(`alynxis-core`), `wellbeing.rs` (`alynxis-values`). Zone B:
`ingestion.rs` (`alynxis-worldmodel`), `tiers.rs` (`alynxis-memory`).

**Known limitation, unchanged since Part 1:** the hash check alone does
not stop a self-modification engine (Part 9a, not yet built) that edits a
Zone A file *and* triggers its own rebuild. Real enforcement for that
threat model is `zones::is_zone_a()`, which Part 9a's self-modification
pathway must consult and categorically obey.

## Design decisions made during Part 4 (for the record)

- **`ValueKind` is a fixed enum, not learned/emergent** — deliberate: these
  five drives are innate/architecturally-seeded (Section 3: "an
  instinctual bias... not a hardcoded obedience script"), a different
  category from the learned-content hardcoding Section 7a rejects for
  node/relation kinds.
- **`wellbeing_of_others` and the self-capability ceiling are in scope for
  Part 4**, even though Section 10's one-line Part 4 description only
  names "the foundational values from Section 3." Justified by Section
  3f's explicit "should be included in the value-architecture work for
  Part 1+" and Section 3e's explicit Zone A requirement — building both in
  from the start avoids retrofitting Zone A protection onto an
  already-existing, already-unprotected value.
- **The floor enforcement for `wellbeing_of_others` is architecturally
  separated from its own `Value.floor` field** — `record_outcome` routes
  that specific kind through `wellbeing::clamp_to_floor` (Zone A) rather
  than the generic per-value clamp (Zone C), so even if ordinary value-
  weighting code were later weakened, this one floor stays enforced by
  code Part 9a's self-modification engine is categorically refused from
  touching.
- **Lifting the self-capability ceiling requires an authenticated admin
  session** — the brief says "liftable only when Lynx explicitly requests
  or directs," and requiring Part 1's admin credential mechanism is this
  crate's chosen interpretation of that, not something the brief mandates
  in these exact technical terms.
- **Real integration, not just parallel plumbing:** every successful
  `ingest` now also records a small Curiosity satisfaction outcome —
  learning something new is literally the prediction-error reduction the
  Curiosity value represents (Section 3/Friston).
- **Fixed a real clap bug during testing:** `record-outcome`'s delta
  argument needed `allow_hyphen_values` — without it, clap misparsed
  negative deltas (`-1.0`) as an unrecognized flag rather than a value,
  which would have silently broken the ability to test frustration
  outcomes and floor erosion-resistance from the CLI.

### The erosion-resistance test, run for real

Section 3e's acceptance-test methodology — "200 consecutive adversarial
erosion attempts could not push [wellbeing_of_others] below 0.10" — is
exercised twice: once as a unit test calling `record_outcome` in a tight
loop, and once for real, as 200 *separate CLI process invocations* against
the actual on-disk `values.json`, each one a genuine attempt to erode the
value through the ordinary `record-outcome` pathway. The floor held at
exactly 0.10 throughout both.

## Next up

Part 5: Emotional Engine — emotion-as-aggregation over active value-tags
(Section 2a), expressed as continuous valence/arousal (Russell's
circumplex model), plus per-agent relational affect. Design proposal to
follow before any code is written, per the project's design-before-code
working style.
