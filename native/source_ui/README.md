# Original source CAS UI host (experimental)

Rust-owned AVM/display-list/GPU adapter against Ruffle commit
`1d7cf3b68281a3b857bdac8e247d346e76aba685` (0.7.1, MIT OR Apache-2.0).
This adapter is GPL-3.0-or-later. It contains no installed game/creator movie,
font, atlas, script, package or user data. The small test SWFs are compiled from
the accompanying original test source and contain no game assets.

This is an integration seam, not complete Scaleform compatibility. Full original
CAS UI initialization still has explicit VM/native-service capability gates.
Do not replace generated data classes, suppress errors, invent valid Sim data,
or substitute authored panels to make the source application appear complete.

## App integration

On a read-only worker, prepare the explicit source package:

```rust,ignore
let aliases = locale_aliases("en_us")?; // caller-selected installed locale
let bundle = prepare_movie_bundle_with_aliases(
    &index, "CASNavigation", false, &aliases, expected_pins, &cancel,
)?;
// UIContainer + true includes all original PC CAS-state widget dependencies.
```

Missing sources, duplicate provider rows, stale expected pins, invalid hashes,
unknown font suffixes and unqualified atlas encodings fail explicitly. This
single-provider helper does not guess game-wide or Mods precedence. Use the
actual configured provider resolver when assembling a multi-provider bundle.

The runtime takes cloned handles from the existing eframe/wgpu 30 RenderState:

```rust,ignore
let runtime = SourceUiRuntime::new(
    SharedGpu { instance, adapter, device, queue,
                target_format: wgpu::TextureFormat::Rgba8Unorm },
    bundle, Arc::new(verified_snapshot_host), [physical_width, physical_height],
    pixels_per_point,
)?;
```

It creates no second device, window, OS-file fetcher or network transport.
`render()` returns `UiFrame { texture, view, dimensions, format,
premultiplied_alpha: true }`, a transparent non-sRGB RGBA8/BGRA8 intermediate.
Composite it once using premultiplied-alpha blending. An sRGB intermediate is
rejected instead of applying another transfer curve. No CPU readback occurs in
the runtime render path; only the diagnostic example explicitly reads pixels.

`tick(Duration)`, `input(UiInput)`, `resize([w,h], dpi)` and `needs_render()` own
source timing/input. UiInput is the pinned Ruffle PlayerEvent type. Pointer
coordinates are physical texture pixels; map eframe points through the same
pixels-per-point factor. The original movie owns its logical stage and layout.

The VM is thread-local. Preparation is separate from interpreter initialization.
Do not let large first-load interpretation block an interactive UI: keep that
phase visibly loading or own the runtime on a dedicated actor thread. Initial
script limits are configurable with `new_with_limits`; the default is two
seconds per script execution, 10,000 preload operations and a 4ms preload slice.
These are execution bounds, not a claim that every initial import is streamed.

## Real host boundary

`UiHost::call(&HostRequest) -> HostReply` is synchronous. Read only an immutable,
verified editor snapshot or execute an already-qualified bounded host service.
Never wait for, re-enter or call this VM from that callback. Queue longer work
and publish its verified completion through the original incoming messages.

HostRequest retains a host correlation ID, the original bridge kind, name,
arguments and asynchronous flag. `HostReply::Unavailable(reason)` records the
capability and returns Undefined, never fabricated valid user content or mutation
success. `status()` and `drain_events()` expose unbound services, VM failures,
denied resource fetches and any bounded diagnostic event overflow. Full GFx
support remains false.

`send_message`, `call_service` and `call_source` invoke only actually registered
original callbacks and return their real value. A missing callback is an error.
Original function/native-object arguments remain rooted VM `HostValue::Avm2Handle`
values rather than becoming Null. `call_retained_function` accepts only a valid
function handle from that runtime; cross-runtime handles fail.

Proven host contract facts: GetGameState is a uint (CAS = 983016380), and
CasGetCASEditMode is an int (ordinary full edit = 0). The original IsGameClient
reply is a platform capability Object that selects native-host-driven behavior;
without it UIManager explicitly switches to PLAY. Its graphicsAPI is a String,
not an integer. Legacy source identifiers include D3D, DX11, OGL, METAL and GNM.
The distribution platform is an int with known values 0/1/3/4; no source Unknown
entry has been established. Do not silently identify this app as an EA/Steam
account. The diagnostic example reports its actual DX12 backend and unsupported
standalone commerce context, and contains no simulated Sims/catalog responses.

## Qualification

- Actual original CASNavigation alone renders original artwork/font resources
  on a single shared RTX 4090/DX12 device with transparent pixels. It avoids the
  GameplayStateData initialization path. The original LocKey Function is now
  bound before first frame to exact installed STBL literal resolution; original
  CommunicationManager construction supplies the GalleryInfo context.
- The complete explicit English CAS dependency graph prepares 71 original
  movies, nine fonts and 2,038 source sub-images. This does not mean every widget
  initializes or every source control has a native backend.
- Own-source AS3 fixtures verify class-only placements, named-parent construction
  order, per-instance class overrides without shared-symbol contamination, and
  native callback retention/42 roundtrip/cross-runtime rejection.
- Generated class reentrancy, optional sealed-field behavior and collection
  callback argument semantics have concrete remaining failures. Retained
  minimal fixtures/source receipts isolate them; do not mark them solved by a
  source-specific default or broad error suppression.
- Native Scaleform Extensions/TextFieldEx behavior, localization/STBL token
  binding, original UI state/message contracts and 3D-bound controls remain
  release gates. Original AS stub bodies are not working implementations.

The private capture example writes source pins and optional screenshots only
to the caller's explicit diagnostic destination. Keep those assets/evidence
outside Git. Build with debug info and incremental compilation disabled to keep
the private runtime build bounded; cleanup requires the user's current approval
workflow and is not part of this adapter.

## Verified editor actor

`actor::Actor::spawn(gpu, config, initial_snapshot, wake)` owns package reads,
original VM, and GPU overlay on its worker. Keep the production Ruffle core pin
unchanged unless a separately qualified compatibility patch is accepted.
`HostConfig.configure_runtime` is an explicit private qualification seam; normal
production configuration uses `None`.

The app calls `publish(EditorSnapshot)` after an observed native context change.
A snapshot has `revision`, `ready`, optional `ObservedSimInfo`, exact
`service_replies`, and explicitly available `action_services`. Changed contents
require a newer revision. Sim IDs remain decimal strings with all 64 bits; absent
appearance/household fields stay absent. A CASGetSimInfo request for another Sim
is refused. No placeholder household or default pregnancy/species data is added.
Snapshots reject VM identities, nonfinite numeric values, conflicting read/action
names, excessive nesting, and payloads above 64 MiB. They contain no path-count
limit. The app remains responsible for the validity of its observed values.

`try_send(Command::Frame(FrameRequest {size,dpi,input}))` drives the original
movie. `ActorEvent::Frame` returns its transparent texture, source pins, source
status, lifecycle status, and exact latched snapshot revision. A single VM
command sees one immutable snapshot even if the app publishes concurrently.
`WaitingForEditor` acknowledges the frame and lists missing required source
packets; the app must clear its in-flight flag, hide stale interactive source
content, and show that waiting state. This gate also applies after startup when
the native editor becomes unavailable. `Failed` terminates the worker.

`Action(ActionRequest)` retains original request arguments, correlation ID and
snapshot revision. The app rejects stale actions, executes its real operation,
waits for verified recovery acceptance, and publishes a fresh snapshot. Undefined
is returned only as queue admission, never a fabricated mutation result.
`SetSimView` is strictly Boolean `{left,button_down}`; expose it only after the
source heading/motion contract is available. Original `CasNavigationUndo` and
`CasNavigationRedo` require `Null,true`. After accepted native operations, send
`Command::SendMessage` with `CasUndoEnabled` / `CasRedoEnabled` and bare Boolean
payloads. The original Navigation initializes both buttons disabled. A refreshed
observed Sim packet uses original message `CASSetSimInfo`; source listeners
construct their actual SimInfo type. No invented per-request success callback is
required. Other actions remain raw original requests and must be explicitly
advertised by an actual backend implementation.

`required_before_first_frame` is caller-selected and must include any editor
packets needed by the chosen original root. Static replies and editor replies
cannot claim the same service. `static_data::TropesSnapshot` projects original
GetTropesTuning metadata from explicit, ambiguity-checked provider choices and
retains every original byte and parsed unknown field. It is qualified against
the independent decoder and original native marshaller; provider precedence and
active pack eligibility remain caller-owned. The source menu/filter projection
is qualified separately below; unresolved capabilities remain visible.


`menus::CasMenuSnapshot` supplies the three source menu/filter packets from one
explicitly resolved SimData provider. `HostConfig.menu_provider` loads it on the
worker; `EditorSnapshot.menu_context` supplies current observed eligibility facts.
Missing facts remain unavailable. The native packets match both independent
S4TK and original native marshalling/filter instructions in 12 contexts; see
`evidence/static-menu-qualification.json` and the source menu host-data document.
This replaces the previous open menu-data projection gate; provider precedence,
pack capabilities and complete original UI behavior remain separate gates.


`HostConfig.static_images` optionally supplies `Arc<dyn StaticImageResolver>`.
The source Navigator accepts only the qualified GET `img://0x<instance>?rid=<id>`
shape. `rid` is correlation, not an alternate resource identity. Each resolver
call runs on a separate bounded background worker, with shared cancellation;
there is no VM re-entry or default filesystem/network transport. The resolver
must choose one supported full TGI/provider within the caller's explicit scope,
return bounded PNG bytes and actual source pins, and maintain its own bounded
cache. Missing/ambiguous/unsupported resources fail visibly. The adapter checks
PNG extents, pin hashes, cancellation and consistency with the already loaded
source provider index. `RuntimeEvent::ImageResolved` reports the accepted pins
and encoded PNG digest. Accepted image pins join subsequent actor frames for
session persistence. Generated Sim portraits and other URI schemes are not
covered by this static original-resource contract.
