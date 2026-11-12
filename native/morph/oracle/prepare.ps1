param([Parameter(Mandatory=$true)][string]$SimRipperSrc,[Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference='Stop'
$pins=@{'GEOM.cs'='9e332175017405284ca0744025ae8cc3269f7e6ccd9caa28306711e451d99693';'PreviewControl.cs'='a8b8044ad5d1f768f57019e7073f5a09e479683fc06d7ad91c2b313d14f9ca3d';'RIG.cs'='3d9ea554c5428c28c1eecba95f608ec0dcb17ed3379c9d3f6a53998d1cc7e6fd';'BOND.cs'='d86adf8236dffd5447eb1bd58d4ce48469cb5914ea3c0b64c3b49e1937c84640';'Geometry.cs'='231b6a332f5929eae91e0345392a88d0ca76dca74ddcca87d15c672beeb7c920';'TGI.cs'='56c8992118e229701ca6151d3c3f15b19b2940a72d394480a7112987287f243d';'Enums.cs'='fa48d0ec2a7418ee30d8167682ca212d879494d94f8e9a906701fa7ffb0b006b'}
foreach($name in $pins.Keys){if((Get-FileHash -LiteralPath (Join-Path $SimRipperSrc $name) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pins[$name]){throw "Source pin mismatch: $name"}}
function Extract-Method([string]$text,[string]$signature){$start=$text.IndexOf($signature,[StringComparison]::Ordinal);if($start -lt 0){throw 'Method absent'};$at=$text.IndexOf('{',$start);$depth=1;$end=$at+1;while($depth -gt 0 -and $end -lt $text.Length){if($text[$end] -eq '{'){$depth++}elseif($text[$end] -eq '}'){$depth--};$end++};if($depth -ne 0){throw 'Method extent'};return $text.Substring($start,$end-$start)}
$geom=Get-Content -LiteralPath (Join-Path $SimRipperSrc 'GEOM.cs') -Raw
$preview=Get-Content -LiteralPath (Join-Path $SimRipperSrc 'PreviewControl.cs') -Raw
$methods=@('public float GetTotalBoneWeight(int vertexIndex, List<uint> boneHashes)','internal void SetupDeltas()','internal void UpdatePositions()','internal void BoneMorpher(RIG.Bone bone, float weight, Vector3 offset, Vector3 scale, Quaternion rotation)') | ForEach-Object {Extract-Method $geom $_}
$load=Extract-Method $preview 'public static void LoadBONDMorph(FileStream log, GEOM[] baseMeshes, BOND boneDelta, RIG rig, bool updateRig)'
$content="// Generated unmodified source blocks after whole-file SHA256 verification.`nusing System;using System.IO;using System.Collections.Generic;using System.Windows.Forms;namespace TS4SimRipper { public partial class GEOM {`n"+($methods -join "`n")+"`n} public class PreviewControl {static void LogMe(FileStream f,string s){} `n"+$load+"`n}}"
$directory=Split-Path -Parent $Output
if(-not(Test-Path -LiteralPath $directory)){New-Item -ItemType Directory -Path $directory | Out-Null}
if(Test-Path -LiteralPath $Output){throw 'Output already exists'}
[IO.File]::WriteAllText([IO.Path]::GetFullPath($Output),$content)
