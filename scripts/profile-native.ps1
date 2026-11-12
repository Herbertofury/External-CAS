param(
    [Parameter(Mandatory)][string[]]$AssetRoots,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Query = '',
    [int]$Frames = 180,
    [ValidateRange(1,4)][int]$Columns = 3,
    [switch]$Scroll,
    [string]$UiAssets,
    [string]$Binary,
    [int]$TimeoutSeconds = 1800
)
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
if (!$Binary) { $Binary = Join-Path $workspace 'target\release\external-cas.exe' }
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$outputPath = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $outputPath) { throw 'Choose a new output directory; existing evidence is never replaced.' }
[IO.Directory]::CreateDirectory($outputPath) | Out-Null
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = $Binary
$start.WorkingDirectory = $workspace
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$arguments = @('--hidden','--exit-on-ready','--report',(Join-Path $outputPath 'app.json'),'--capture',(Join-Path $outputPath 'app.png'),'--profile-frames',"$Frames",'--catalog-columns',"$Columns",'--query',$Query)
foreach ($root in $AssetRoots) { $arguments += @('--assets',(Resolve-Path -LiteralPath $root).Path) }
if ($Scroll) { $arguments += '--profile-scroll' }
if ($UiAssets) { $arguments += @('--ui-assets',(Resolve-Path -LiteralPath $UiAssets).Path) }
foreach ($argument in $arguments) { $start.ArgumentList.Add($argument) }
$process = [Diagnostics.Process]::Start($start)
$stdout = $process.StandardOutput.ReadToEndAsync()
$stderr = $process.StandardError.ReadToEndAsync()
try { $process.PriorityClass = [Diagnostics.ProcessPriorityClass]::BelowNormal } catch { Write-Warning $_ }
$watch = [Diagnostics.Stopwatch]::StartNew()
$samples = [Collections.Generic.List[object]]::new()
while (!$process.WaitForExit(100)) {
    $process.Refresh()
    $samples.Add([pscustomobject][ordered]@{ elapsedMs=$watch.Elapsed.TotalMilliseconds; workingSetBytes=$process.WorkingSet64; privateBytes=$process.PrivateMemorySize64; cpuSeconds=$process.TotalProcessorTime.TotalSeconds })
    if ($watch.Elapsed.TotalSeconds -gt $TimeoutSeconds) {
        # Only this diagnostic child is terminated. Inputs are read-only.
        $process.Kill($true)
        throw "Diagnostic child exceeded $TimeoutSeconds seconds. Partial evidence remains at $outputPath"
    }
}
$process.WaitForExit()
$stdout.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $outputPath 'stdout.log')
$stderr.GetAwaiter().GetResult() | Set-Content -LiteralPath (Join-Path $outputPath 'stderr.log')
$samples | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $outputPath 'memory-samples.json')
$result = [ordered]@{
    privateEvidence=$true; exitCode=$process.ExitCode; wallMs=$watch.Elapsed.TotalMilliseconds
    binarySha256=(Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash.ToLowerInvariant()
    peakObservedWorkingSetBytes=($samples | Measure-Object -Property workingSetBytes -Maximum).Maximum
    peakObservedPrivateBytes=($samples | Measure-Object -Property privateBytes -Maximum).Maximum
    samples=$samples.Count; query=$Query; controlledScroll=$Scroll.IsPresent; framesRequested=$Frames
    cacheState='Existing local metadata cache; inspect app cache hit/miss counts. OS cache is not flushed.'
    limitations='CPU UI/submission and sampled process memory; no GPU timestamps, clean machine, game baseline or speedup ratio. Captures contain private installed content.'
}
$result | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $outputPath 'run.json')
$result | ConvertTo-Json -Depth 4
if ($process.ExitCode -ne 0) { throw "Native app exited with $($process.ExitCode). Inspect retained logs." }
