param([Parameter(Mandatory=$true)][string]$SimRipperSrc)
$ErrorActionPreference='Stop'
$protoRoot=Join-Path $SimRipperSrc 'Proto'
[string[]]$names=Get-ChildItem -LiteralPath $protoRoot -File -Filter '*.cs' | ForEach-Object Name
[Array]::Sort($names,[StringComparer]::Ordinal)
$manifest=''
foreach($name in $names){$hash=(Get-FileHash -LiteralPath (Join-Path $protoRoot $name) -Algorithm SHA256).Hash.ToLowerInvariant();$manifest+=$name+[char]0+$hash+"`n"}
$digest=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($manifest))).ToLowerInvariant()
if($names.Length -ne 55 -or $digest -ne '089452376ca4b45780b306f317a1b2c669873571f3383019b0bb958bc461f6fe'){throw 'Authoritative generated schema manifest changed'}
