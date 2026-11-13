# Original installed shader assets

This Rust-owned reader resolves DATA/PRECOMP v6 effect/technique/pass/state
tables, original raw-Snappy stage streams, global parameters, texture bindings
and constant-buffer preparation records from an explicit local installation.
It validates bounded DXBC containers and preserves every unknown record word.
It does not load a game executable or choose a runtime shader permutation.

`named_set_count`/`named_set` retain source-named shader families, mode pairs,
the original feature-program words/hash, variant fields (including unaligned
names), raw record words and parsed metadata prefix. Following parameter/heap
metadata remains explicitly unparsed in the pinned library. Retained program
hashes are checked again before returning `NamedShaderSet::feature_program`.

`FeatureProgram::evaluate` accepts explicit renderer IDs, material feature
hashes and the source caller's wildcard-empty-list flag. It preserves the
installed interpreter's rotating32-bit membership, boolean operations, forward
branches,32 field accumulators and wrapping sum. It rejects malformed bounds,
stack overflow/underflow, operand-word jump targets, cancellation and native
failure opcodes. It does not infer those inputs from convenient texture flags.
The current engine's material/renderer input producers remain a separate
production selection gate.

The compiled Rust evaluator matches1,384 controlled inputs across all173
installed named sets against isolated emulation of the original current
wrapper/evaluator instructions:854 exact masks and530 explicit rejections.
There is no native game process or I/O in that comparison. The only intercepted
intrinsic is bounded stack-buffer memset. Proprietary program words and machine
instructions stay in the ignored local evidence; see the ignored acceptance
test and [functional interpreter record](../../docs/engine/SOURCE_FEATURE_VM_20261112.md).

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

`vertex_bindings` preserves the original packed register/index/usage triples
and stage-specific constant preparations. The triple count is a byte extent;
semantic names use the source consumer's low-nibble domain, independently of
the feature interpreter's renderer IDs. Unknown flag bits remain retained.
The actual VS924 case matches all eight triples, three buffers and 23 members
against independent source inspection, including every default bit. See
[the paired-stage contract](../../docs/engine/ORIGINAL_VERTEX_CONTRACTS_20261112.md).
