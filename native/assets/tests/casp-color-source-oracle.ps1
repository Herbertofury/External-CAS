# SPDX-License-Identifier: GPL-3.0-or-later
# Compile the original GPL slider classes unchanged in memory; no assets saved.
param([string]$SourcePath,[string]$SourceSha256)
$ErrorActionPreference='Stop'
$sourceBytes=[IO.File]::ReadAllBytes($SourcePath)
$actual=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($sourceBytes)).ToLowerInvariant()
if($actual -ne $SourceSha256){throw 'Pinned CASP source changed'}
$source=[Text.Encoding]::UTF8.GetString($sourceBytes)
$start=$source.IndexOf('        public class OpacitySettings')
$end=$source.IndexOf('        internal byte RemoveKey',$start)
if($start -lt 0 -or $end -le $start){throw 'Original slider classes unavailable'}
$classes=$source.Substring($start,$end-$start)
$wrapper=@'
using System;using System.IO;
namespace OriginalCaspSliderOracle { public class Casp {
CLASSES
public static uint[] Read(byte[] raw) {
using(var reader=new BinaryReader(new MemoryStream(raw))) {
var opacity=new OpacitySettings(reader);var hue=new SliderSettings(reader);var saturation=new SliderSettings(reader);var brightness=new SliderSettings(reader);
float[] values={opacity.Minimum,opacity.Increment,hue.Minimum,hue.Maximum,hue.Increment,saturation.Minimum,saturation.Maximum,saturation.Increment,brightness.Minimum,brightness.Maximum,brightness.Increment};
var bits=new uint[values.Length];for(int i=0;i<bits.Length;i++)bits[i]=BitConverter.ToUInt32(BitConverter.GetBytes(values[i]),0);return bits;
}}}}
'@
Add-Type -TypeDefinition $wrapper.Replace('CLASSES',$classes)
$stream=[Console]::OpenStandardInput();$reader=[IO.BinaryReader]::new($stream);$count=$reader.ReadUInt32()
if($count -gt 65536){throw 'Explicit sample oracle bound'}
$rows=[Collections.Generic.List[object]]::new()
for($i=0;$i -lt $count;$i++){$raw=$reader.ReadBytes(44);if($raw.Length -ne 44){throw 'Truncated sample slider block'};$rows.Add([OriginalCaspSliderOracle.Casp]::Read($raw))}
ConvertTo-Json -InputObject @($rows.ToArray()) -Compress -Depth 5
