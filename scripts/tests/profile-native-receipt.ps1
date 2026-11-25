# Generated no-op child; no Sims install or personal files are used.
$ErrorActionPreference = 'Stop'
$profile = Join-Path $PSScriptRoot '../profile-native.ps1'
$root = Join-Path ([IO.Path]::GetTempPath()) ('eca-profile-fixture-' + [guid]::NewGuid())
[IO.Directory]::CreateDirectory($root) | Out-Null
try {
    $source = Join-Path $root 'child.cs'
    $child = Join-Path $root 'child.exe'
    @'
using System;
using System.Threading;
class Fixture {
    static int Main(string[] args) {
        Console.WriteLine("synthetic-out");
        Console.Error.WriteLine("synthetic-err");
        if (Array.IndexOf(args, "timeout-fixture") >= 0) Thread.Sleep(30000);
        return Array.IndexOf(args, "exit-fixture") >= 0 ? 17 : 0;
    }
}
'@ | Set-Content -LiteralPath $source
    $compiler = Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
    & $compiler /nologo "/out:$child" $source
    if ($LASTEXITCODE -ne 0) { throw 'Could not compile the disposable profiling fixture.' }
    foreach ($query in @('success-fixture','exit-fixture','timeout-fixture')) {
        $out = Join-Path $root $query
        $caught = ''
        try {
            & $profile -Binary $child -AssetRoots @($root) -OutputDirectory $out -Query $query -TimeoutSeconds 2 | Out-Null
        } catch {
            $caught = $_.Exception.Message
        }
        if ($query -eq 'success-fixture' -and $caught) { throw $caught }
        if ($query -eq 'exit-fixture' -and $caught -notmatch 'exited with 17') { throw "Wrong nonzero-exit outcome: $caught" }
        if ($query -eq 'timeout-fixture' -and $caught -notmatch 'exceeded 2 seconds') { throw "Wrong timeout outcome: $caught" }
        $run = Get-Content -LiteralPath (Join-Path $out 'run.json') -Raw | ConvertFrom-Json
        $samplesText = Get-Content -LiteralPath (Join-Path $out 'memory-samples.json') -Raw
        if ($samplesText -notmatch '^\s*\[') { throw 'Memory samples are not stored as an array.' }
        $sampleArray = ConvertFrom-Json -InputObject $samplesText -NoEnumerate
        if ($sampleArray.Count -ne $run.samples) { throw 'Sample count differs from the retained array.' }
        if ($run.timedOut -ne ($query -eq 'timeout-fixture')) { throw 'Incorrect timedOut flag.' }
        if ($query -eq 'timeout-fixture' -and $run.samples -lt 1) { throw 'Timeout lost its process samples.' }
        if ($run.query -ne $query -or !$run.privateEvidence -or $run.timeoutSeconds -ne 2) { throw 'Missing run metadata.' }
        if ((Get-Content -LiteralPath (Join-Path $out 'stdout.log') -Raw) -notmatch 'synthetic-out') { throw 'Lost stdout.' }
        if ((Get-Content -LiteralPath (Join-Path $out 'stderr.log') -Raw) -notmatch 'synthetic-err') { throw 'Lost stderr.' }
        if ($run.binarySha256 -ne (Get-FileHash -LiteralPath $child -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Binary identity mismatch.' }
    }
    Write-Host 'Profiler retained exact stdout, stderr, memory samples and run metadata on success, nonzero exit and timeout.'
} finally {
    Remove-Item -LiteralPath $root -Recurse -Force
}
