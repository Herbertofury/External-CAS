param([Parameter(Mandatory=$true)][string]$ToneSource,[Parameter(Mandatory=$true)][string]$InputFile)
$ErrorActionPreference='Stop'
if ((Get-FileHash -LiteralPath $ToneSource -Algorithm SHA256).Hash -ne '3ABCE3053D23A246C5B11F1FC8EC1337720028C5F6AF50DA41BF5ECAB42D70D0') { throw 'Original TONE source pin changed' }
# Minimal type adapters let the unmodified original TONE reader compile.
$types=@'
namespace TS4SimRipper {
 [System.Flags] public enum AgeGender:uint { None=0 }
 public class CASP { public class PartTag {
  public ushort FlagCategory; public uint FlagValue;
  public PartTag(ushort c,uint v){FlagCategory=c;FlagValue=v;}
  public PartTag(System.IO.BinaryReader r,int width){FlagCategory=r.ReadUInt16();FlagValue=width==4?r.ReadUInt32():r.ReadUInt16();}
  public void Write(System.IO.BinaryWriter w,int width){w.Write(FlagCategory);if(width==4)w.Write(FlagValue);else w.Write((ushort)FlagValue);}
 } }
}
'@
Add-Type -TypeDefinition ('using System; using System.Collections.Generic; using System.Linq; using System.Text; using System.Drawing; using System.IO;' + [Environment]::NewLine + $types + [Environment]::NewLine + (Get-Content -LiteralPath $ToneSource -Raw).Replace('using System;','').Replace('using System.Collections.Generic;','').Replace('using System.Linq;','').Replace('using System.Text;','').Replace('using System.Drawing;','').Replace('using System.IO;','')) -ErrorAction Stop
$reader=[System.IO.BinaryReader]::new([System.IO.MemoryStream]::new([System.IO.File]::ReadAllBytes($InputFile)))
try {
 $tone=[TS4SimRipper.TONE]::new($reader)
 [ordered]@{version=$tone.Version;texture=('{0:X16}' -f $tone.GetSkinSetTextureInstance(0));overlay=('{0:X16}' -f $tone.GetSkinSetOverlayInstance(0));hue=$tone.Hue;saturation=$tone.Saturation;opacity=$tone.Opacity;overlayCount=$tone.NumberOverlays;makeupOpacity=$tone.SkinSets[0].MakeupOpacity} | ConvertTo-Json -Compress
} finally {$reader.Dispose()}
