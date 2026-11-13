# Actual installed shader feature programs

The native [shader crate](../../native/shader) now evaluates the actual source
feature programs using a compatible, bounded Rust implementation. This is a
qualified interpreter contract, not a claim that the app has reproduced every
live renderer input or selected a complete game pass.

The qualified source is installed build1.129.39.1030. Client SHA256 is
`95e40b75f76711c66b9f36a974ec04d43304a6f591d409d3d4019818f366237b`;
PRECOMP SHA256 is
`62ffab0f85c3af8ace5a375c968216d1ada2b1c9c2d18f32aa4b25d250a0b101`.
Static wrapper RVA01372260 forms material membership and tail-calls evaluator
013724B0. Proprietary instructions and original program words remain private.
The game was not loaded or contacted.

## Source structure and preserved data

The PRECOMPv6 root's field-relative pointer at+0x2C and count at+0x30 identify
173 named-set records of8 bytes. Their second word remains raw, without an
invented size meaning. A pointed record contains its padded set name, mode
hash pairs, program word count/words and variant fields. Variant names are
unaligned; their declared lowercase FNV1 hashes all validate in this source.
Following parameter/state/heap metadata is explicitly unparsed and remains
in the pinned library. The API retains the exact parsed prefix, source offsets,
unknown record words and program hash; it never reserializes original bytes.

Within the program itself the structure is renderer-ID count/list, material
hash count/list, then instructions. There is no additional instruction count
between those lists and the first operation. Empty programs return mask0.

## Functional semantics

Material membership is computed against the caller's explicit hash list. Its
bit rotates within a32-bit word for each source-declared material hash, so
declarations beyond32 alias that bit position exactly as the native wrapper.
Instructions use the low16 bits as opcode and high16 as operand:

- `1` combines the two top boolean values with AND; `2` combines them with OR.
- `3` negates the top boolean value.
- `4` skips forward by the operand word count if false; `5` skips if true. Both pop.
- `6` pushes a material membership bit, with the native32-bit index semantics.
- `7` pushes membership of the operand ID in the caller's renderer-ID list.
  Only the explicit native wildcard flag combined with an empty caller list
  makes every renderer ID present.
- `9` assigns the next word to one of32 field accumulators; `10` adds it, wrapping.
- `11` skips forward by the operand word count.
- An exactly zero word halts. The returned mask is the wrapping sum of all32
  fields. Opcode8 and other unhandled opcodes take the native failure path.

The Rust boundary additionally refuses out-of-bounds headers/operands/targets,
branches into field-immediate words, boolean stack underflow/overflow, explicit
resource budgets and cancellation. Raw unknown words remain available even
when evaluation refuses them. A native failure is never treated as maskFFFFFFFF
or silently replaced with a convenient candidate.

## Independent qualification

An isolated Unicorn2.1.4 harness follows the actual original wrapper/evaluator
instructions in read-only emulated memory. It permits no native calls, imports,
game entry point or I/O. Its sole intercepted intrinsic is bounded memset of
the evaluator's stack accumulator array. Controlled inputs cover every one of
the173 installed named sets, empty lists, the explicit wildcard, complete lists
and deterministic mixed membership lists. All1,384 results match the compiled
Rust implementation:854 exact masks and530 explicit rejections. The source
contains137 distinct program arrays. The harness executes793,080 instructions
and reaches185 distinct instruction addresses. Rejections are retained evidence;
the meaning of those invalid input combinations is not guessed.

Default tests cover source offsets/unaligned fields, retained program mutation,
branch/immediate bounds, cancellation, budgets and exact wrapping/membership.
The real-source acceptance test is explicit and ignored by default:

```powershell
$env:EXTERNAL_CAS_SHADER_SOURCE='ABSOLUTE_INSTALLED_PRECOMP_PATH'
$env:EXTERNAL_CAS_FEATURE_VM_RECEIPT='ABSOLUTE_PRIVATE_ORIGINAL_VM_RECEIPT'
cargo test -p external-cas-shader installed_programs_match_isolated_original_machine_vm -- --ignored --nocapture
```

The diagnostic `feature_reference` JSON example accepts caller-supplied words
and lists without accessing game files. No source program array is shipped as
a fixture. [Sanitized evidence](evidence/source-feature-vm.json) records hashes,
counts and qualification boundaries.

## Next caller boundary

Material producer RVA011FCA80 gathers flagged material parameter records,
global/injected hashes and renderer context before evaluating the named set.
Renderer-ID producer01372700 converts up to20 native vertex-declaration entries
into IDs. Source usage0,1,2,3,4,5,6 maps to feature domain0,1,6,3,4,2,5;
the usage index occupies bits8 and above. Unknown usages are skipped. This
explains the named-set IDs as declaration capabilities, including extra indexed
streams. Material setup also removes particular declaration-ID categories in
one derived list. Exact call-context/declaration construction remains separate.
This
is already enough to reject a texture-presence heuristic as the complete input
contract. The exact producers and caller contexts still need qualification.
`FeatureProgram` therefore requires explicit lists and does not choose quality,
MTST alternatives, technique, pass order, source states or input targets.
Executing an exact program with invented caller inputs would still produce
the wrong CAS appearance.

## Native material feature-list producer

The actual constructor `011FD0E0` copies its input material bytes before
calling `011FD9F0`. That constructor consumes MTNF's header count at byte 12
and 16-byte parameter entries directly. The parameter's data-type LOW byte is
its native type; its HIGH16 bits are a required capability mask. A parameter
is admitted only when `requiredMask & ~allowedMask == 0`, using the current
global 16-bit allowed mask at `02975160`. Setter `011FD0C0` stores that mask
and getter `011FCDB0` tests requested bits. No quality preference is inferred
from a texture's existence. The source constructor consumes the parameter
count's LOW16 bits; unused/reserved bits remain data to retain and qualify.

It constructs 32-byte native parameter records. The feature-member bit is
bit 0 of byte `+0x1A`:

- Native types 1 and 3 scan float32 values and set the bit when an original
  `UCOMISS`/`JNE` test finds a nonzero value. This instruction sequence's
  unordered-value behavior must not be replaced by an ordinary NaN predicate.
- Type 2 scans integer values and sets the bit if any word is nonzero.
- Types 4, 5 and 6 set membership unconditionally, even with a zero image key
  or pointer. Their binding and resource availability are separate operations.

`011FCA80` gathers hashes only from records with that bit set, appends the
explicit global hash vector and caller list, then injects `DD3D121E` (`win32`)
unconditionally. Its explicit final caller flag adds `4A06F965`
(`ShadowCaster`). The same routine creates the derived renderer-ID list with
blend-index/weight capabilities removed. A source texture setter such as
`011FD410` can change an image binding without changing this feature bit.

This closes the parameter-membership mechanism, not its complete CAS caller.
The CAS material remapper merges authored parameters and injected globals
before this constructor. A feature list made only from authored GEOM MTNF
parameters would still omit that work. The effective capability mask, material
override chain, per-mode lists and complete source context remain explicit
production-selection gates.

[Sanitized producer evidence](evidence/source-material-feature-producer.json)
records the field contracts without original instructions or payloads.
