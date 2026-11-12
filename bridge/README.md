# External CAS compatibility boundary

This source is inactive until explicitly constructed and scheduled. It is not
installed in the game profile and has no startup/injection hooks. Do not point
tests at an active game. The Rust editor and link controller own editing,
revision/conflict rules, undo, checkpoints and Apply decisions.

`external_cas_bridge.protocol` matches the bounded authenticated Rust protocol.
`Sims4ReadAdapter` uses actual installed service/client/native appearance APIs
on its bound game thread. It reads partial appearance observations, preserving
the complete `save_outfits()` blob including unknown fields and exact packed
u64 color shifts. It does not rebuild an outfit or claim complete Sim/form
coverage. Unknown native values, missing identity, unsupported forms and all
writes fail explicitly. No Apply operation enters in-game CAS.

`GameThreadBridge` is a bounded dispatcher with detached requests. A supported
bootstrap must explicitly pair the peer, run socket work away from the game
thread, and schedule `pump` on the game thread. Its time budget stops starting
another request; it cannot interrupt a native getter already running. Such a
bootstrap and real-game read/write qualification are still outstanding; no
working game connection is claimed from fixture tests.

The compatibility audit reads installed bytecode signatures with the matching
Python interpreter without executing the code. Run it only against an explicit
authorized installed archive. The resulting native/link evidence contains
names/hashes, no EA bytecode or decompiled source. Source, observed APIs,
contract details and test commands are documented in `native/link/README.md`.

Tests under `bridge/tests` use deliberate local fixtures and never import Sims
modules. The standalone app does not require Python; this language boundary is
only for the game scripting environment. All bridge source is GPL-3.0-or-later.
