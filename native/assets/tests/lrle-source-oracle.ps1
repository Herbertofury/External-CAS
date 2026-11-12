# SPDX-License-Identifier: GPL-3.0-or-later
# Read-only, in-memory hash comparison using the original SimRipper decoder.
param([Parameter(Mandatory=$true)][string]$SourcePath,[Parameter(Mandatory=$true)][string]$SourceSha256)
$ErrorActionPreference='Stop'
$sourceBytes=[IO.File]::ReadAllBytes($SourcePath)
$actual=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($sourceBytes)).ToLowerInvariant()
if($actual -cne $SourceSha256){throw 'reference source pin mismatch'}
$source=[Text.Encoding]::UTF8.GetString($sourceBytes).Replace('using System.Windows.Forms;','').Replace('using System.Data;','').Replace('using System.ComponentModel;','')
$bridge=@"
public static class LrleBatchHashOracle {
 public static void Run(){using var reader=new System.IO.BinaryReader(System.Console.OpenStandardInput());var count=reader.ReadInt32();if(count<1||count>1024)throw new System.Exception("test batch size");var json=new System.Text.StringBuilder("[");
 for(int n=0;n<count;n++){int length=reader.ReadInt32();if(length<16||length>64*1024*1024)throw new System.Exception("test payload size");byte[] bytes=reader.ReadBytes(length);if(bytes.Length!=length)throw new System.Exception("test payload truncated");using var memory=new System.IO.MemoryStream(bytes,false);using var br=new System.IO.BinaryReader(memory);var lrle=new TS4SimRipper.LRLE(br);using var image=lrle.image;
 var rect=new System.Drawing.Rectangle(0,0,image.Width,image.Height);var data=image.LockBits(rect,System.Drawing.Imaging.ImageLockMode.ReadOnly,System.Drawing.Imaging.PixelFormat.Format32bppArgb);try{if(data.Stride!=image.Width*4)throw new System.Exception("test image stride");byte[] rgba=new byte[data.Stride*image.Height];System.Runtime.InteropServices.Marshal.Copy(data.Scan0,rgba,0,rgba.Length);for(int i=0;i<rgba.Length;i+=4){byte t=rgba[i];rgba[i]=rgba[i+2];rgba[i+2]=t;}string hash=System.Convert.ToHexString(System.Security.Cryptography.SHA256.HashData(rgba)).ToLowerInvariant();if(n>0)json.Append(",");json.Append("{\"width\":").Append(image.Width).Append(",\"height\":").Append(image.Height).Append(",\"rgbaHash\":\"").Append(hash).Append("\"}");}finally{image.UnlockBits(data);}}
 json.Append("]");System.Console.Write(json.ToString());
 }
}
"@
Add-Type -TypeDefinition ($source+"`n"+$bridge) -ReferencedAssemblies @('System.Drawing.Common','System.Private.Windows.GdiPlus','System.Private.Windows.Core','System.Drawing.Primitives','System.Runtime','System.Linq','System.Collections','System.IO','System.Console','System.Runtime.InteropServices','System.Security.Cryptography')
[LrleBatchHashOracle]::Run()
