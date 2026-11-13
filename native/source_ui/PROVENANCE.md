# Original movie runtime provenance

The Rust adapter is GPL-3.0-or-later. The interpreter and GPU backend are Ruffle
0.7.1, MIT OR Apache-2.0, based on upstream revision
`1d7cf3b68281a3b857bdac8e247d346e76aba685`. Applicable notices are retained in
[RUFFLE-LICENSE.md](RUFFLE-LICENSE.md).

All four Ruffle dependencies use the immutable compatibility fork revision
[`86f1fc8e1425edeec9e79328d3d867d1490d713b`](https://github.com/Herbertofury/ruffle-external-cas/commit/86f1fc8e1425edeec9e79328d3d867d1490d713b).
Its source-file changes cover imported AVM2 movie/domain initialization,
class-only/per-instance placement, retained native function handles, visible VM
errors and SWF writing. The upstream base and each changed file's exact hashes
were verified before publication. No upstream contribution or endorsement is
implied.

Own-source AS3 fixtures cover named parent construction, class-only placement,
per-instance class isolation and retained callback/cross-runtime handle behavior.
The tiny SWF fixtures are compiled from that accompanying source. They contain
no installed game art, fonts, movies, scripts, creator content or user data.

The desktop app prepares sources and owns the interpreter on a dedicated actor.
It shares the existing wgpu instance, adapter, device and queue, presenting an
RGBA8 premultiplied transparent texture without production CPU readback.
`--source-ui-root MOVIE --source-ui-locale ll_cc --ui-assets PACKAGE
--source-ui-strings STRINGS_PACKAGE` is an
explicit compatibility inspection path while original control bindings and
movie initialization remain incomplete. It records VM/unbound-service failures
and returns a failed diagnostic result rather than declaring full UI support.

The full source dependency graph, source provider priority, localization,
Scaleform extensions, widget placement/lifecycle, keyboard/IME behavior and
native service protocols remain qualification gates. Original source button
actions must retain their meanings: CAS's accept triangle is not pose playback.
Unsupported host services never fabricate Sim/catalog/account data or mutation
success. Installed original resources are read locally and are never bundled.
