param([Parameter(Mandatory=$true)][string]$ReferenceSrc,[Parameter(Mandatory=$true)][string]$OutputFile)
$ErrorActionPreference='Stop'
$pins=@{'GEOM.cs'='9e332175017405284ca0744025ae8cc3269f7e6ccd9caa28306711e451d99693';'Enums.cs'='fa48d0ec2a7418ee30d8167682ca212d879494d94f8e9a906701fa7ffb0b006b'}
foreach($relative in $pins.Keys){if((Get-FileHash -LiteralPath (Join-Path $ReferenceSrc $relative) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pins[$relative]){throw "Pinned GPL source changed: $relative"}}
function Get-BracedDeclaration([string]$Text,[string]$Declaration){
    $start=$Text.IndexOf($Declaration,[StringComparison]::Ordinal)
    if($start -lt 0){throw "Missing declaration $Declaration"}
    $open=$Text.IndexOf('{',$start);$depth=0
    for($at=$open;$at -lt $Text.Length;$at++){
        if($Text[$at] -eq '{'){$depth++}elseif($Text[$at] -eq '}'){$depth--;if($depth -eq 0){return $Text.Substring($start,$at-$start+1)}}
    }
    throw "Unclosed declaration $Declaration"
}
$geom=Get-Content -LiteralPath (Join-Path $ReferenceSrc 'GEOM.cs') -Raw
$enums=Get-Content -LiteralPath (Join-Path $ReferenceSrc 'Enums.cs') -Raw
$source="// Generated only for independent local validation. GPL-3.0-or-later, C. Marinetti, TS4 SimRipper.`nusing System;using System.IO;using System.Text;using System.Collections.Generic;`nnamespace PinnedReference { public class GEOM {`n"+(Get-BracedDeclaration $geom 'public class MTNF')+"`n}`n"+(Get-BracedDeclaration $enums 'public enum ShaderParam : uint')+"`n}`n"
$file=[IO.File]::Open($OutputFile,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write)
try{$bytes=[Text.Encoding]::UTF8.GetBytes($source);$file.Write($bytes,0,$bytes.Length);$file.Flush($true)}finally{$file.Dispose()}
