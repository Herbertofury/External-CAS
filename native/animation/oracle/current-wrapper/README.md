# Optional current CLIP validation oracle

This development-only C# harness invokes a separately supplied, unchanged
`TS4AnimationBrowser.Core` 1.9.1 reader. No source or built reader is vendored.
No license grant was found in the inspected source tree, so this harness and
its outputs must not cause that reader to enter a product distribution.

Run `verify.ps1 -PrimaryReaderSrc PATH` first. Its SHA256 pins cover
`ViewerV2/StudioClipDecoderV2.cs`, `Animation/ClipAnimation.cs` and
`Animation/ClipChannelType.cs`. Supply that same root as the MSBuild property
`PrimaryReaderSrc`; keep `BaseIntermediateOutputPath` and the output directory
under ignored local build storage. The harness reads one authorized CLIP from
stdin and emits decoded private metadata and skeletal frame bits to stdout.
Never publish asset-bearing output or include it in a release.

The character example `qualify_cas_stand_idle` optionally consumes the harness
through `CAS_CURRENT_CLIP_ORACLE`. It compares wrapper fields exactly and every
skeletal value within a declared float tolerance, preserving any bit differences
in the private receipt. Runtime Rust does not invoke this executable.
