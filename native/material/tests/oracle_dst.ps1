# SPDX-License-Identifier: GPL-3.0-or-later
# Compile the ORIGINAL pinned s4pi Unshuffle method in a minimal header harness.
# Private source/converted assets stay in the caller's explicit ignored testroot.
param([Parameter(Mandatory)][string]$Source, [Parameter(Mandatory)][string]$Manifest)
$original = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $Source))
$start = $original.IndexOf('        private Stream Unshuffle(DDSHeader header, Stream s)')
$end = $original.IndexOf('        public int Width', $start)
if ($start -lt 0 -or $end -lt $start) { throw 'Original Unshuffle method not found' }
$method = $original.Substring($start, $end - $start)
$prefix = @'
using System;
using System.IO;
public enum FourCC:uint {DST1=0x31545344,DST3=0x33545344,DST5=0x35545344,DXT1=0x31545844,DXT3=0x33545844,DXT5=0x35545844}
public class Format {public FourCC Fourcc;}
public class DDSHeader {
 public Format pixelFormat = new Format(); private byte[] original;
 public DDSHeader(byte[] b){original=b;pixelFormat.Fourcc=(FourCC)BitConverter.ToUInt32(b,84);}
 public void UnParse(Stream stream){byte[] b=(byte[])original.Clone();Buffer.BlockCopy(BitConverter.GetBytes((uint)pixelFormat.Fourcc),0,b,84,4);stream.Write(b,4,124);}
}
public class OriginalS4piUnshuffle {
 public byte[] Convert(byte[] bytes){return ((MemoryStream)Unshuffle(new DDSHeader(bytes),new MemoryStream(bytes))).ToArray();}
'@
Add-Type -TypeDefinition ($prefix + "`n" + $method + "`n}")
$harness = [OriginalS4piUnshuffle]::new()
$manifestPath = (Resolve-Path -LiteralPath $Manifest).Path
$document = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
foreach ($sample in $document.samples) {
    $out = $sample.local_input + '.oracle.dds'
    [IO.File]::WriteAllBytes($out, $harness.Convert([IO.File]::ReadAllBytes($sample.local_input)))
    $sample | Add-Member -NotePropertyName oracle_dds -NotePropertyValue $out -Force
    $sample | Add-Member -NotePropertyName oracle_dds_sha256 -NotePropertyValue (Get-FileHash -Algorithm SHA256 -LiteralPath $out).Hash.ToLowerInvariant() -Force
}
$document | Add-Member -NotePropertyName oracle_source_sha256 -NotePropertyValue (Get-FileHash -Algorithm SHA256 -LiteralPath $Source).Hash.ToLowerInvariant() -Force
$document | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $manifestPath -Encoding utf8
