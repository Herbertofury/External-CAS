# Original PC UI host context

This source contract is qualified for installed client SHA256
`95e40b75f76711c66b9f36a974ec04d43304a6f591d409d3d4019818f366237b`.
It was recovered with read-only static analysis; no original native handler
was called and no game or profile was contacted. Original code stays private.

The source `IsGameClient` service registration is in `00DA0270`. At `00DA0533`
it passes callback `00DA3460` together with the literal `IsGameClient` at
`01FB27C0` to registrar `00DADA30`.

That actual PC callback writes both `usingController=false` and
`usingMouseKeyboard=false` unconditionally. These fields are source UI modes,
not device-availability detection. Inferring `usingMouseKeyboard=true` from
the presence of a Windows mouse/keyboard activates inappropriate console UI
paths in source code that combines these mode flags.

The same callback writes `isClient=true`, `isDebug=false`,
`razerEnabledPC=true`, `swapJPControls=false` and
`isFeaturePreviewBuild=false`. Its `graphicsAPI` field is the literal string
`DX11`, not an enum integer. `distributionPlatform` is an integer obtained
through native interface `00BB6CE0`, virtual method `+0x50`; its runtime value
has not been inferred from the executable path or a launcher name.

The field setters themselves corroborate types: `00DA6EE0` sets value type2
and the supplied boolean byte; `00DA6BA0` is the integer path and `00DA6C70`
the string path. The host adapter may reproduce the qualified source response
in its Rust-owned UI context. It must not invoke the original handler or copy
its native side effects into a game session.

[Sanitized evidence](evidence/original-pc-ui-host-context.json) records only
hashes, names, values and source addresses. Original ActionScript behavior and
the complete game-service ABI remain separate qualification boundaries.
