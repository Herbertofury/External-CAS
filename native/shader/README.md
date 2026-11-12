# Original installed shader assets

This Rust-owned reader resolves DATA/PRECOMP v6 effect/technique/pass/state
tables, original raw-Snappy stage streams, global parameters, texture bindings
and constant-buffer preparation records from an explicit local installation.
It validates bounded DXBC containers and preserves every unknown record word.
It does not load a game executable or choose a runtime shader permutation.

No original shader bytecode, disassembly or game binary belongs in this crate or
a distributed package. A renderer may request the original installed shader
after source, stage, feature selection, bindings and pass-state validation.
Source defaults are recorded as raw bits; they do not supply CAS runtime state.

Format facts were independently implemented from actual installed bytes and
the maintained [PRECOMP research](https://github.com/Demeterio/TS4-Animation-Browser/blob/38598aaf4142b93d46bc452fde088e962cad3372/modding-resources/precomp/guides/FORMAT_REFERENCE.md).
Its older-build corpus does not qualify a newer installation automatically.
Raw Snappy follows [Google's format description](https://github.com/google/snappy/blob/main/format_description.txt).
No upstream parser/template implementation or proprietary code is vendored.

The current installed library has 77,771 effect records, 7,947 vertex shaders,
30,009 pixel shaders and four compute shaders. Native extraction of the source
CAS normal candidate matches the independently decompressed DXBC SHA256.
Controlled execution of the original shader outside the game independently
qualifies the corresponding composition/normal/lighting arithmetic; it does
not qualify actual material-plane construction, SRV color space or live feature
selection. See [the engine implementation map](../../docs/engine/ENGINE_CAS_IMPLEMENTATION_MAP.md)
and [the shader contract](../../docs/engine/SIMSKIN_DX11_20261112.md).

```powershell
cargo test -p external-cas-shader --locked
cargo run -p external-cas-shader --example inspect_precomp -- 'ABSOLUTE_PRECOMP_PATH' PS_REFERENCE
```

The example prints private source receipts and metadata, never original shader
bytes. It is diagnostic tooling; the final native renderer integration and its
complete-frame comparison gates remain open.
