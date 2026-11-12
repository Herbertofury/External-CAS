param([Parameter(Mandatory=$true)][string]$SimRipperSrc,[Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference='Stop'
$pins=@{'CASModifierTuning.cs'='527a12b8982bd96c757fa78a42ec8c6615e9fd29a69ef9496bd5e3d26ef0f021';'FNV.cs'='a180b100f1f5aeb7d203525e6b5beb3a7bf5d6df4350ba6e1f93abe103cd75ff';'Enums.cs'='fa48d0ec2a7418ee30d8167682ca212d879494d94f8e9a906701fa7ffb0b006b'}
foreach($name in $pins.Keys){if((Get-FileHash -LiteralPath (Join-Path $SimRipperSrc $name) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $pins[$name]){throw "Source pin mismatch: $name"}}
$source=Get-Content -LiteralPath (Join-Path $SimRipperSrc 'CASModifierTuning.cs') -Raw
function Extract-Method([string]$signature){$start=$source.IndexOf($signature,[StringComparison]::Ordinal);if($start -lt 0){throw 'Method absent'};$at=$source.IndexOf('{',$start);$depth=1;$end=$at+1;while($depth -gt 0 -and $end -lt $source.Length){if($source[$end] -eq '{'){$depth++}elseif($source[$end] -eq '}'){$depth--};$end++};if($depth -ne 0){throw 'Method extent'};return $source.Substring($start,$end-$start)}
$prefix='using System;using System.IO;using System.Linq;using System.Collections.Generic;using System.Xml;using System.Globalization;namespace TS4SimRipper {public class TuningHarness {'
$suffix='public object Parse(byte[] xml){using var s=new MemoryStream(xml);using var r=new XmlTextReader(s);r.DtdProcessing=DtdProcessing.Prohibit;r.XmlResolver=null;while(r.Read()){if(r.NodeType==XmlNodeType.Element&&r.GetAttribute("c")=="Client_CASModifierTuning"){var t=ParseTuning(r);return new {species=(uint)t.species,occult=(uint)t.occult,age=(uint)t.age,gender=(uint)t.gender,names=t.names.Count,scales=t.weights.ToDictionary(x=>x.Key.ToString("X16"),x=>BitConverter.SingleToInt32Bits(x.Value).ToString("X8"))};}}throw new Exception("class absent");}}}'
$content=$prefix+"`n"+(Extract-Method 'private Tuning ParseTuning(')+"`n"+(Extract-Method 'internal class Tuning')+"`n"+$suffix
$directory=Split-Path -Parent $Output
if(-not(Test-Path -LiteralPath $directory)){New-Item -ItemType Directory -Path $directory | Out-Null}
if(Test-Path -LiteralPath $Output){throw 'Output already exists'}
[IO.File]::WriteAllText([IO.Path]::GetFullPath($Output),$content)
