# Native CAS implementation map from installed sources

This map follows the actual installed CAS dependencies and names the remaining
behavior that must be implemented. It does not equate a format reader, a UI
layout, a source-room mesh or a successfully reopened file with complete CAS.
The qualified installation is observed as1.129.39.1030. Static inspection is
read-only; the reserved active game/Apex session has not been contacted.

## What can run in the external Rust app

The practical source-native boundary is **installed assets and specified
behavior**, with Rust owning discovery, current Sim state, input, undo, asset
construction, renderer/animation orchestration and persistence. Actual source
movies/atlases/fonts, model/rig/texture/tuning resources and original compiled
shaders can be resolved from the selected installation rather than shipped.
No generic backdrop, body, gallery art or random pose is a completion substitute.

Original pixel shaders now execute successfully in an offscreen D3D11 harness
outside the game. [Native shader loading](../../native/shader) reads the actual
PRECOMP/DXBC boundary in Rust. The
[shader record](SIMSKIN_DX11_20261112.md) identifies exact source families,
resources, register contracts, normal conventions and verified CAS equations.
This is a concrete reuse route that retains Rust ownership of rendering/editing.

Hosting the whole installed game client is a different and unqualified route.
Static PE inspection finds six TS4 client exports, none a standalone CAS editor
entry point. Simulation_x64.dll exports 47 entries, including`PyInit__CAS`,
`dllInit`, `load` and shutdown/autosave lifecycle functions. Its `_cas` module
operates on native SimInfo objects containing internal C++ pointers. No
documented, independently initialized headless-CAS/renderer ABI was found.
Absence from exports does not prove impossibility; it means an arbitrary DLL
load or function call is not an accepted compatibility design. It also does not
establish permission to redistribute or adapt proprietary implementation code.
Source-linked math/data contracts, compatible open-source components and local
installed asset access remain distinct from a new license grant.

## Launch, source discovery and generation ownership

The app must first bind an explicit installation/profile/mod scope. Resource.cfg
and profile options define resource coverage/priority; package order alone is
not game load order. Equal-priority duplicate providers and instance-only
outfit CASP references need explicit ambiguity handling. Imported projects pin
the selected provider/fullTGI/ordinal/index/payload rather than silently taking
a later file with the same visible name.

[assets](../../native/assets) already retains complete unsigned keys, duplicate
providers, source/index identity, cancellable bounded decoding and payload
pins. [scene](../../native/scene) resolves declared dependencies and configured
priority. [catalog](../../native/catalog) publishes only complete package
metadata atomically and qualifies indexed query scopes; it is not the source of
truth for edited native payloads. Read-only real-package qualification exposed
and fixed the trailing DBPF Unknown2=0 incompatibility: treating it as a
mandatory committed flag had wrongly rejected an installed room's zlib assets.

The next edit/preview request must carry its Sim/form/outfit identity, draft
revision and source generation. Completed work for an older selection must not
publish into a new one. [link](../../native/link) contains these per-Sim context,
revision, echo/conflict and durable checkpoint contracts; source game activation
must never hijack standalone mode or a dirty external draft.

## Source UI to actual editing commands

Installed CASNavigation, CASLibrary, CASCustomizer and ClassLibraryGamedata
movies are not merely image sheets. They contain imported classes, display-list
timelines, state transitions, focus/hit regions and native service requests.
[fidelity](../../native/fidelity) reads/pins real movies, fonts, atlases and a
limited source display-list subset. Its source-symbol comparisons and actual
font/atlas rendering are useful component evidence; six source navigation
buttons do not establish complete CAS UI behavior.

The retained CASLibrary ActionScript research exposes the Olympus boundary:
`SendGameMessage(name,args)`, `CallGameService(name,args,async)` and
`ObjectSendMessage(target,name,args)`. Inbound callbacks route to UI messages
and services. Widget initialization/name registration and on-demand stored
messages participate in initialization order. This retained export came from
the owner's supplied UI baseline and must be checked against the original
installed movie before claiming an untouched EA command map.

Scaleform extension classes are an important root cause: their decompiled
ActionScript bodies can be intentional native stubs. For example text/GFX
extension APIs may appear to run but return false/empty values or perform no
work without their native bindings. A generic ActionScript VM alone is
insufficient. The ongoing Rust Ruffle experiment must qualify import loading,
ExternalInterface, FSCommand and each actually used GFX extension. The service
boundary then needs typed Rust handlers and response/state messages connected
to the real native draft. Unimplemented commands must remain explicit; drawing
the source button while ignoring its request is not source-native behavior.

Identity, voice, pronouns, gender options, traits, aspirations, sexuality,
relationships, household selection, undo/redo, randomization, genealogy,
occult forms, catalog search/filter/swatch/outfit tabs and direct manipulation
must all have bidirectional state. The same-name Sim case requires exact IDs.
Controls cannot infer AgeGender bitfields from ordinals or reuse CASP gender
encoding as native Sim gender. Real controller contracts are still required
for many of these actions; source text labels do not implement their semantics.

## Native Sim state, genetics and generation

The installed `_cas` module exposes 35 native methods, reconstructed statically
from its actual PyModuleDef/PyMethodDef tables. Names, flags, image hashes and
selected argument metadata are retained in
[the boundary inventory](evidence/native-cas-boundary.json). Relevant families
include part replacement/removal and compatibility, random color/part/outfit
selection, offspring, household generation, occult conversion, age changes,
native override operations and genealogy graph mutation.

For example`generate_offspring` has explicit parent_a,parent_b,offspring and
seed keyword names. This identifies an actual native genetics operation,
not a request to pick random hair and skin. `set_caspart` has explicit native
color-shift and layer fields as well as replacement/conflict/genetics controls.
Changing a CASP ID without evaluating conflicts, flags, compatible fallback
parts, hidden regions, layer metadata and genetic consequences is not complete
native part editing.

Installed SimInfo field names include skin tone/value shift, physique,
facial_attributes, outfits, genetic_data, pelt/custom texture, voice,
occult/current-occult types, traits and body-frame gender. The byte-preserving
[Tray appearance reader](../../native/tray/src/appearance.rs) supplies actual
imported state. Unknown protobuf fields and per-form data must remain intact.
Creation needs qualified native initialization/IDs/dependent defaults and
genetic/trait rules; a generic neutral body does not establish a new native Sim.
Native DLL methods are research targets, not calls currently authorized against
the active game or proven usable as an isolated foreign-function library.

## Geometry, morphology, skinning, regions and LOD

The actual imported outfit selects CASP full-TGI candidates, their complete
reference tables, GEOMs, region maps, texture roles and native rig. Source GEOM
positions/normals/tangents, UV0/UV1, weights/joints, bone palette, vertex IDs,
tags and ordered stitch records all matter. BGEO uses stable IDs across seams;
DMAP consumes UV1 and the first matching UVStitch list, plus native tag weights.
Missing IDs cannot be replaced with generated indices. Unknown stitch tails
cannot silently become an empty seam map.

[morph](../../native/morph) and
[character deformation](../../native/character/src/deformation.rs) recover the
source-reference BGEO→DMAP→BOND path and report qualification. Current native
modifier tuning supplies scale/offset/context and dampening data. Complete
direct manipulation still needs selection regions, limits, elastic response,
symmetry, modifier blending, physique/pregnancy/age/frame transforms, normals
and tangent deformation and consistent clothing behavior. Parsing a modifier
does not prove the native order or the visible body result.

[RMAP](../../native/character/src/region.rs), CASP exclusion masks and part
substitution govern mesh visibility. Shader cutout/transparency is a separate
material operation; geometry region removal must not substitute for alpha
testing. Source LOD thresholds and camera context determine LOD selection, not
an unconditional LOD0 render. Deformation and pose resets must restore the same
native rest data and preserve parts that the app cannot yet evaluate.

## The full material construction is an immediate fidelity blocker

The source-named CAS family requires independent OverlayTexture,
SkinToneAndMakeupTexture, ClothWithAlphaTexture, SkinDetailHSL lookup,
CASAmbientIntensityTexture and optionally normal/specular/reflection/sheens.
Actual normal decoder pixels and controlled shader equations are now qualified.
The currently recovered reference-tool atlas is not the engine's complete
composite contract. Feeding it as every mesh's common diffuse can cross colors
between UV regions or apply skin/detail color operations twice.

The next work must trace the native construction/selection of each plane from
TONE, age/sex/physique detail overlays, sculpt overlays, CASP texture-space and
composition/layer ordering, shadow/makeup/tattoo/skin details, tan/pelt/custom
painting and packed color shifts. Installed shader parameter names identify
bound **variables**, not necessarily existing single-file texture resources.
No verified resource selector for a global lookup/ambient plane means the app
must not invent one or use a neutral image while claiming native fidelity.
Per-mesh normal/specular transforms and UV1 ambient addressing must remain
separate. Sampling space/texture formats, alpha masks, blending, pass selection,
sheen/reflection and output color still need actual evidence.

Use original local shaders or independently verified Rust/WGSL ports with the
same qualified inputs. The [shader record](SIMSKIN_DX11_20261112.md) explains
both routes and the actual D3D11On12/wgpu ownership requirements. The user wants
the original CAS behavior; similarity-based lighting calibration does not
close the missing material construction and pass-state contracts.

## Actual room, camera and lighting

[room](../../native/room) traces the selected CAS world→OBJD→MODL/MLOD chain,
packed VRTF buffers, MTST state/variant alternatives, MATD, SKIN, texture
references and configured replacement scope. The configured replacement used
for current investigation contains FullBright, translucent glass, flat mirror
and drop-shadow materials, plus a cubemap dependency. Treating those as a
generic textured background would discard real room behavior.

Its blend attributes, SKIN palette and MLOD joint-reference data currently expose
an unresolved engine palette binding. Null OBJD Rig and identity inverse-bind
matrices are not sufficient proof to ignore that metadata. Diagnostic source
bind-space rendering remains explicitly separate from an accepted app room.

Actual camera/light tuning has independently matched nine resolved source
trees / 476 scalar-vector cells, four camera contexts, 34 regions, 17 pose overrides
and five lighting profiles. Source values are inputs, not projection equations.
The newly recovered CAS shader independently confirms two key lights, two rim
colors/falloffs and green/alpha-driven ambient texture behavior. Camera anchoring,
transitions, autozoom, direct-edit regions, per-species/age/occult context,
near/far/FOV and coordinate conversion need their source runtime conventions.
See [fidelity source custody](../fidelity/CAS_FIDELITY_20261112.md).

## Idle, posing, animation and interaction

The current native [animation](../../native/animation) pipeline evaluates actual
RIG3.1 and CLIP14/current18 skeletal channels, retaining unknown/event/IK data.
The selected real CAS idle removes the generic A-pose in the current fixture.
The native ASM/tuning state selector is separate work: age/species/frame/form,
selected region, manipulation, try-on replay, facial control, walk preview and
sequence transition requests must select the correct assets and transitions.

Sampling one CLIP does not implement the CAS state machine. Event actions, IK
targets/chains, expressions/control tracks, interpolation and root transforms,
clothing/morph interactions, timing and reset remain qualification boundaries.
Preserve-and-ignore preview policies are explicitly partial and must remain
visible. No game event may be triggered merely by previewing an unsupported
sequence in the external app.

## Native persistence and the separate game boundary

[Tray](../../native/tray) retains wrappers, native wire fields, unknown data and
portraits. Its draft writer supports specific qualified field edits with
revision/undo and disposable save-as projects. A native-looking project bundle
is not yet evidence of game-accepted new Tray creation. Each supported edit
requires exact before/after wire comparisons, readback and same visible result
after closing/reopening. Replacing native files requires backups, identity and
concurrent-change checks, validation and recoverable commit publication.

Standalone, CAS Sync and Live Link are explicit different modes. Imported Sim
selection, game launch and household changes do not authorize discarding a
draft or silently applying an edit. The narrow game adapter reads actual native
SimInfo/outfit state and preserves raw packed 64-bit color shifts. All live writes
remain refused until an exact game-build/adapter/field qualification exists.
Apply must review an exact target revision, send an authenticated bounded
operation, read back/reload and only then accept success; timeouts are
indeterminate rather than automatic retries. Rollback requires a known
preimage and no newer state. The reserved active game cannot be used to
manufacture those qualifications.

## Completion evidence follows the dependencies

The shortest useful next result is correct source plane construction and CAS
permutation/SRV/pass selection feeding the verified shader, followed by the
same actual Sim/source room/camera/idle comparison. Source UI execution must
then reach real draft actions and response state. Native creation/genetics,
complete edit history/Tray writes and separate CAS/Live readback qualification
remain required behavior, not optional parser coverage.

The [acceptance record](../EXTERNAL_CAS_ACCEPTANCE.md) must stay open until the
independent external clean-launch→real import/CC→edit→pose→save→close→reopen
workflow and disposable game verification pass across representative bodies,
outfits, tones, CC, missing fields, failures and concurrent state. Windows
packaging, clean-environment installation/upgrade/recovery, licenses/checksums
and measured actual-app performance follow the same exact qualified build.
