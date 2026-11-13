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
`01387E10`. This producer's caller matrix identity and live path selection
remain separate gates; the larger frame path `01388210` also uses its angle
helper and must not be conflated with this output profile.

The explicit Rust math boundary is
`material::cas_lighting::source_cas_light_frame`. It accepts every resolved
profile scalar/enable/color plus exact source matrix rows0 and2. It has no
profile selector, camera-name heuristic or missing-value defaults.

The producer multiplies each row by `[1,0,1,1]`, normalizes XYZ, and multiplies
by2. The source static initializers `0004B800` and `0004C330` establish that
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
radius2, not an elevation angle. This source world-space operation uses Y as
vertical; the Sim GEOM-to-world transform is a separate contract.

Key and rim color vectors are source RGB multiplied by intensity. The source
loader expands RGB to `[R,G,B,R]`, including the fourth component. Disabled
keys have zero direction/color and disabled rims have zero color. The scalar
vectors are independent of those enable flags:

- `CASSpecularTerms = [key0Specular,key1Specular,0,0]`.
- `CASRimFalloff = [rim0Falloff,rim1Falloff,0,0]`.
- `CASAmbientIntensity = [minimum,maximum,0,0]`.

The source light-state block contains directions at byte16 and colors at80.
These match the separate shader parameter bindings; the entire source state
must not be copied blindly into a shader's stage-specific CB layout.

## Original-instruction qualification

The isolated reference executes original static initializers and original
helper/frame instructions. A bounded memset and the downstream light-state
adjustment boundary `01270220` are intercepted; no native API is called.
That boundary may derive additional state rows in some renderer modes; it
does not change the requested direction/color rows in this profile.

All268 helper cases match the compiled Rust output bits. All153 frame cases
match every color/specular/rim/ambient bit; directions differ by at most
`1.1920928955078125e-7` after the original refinement steps. The cases include
all five retained source profiles and controlled orientations, scale, enable
flags and colors. The reference executes117,271 original instructions at381
addresses, with153 bounded memset and153 state-boundary interceptions.

The explicit test uses `EXTERNAL_CAS_LIGHT_FRAME_RECEIPT`. It qualifies the
math, not a camera selector, live chosen frame path, view projection or finished
appearance. [Sanitized evidence](evidence/cas-light-frame-math.json) retains
those limits without original programs or tuning payloads.

## Original SetSimView press, hold and release

Original CASNavigation sends `{left,button_down}`. Native registration maps
the `SetSimView` literal at `01E7F910` to handler `004390E0`. It maps left to
sign+1 and right to-1, and obtains the active CAS hold controller through
virtual method `+0x608`. It does not request pose or animation playback.

Press calls `004815D0`, storing a short-click callback `0043A940`, held callback
`0043A8E0`, the sign and the original delay argument300. Release `00481640`
invokes the short-click callback only if the held path has not begun; otherwise
it clears both callbacks. Timing-unit and update-cadence qualification remain
explicit until the underlying timer contract is joined.

The concrete CAS vtable installed by constructor `00443A60` is `01E849C0`:

- `+0x288` selects `00457770`, which applies an incremental Y-axis quaternion
  in the actor transform, using source degree/radian and trig operations.
- `+0x290` selects `00455AD0`, which finds the next/previous of six source
  `Client_CASTuning.PresetRotation.ViewAngles`. It compares within0.01 degrees,
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
The tuning owner is adding typed preset fields and their actual SimData partner.
Exact Sim/form identity, source context selection and draft rotation retention
remain responsibilities of the Rust-owned editor controller.

## Camera and anchor work still open

Actual CameraTarget/FOV/Distance/OrbitalRotation/Pitch values and world origin
are source-pinned, but their existence alone does not establish matrix identity,
FOV axis, handedness, model anchor, projection near/far or transition rules.
The next boundary is their native consumer and actor world transform. The light
API deliberately retains raw source row identities instead of labeling an
arbitrary host matrix as the original camera basis.
