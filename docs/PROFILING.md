# Native application measurements

`scripts/profile-native.ps1` launches the actual native Windows application
hidden, with explicit authorized source roots. It collects startup/discovery,
search, thumbnail work, actual UI/render-submission CPU costs, app-frame
intervals and sampled process working/private memory. Optional controlled
scrolling traverses the real virtualized catalog and cancels obsolete thumbnail
requests. It does not drive or contact the game.

Build release mode first, then choose a **new private** output directory:

```powershell
pwsh -File scripts/build-native.ps1 -Configuration Release
pwsh -File scripts/profile-native.ps1 -AssetRoots '<installed CC directory>' -OutputDirectory '.local/profile-run-01' -Query 'hair' -Frames 180 -Columns 3 -Scroll
```

The script uses a below-normal-priority child and records the exact binary
SHA-256. It never replaces existing evidence. The app’s explicit `--capture` output
also refuses existing filenames. It stages the encoded image in the chosen
folder and reports `capture.status` as saved, not_saved, or not_requested.
Runtime captures and source paths
are private and must stay excluded from Git. A diagnostic timeout terminates
only its own child, retaining any partial evidence.

Report package/CASP counts, cache hits/misses, unreadable/unsupported sources,
query, columns, actual hardware, other active workloads and cache state. Repeat
only to distinguish cold metadata from warm metadata or investigate a change.
OS caches are not flushed and the shared machine is not a clean environment.

`ui_and_render_submission_cpu_ms` measures native UI plus wgpu submission work;
`app_frame_interval_ms` includes scheduling/presentation. Neither is a GPU
timestamp. Query timing ends at UI submission or visible-thumbnail completion,
not measured monitor presentation. Startup and total diagnostic wall time are
separate because controlled frames occur after initial content readiness.

Component throughput and a tiny populated fixture do not establish large-library
performance. No in-game baseline or improvement ratio is implied. Clean-machine,
long-session, multiple-real-Sim, GPU/frame-pacing and interactive input-to-visible
qualification remain acceptance gates.

Superseded wardrobe searches also stop in the design/swatch grouping stage,
without publishing partial groups. The focused grouping fixtures retain every
row and exact first-seen design/stable swatch order for live searches. In the
4,096-row cancelled fixture, 256 group assignments were visited and no sort
started, instead of processing the entire stale result. This is a work-count
check, not an interactive timing claim; a single already-running stable sort
can still finish before its result is discarded.
