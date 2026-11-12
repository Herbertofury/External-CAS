# External CAS source publication boundary

The owner selected https://github.com/Herbertofury/External-CAS as a new public
repository with a wiki. It is separate from the recovered TD1/Apex history,
CAS-Helper and Sims4Foundry.

The public source consists of the reviewed `native/` crates, the native-only
Cargo workspace and lock, build/qualification scripts, project documentation
and applicable license notices. Source promotion must use the Git-visible
allowlist, verify the destination remote, and compile/test the exact copied
tree before committing or pushing it. A source checkpoint is not a release.

Never copy `.local`, private manifests/evidence, game/CC packages, extracted
textures/fonts/geometry, Sim/Tray files, portraits, screenshots, proprietary
vendor binaries/decompiled code, credentials, unrelated Apex source/history,
build products or caches. Authorized installed resources are read locally by
the native app. No resource permission implies public redistribution rights.

Source-derived implementations retain GPL-3.0-or-later provenance and their
specific pinned references in each crate. The session crate remains MIT; the
combined application is GPL-3.0-or-later. Dependency license and corresponding
source checks remain required before a distributable or release is published.

Product acceptance is tracked in `EXTERNAL_CAS_ACCEPTANCE.md`, the public wiki,
milestone 1 and issues #1–#4. An unqualified debug build must remain plainly
labeled as development and must not claim complete CAS, game compatibility or
performance superiority.
