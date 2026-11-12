param(
    [ValidateSet('Debug', 'Release')][string]$Configuration = 'Debug',
    [int]$Jobs = 2
)
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location -LiteralPath $workspace
try {
    $arguments = @('build', '-p', 'external-cas', '--locked', '--jobs', "$Jobs")
    $profileDirectory = 'debug'
    if ($Configuration -eq 'Release') {
        $arguments += '--release'
        $profileDirectory = 'release'
    }
    & cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Native build failed.' }
    $binary = Join-Path $workspace "target\$profileDirectory\external-cas.exe"
    $result = [ordered]@{
        application = 'External CAS'
        configuration = $Configuration
        binarySha256 = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
        lockSha256 = (Get-FileHash -LiteralPath (Join-Path $workspace 'Cargo.lock') -Algorithm SHA256).Hash.ToLowerInvariant()
        rustc = (& rustc --version)
        cargo = (& cargo --version)
        commit = (& git rev-parse HEAD)
        dirty = [bool](& git status --porcelain)
        releaseAccepted = $false
    }
    $result | ConvertTo-Json
} finally {
    Pop-Location
}
