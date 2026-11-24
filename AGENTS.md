# Repository instructions

Read `.agents/foundry/AGENTS-adapter.md` for the pinned shared engineering core.
Keep this file and nested instructions project-owned: setup, commands, constraints and acceptance belong here.

## External CAS local contract

Read `README.md`, `docs/ACCEPTANCE.md`, `docs/VALIDATION.md` and `docs/RECOVERY.md` before implementation. This native Rust Windows project is a source checkpoint, not an accepted installer or complete CAS replacement. Existing unchecked acceptance gates remain unchecked until direct current evidence proves them.

- Keep the application independent from Apex/hybrid features and from a running game; CAS/Live synchronization is a separate explicit revision-checked boundary. Never use a reserved Apex session or unrelated restoration records.
- Preserve actual installed source UI/assets, complete appearance and unsupported state. Proprietary game resources, private Sims/CC/fixtures, accounts and source receipts with personal paths are not public fixtures.
- Use `.agents/skills/native-cas-acceptance/SKILL.md` for native editing, source fidelity, Tray writes, synchronization and release work.
- Windows workflow pins the Rust toolchain and runs `cargo test --workspace --locked --jobs 2` then `cargo build -p external-cas --locked --jobs 2`. Native dependencies and a real Windows/game environment are separate prerequisites; do not claim Linux fixture checks establish Windows or game acceptance.
- Keep shared-core sync separate from product acceptance. A documentation-only guidance merge never closes the full runtime, licensing, packaging or publication gates in the acceptance checklist.

## Mandatory pre-release optimization gate

Before finalizing every release, follow the owner-mandated [Athena zero-loss pre-release optimization standard](https://github.com/Herbertofury/Athena/blob/main/guidance/PRE_RELEASE_OPTIMIZATION.md). Evaluate shrink-anything, fast-anything and rust-anything using reviewed pinned tools in isolated worktrees; run applicable passes, compare equivalent complete workloads, keep only independently verified improvements, restore rejected changes, and record per-tool evidence or explicit inapplicability/blockers. Never weaken tests, delete required content, force a Rust rewrite or regress functionality, quality, portability, compatibility, security or protected performance metrics. Data/documentation-only releases and existing native Rust paths require honest applicability decisions, not artificial ports. Existing local and pinned governance requirements remain intact.
