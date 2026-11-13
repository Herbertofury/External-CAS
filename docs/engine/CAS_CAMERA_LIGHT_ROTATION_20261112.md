# Source CAS camera, light-frame and Sim-view contracts

This continues the [engine map](ENGINE_CAS_IMPLEMENTATION_MAP.md), qualified
against client SHA256
`95e40b75f76711c66b9f36a974ec04d43304a6f591d409d3d4019818f366237b`.
Original code and local tuning payloads remain private. No running game,
input device, game profile or save was contacted or modified.

## Resolved light inputs to frame constants

The selected installed CAS light profiles and SimData defaults are already
source-pinned by `native/room::load_cas_tuning`. Native SimData loader
`01386F60` fills the profile records consumed by the two-key/two-rim producer
`01387E10`. Leaf dispatcher `01387DF0` chooses this producer when its first
context's byte `+0x2EC` is nonzero; otherwise it tail-dispatches to the larger
frame path `01388210`. The dispatcher is the function pointer at
`02057E30+0x18`, whose primary table is installed by constructor `01386170`.
Its secondary interface at object `+0x70` uses table `02057E70`; setter
`01386E60` writes the source enable byte and profile index at object `+0x2EC`
and `+0x2E8`. These table boundaries are established by constructor writes,
rather than treating neighboring tables as one contiguous interface.

The actual initial CAS path `0044F3C0` creates this state through component
helper `009BC7E0`, obtains its secondary interface with `009BC7A0`, and enables
it with the profile returned by `019F5E60(0,SimDescriptor)`. For species 1 that
selector returns index 0. Loader `01386F60` reads the source name array at
`0280E2D0`; index 0 is the literal `Default`, under the `caslighting` namespace
and `Client_CASLightingTuning` schema. The source data-section decoding is kept
private and hash-pinned. This establishes the initial Human profile without
choosing a profile from a name heuristic. Later dynamic profile changes and the
Update caller matrix identity remain separate gates. The larger frame path also
uses its angle helper and must not be conflated with this output profile.

The explicit Rust math boundary is
`material::cas_lighting::source_cas_light_frame`. It accepts every resolved
profile scalar/enable/color plus exact source matrix rows 0 and 2. It has no
profile selector, camera-name heuristic or missing-value defaults.

The producer multiplies each row by `[1,0,1,1]`, normalizes XYZ, and multiplies
by 2. The source static initializers `0004B800` and `0004C330` establish that
projection vector and the independent XYZ dot-product mask. Their globals are
zero-initialized virtual data, so raw file-offset reads cannot supply their
runtime values. Normalization uses the original RSQRT seed followed by two
source refinement steps.

For each key, original helper `01386B10` forms:

```text
candidate = ( [0,height-LightBaseHeight,0,0]
              + row0ProjectedNormalizedTimesTwo * sin(angle) )
            + row2ProjectedNormalizedTimesTwo * cos(angle)
LightDirection = normalizeXYZ(candidate)
```

Angles are degrees, converted with exact float32 constant `3C8EFA35`. The
source sine/cosine use their original range reduction and polynomial, rather
than a replacement libm operation. Height is a vertical offset with horizontal
radius 2, not an elevation angle. This source world-space operation uses Y as
vertical; the Sim GEOM-to-world transform is a separate contract.

Key and rim color vectors are source RGB multiplied by intensity. The source
loader expands RGB to `[R,G,B,R]`, including the fourth component. Disabled
keys have zero direction/color and disabled rims have zero color. The scalar
vectors are independent of those enable flags:

- `CASSpecularTerms = [key0Specular,key1Specular,0,0]`.
- `CASRimFalloff = [rim0Falloff,rim1Falloff,0,0]`.
- `CASAmbientIntensity = [minimum,maximum,0,0]`.

The source light-state block contains directions at byte 16 and colors at 80.
These match the separate shader parameter bindings; the entire source state
must not be copied blindly into a shader's stage-specific CB layout.

## Original-instruction qualification

The isolated reference executes original static initializers and original
helper/frame instructions. A bounded memset and the downstream light-state
adjustment boundary `01270220` are intercepted; no native API is called.
That boundary may derive additional state rows in some renderer modes; it
does not change the requested direction/color rows in this profile.

All 268 helper cases match the compiled Rust output bits. All 153 frame cases
match every color/specular/rim/ambient bit; directions differ by at most
`1.1920928955078125e-7` after the original refinement steps. The cases include
all five retained source profiles and controlled orientations, scale, enable
flags and colors. The reference executes 117,271 original instructions at 381
addresses, with 153 bounded memset and 153 state-boundary interceptions.

The explicit test uses `EXTERNAL_CAS_LIGHT_FRAME_RECEIPT`. It qualifies the
math, not a camera selector, live chosen frame path, view projection or finished
appearance. [Sanitized evidence](evidence/cas-light-frame-math.json) retains
those limits without original programs or tuning payloads.

## Original SetSimView press, hold and release

Original CASNavigation sends `{left,button_down}`. Native registration maps
the `SetSimView` literal at `01E7F910` to handler `004390E0`. It maps left to
sign +1 and right to -1, and obtains the active CAS hold controller through
virtual method `+0x608`. It does not request pose or animation playback.

Press calls `004815D0`, storing a short-click callback `0043A940`, held callback
`0043A8E0`, the sign and the original delay of 300 milliseconds. Release `00481640`
invokes the short-click callback only if the held path has not begun; otherwise
it clears both callbacks. The hold transition requires the source clock to be
strictly later than the deadline; equality does not begin holding. Constructor
`00481500` selects timer unit 4, setter `013D0FF0` maps that unit to the
coefficient initialized as `1000/counterFrequency` by `013D0B60`, and
`013D0F20` converts the limit back to counter ticks. The independent public
[EAStdC Stopwatch declaration](https://github.com/electronicarts/EAStdC/blob/master/include/EAStdC/EAStopwatch.h)
and [implementation](https://github.com/electronicarts/EAStdC/blob/master/source/EAStopwatch.cpp)
agree with this unit and conversion contract. That agreement does not establish
the installed library's exact upstream revision. Update cadence remains open.

The concrete CAS vtable installed by constructor `00443A60` is `01E849C0`:

- `+0x288` selects `00457770`, which applies an incremental Y-axis quaternion
  in the actor transform, using source degree/radian and trig operations.
- `+0x290` selects `00455AD0`, which finds the next/previous of six source
  `Client_CASTuning.PresetRotation.ViewAngles`. It compares within 0.01 degrees,
  handles the circular final interval, and preserves the current and target
  quaternion as separate interpolation state.
- `+0x608` selects `0044C700`, which returns the hold controller.

Short-click interpolation progress-rate is
`LerpSpeedMult*180/(shortestAngleDifference+float32EPSILON)`; `00489E40`
updates that interpolation later. A fixed invented step would discard the
configured six-view behavior.

Held ticker `00481690` adds source `RotateContinuousEaseIn` to its accumulated
speed, caps with `RotateContinuousSpeed`, multiplies by the selected sign, then
calls `0043A8E0`. That callback multiplies by exact float32 `0.15` and invokes
the incremental method. It receives no delta-time argument; an assumed
degrees-per-second conversion would introduce an unqualified update cadence.
Keyboard rotation has separate `RotateKeySpeed` and `RotateKeySpeedFast` fields.

These values come from `Client_CASTuning`, not `Client_CASCameraTuning`.
`room::preset_rotations` retains the typed preset fields, source pin and actual
SimData partner pin. `scene::cas_view` accepts only explicitly resolved values.
It emits preset targets or per-source-tick degree increments without choosing an
actor, update cadence or world basis. It preserves a pending press on invalid
input and exposes an explicit editor cancellation boundary.

Independent original-instruction qualification covers 2,964 preset-loop cases
and 64 controller sessions containing 731 events. Every preset index, emitted
degree bit pattern and accumulated-speed bit pattern matches the Rust API.
The reference executes 411,705 original instructions at 214 addresses, with
bounded clock/callback/context/tuning interceptions and no native I/O.
[Sanitized rotation evidence](evidence/cas-view-source-math.json) records the
interception boundaries and the unresolved actor/cadence gates.

Exact Sim/form identity, source context selection and draft rotation retention
remain responsibilities of the Rust-owned editor controller.

## Explicit source camera profile to frame

Native constructor `00232280` owns the camera tuning loader `00233D90` and
installs camera-controller table `01E3FF58`. Its update dispatch `00235670`
eventually calls `00235370` with resolved profile state. This is the native
camera consumer, rather than a convention inferred from resource field names.

For pitch `p` and orbital rotation `o`, source spherical helper `00230890`
produces `[cos(p)*sin(o),sin(p),-cos(p)*cos(o),cos(p)*sin(o)]`. All angles use the
source degree conversion and original cosine polynomial. Camera eye is target
plus distance times that vector. The original target's fourth lane repeats X;
the final camera world transform writes eye W as 1.

Static initializer `0000C9A0` establishes world-up `[0,1,0,0]`. Helpers
`01209FD0` and `0120A020` construct backward from eye minus target, right from
up cross backward, and up from backward cross right, using the native RSQRT
profile. Near-zero cross products use the source fixed fallback axes. The world
matrix stores rows right/up/backward/eye. This is a right-handed camera, with
visible geometry at negative camera Z.

The configured FOV is a horizontal angle at the original 4:3 reference aspect,
not a vertical angle. `01209730` computes `2*tan(FOV/2)`, and `00235370`
multiplies by `cameraAspect*0.75`. Setter `0120A630` stores that horizontal span
and divides by aspect for the vertical span. `01442120` supplies the original
rational tangent profile; the Rust port retains its constants and operation
order. Feeding the tuning angle directly into a vertical-FOV constructor gives
different framing.

`scene::cas_camera::source_cas_camera_frame` accepts an explicit source profile,
aspect and near/far values. It emits source world rows, a semantic rigid-inverse
view, perspective projection and view-projection. The unshifted perspective
profile is ported from `01208EF0`; it maps near to depth 0 and far to depth 1.
It does not infer clip defaults, render overrides, anchor, target transitions,
active region or the light producer's input matrix identity.

Across 129 controlled original-instruction cases, eye positions, all FOV-span
bits and every projection element match exactly. World-basis maximum absolute
difference is `1.1920928955078125e-7`. The Rust rigid inverse differs from the
source general inverse by at most `0.00006103515625` for these world positions;
the original general-inverse operation order is not yet ported. Three focused
camera tests pass, including geometric depth/framing checks and the private
original-instruction receipt. [Sanitized camera evidence](evidence/cas-camera-source-math.json)
records that exact scope.

The retained room object's position is an actual WorldData `OMGS/OMGR/OBJ /POS `
field. It has not been proven to be the Sim anchor or a native world origin.
Using it as a coordinate origin is an explicit host convention. The actor
placement writer, clipping-policy caller, viewport adjustments and light-basis
join remain open. The light API deliberately retains raw source row identities
instead of labeling an arbitrary host matrix as the original camera basis.
