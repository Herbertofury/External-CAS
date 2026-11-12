// SPDX-License-Identifier: GPL-3.0-or-later
// Storage shim only; deformation methods are extracted verbatim from pinned
// source. This does not assert the C# GEOM parser was used for input arrays.
using System;using System.IO;using System.Linq;using System.Security.Cryptography;using System.Text.Json;using TS4SimRipper;
namespace System.Windows.Forms { public class Form{} public static class MessageBox{public static void Show(string s){throw new Exception(s);}} }
namespace TS4SimRipper {
public partial class GEOM {
 public class position{public float[] value;public position(float[] v){value=(float[])v.Clone();}}
 private position[] vPositions;private float[][] norms;private byte[][] bones,weights;private uint[] bonehasharray;private Vector3[] deltaPosition;
 public string Instance="oracle",package="oracle";
 public GEOM(float[][] p,float[][] n,byte[][] b,byte[][] w,uint[] palette){vPositions=p.Select(v=>new position(v)).ToArray();norms=n;bones=b;weights=w;bonehasharray=palette;}
 public float[] getPosition(int i)=>(float[])vPositions[i].value.Clone();public float[] getNormal(int i)=>(float[])norms[i].Clone();public void setNormal(int i,float[] v){norms[i]=(float[])v.Clone();}public byte[] getBones(int i)=>bones[i];public byte[] getBoneWeights(int i)=>weights[i];public int count=>vPositions.Length;
 }
}
class Program {
static string Hash(GEOM g,bool normal){using var s=new MemoryStream();using var w=new BinaryWriter(s);for(int i=0;i<g.count;i++)foreach(float f in normal?g.getNormal(i):g.getPosition(i))w.Write(f);w.Flush();return Convert.ToHexString(SHA256.HashData(s.ToArray())).ToLowerInvariant();}
static float[] F(BinaryReader r){var a=new[]{r.ReadSingle(),r.ReadSingle(),r.ReadSingle()};if(a.Any(x=>!float.IsFinite(x)))throw new Exception("nonfinite");return a;}
static byte[] Data(BinaryReader r,int max){int n=r.ReadInt32();if(n<0||n>max)throw new Exception("extent");var b=r.ReadBytes(n);if(b.Length!=n)throw new Exception("truncated");return b;}
static void Main(){using var s=new MemoryStream();Console.OpenStandardInput().CopyTo(s);if(s.Length>20*1024*1024)throw new Exception("bound");s.Position=0;using var r=new BinaryReader(s);var rig=new RIG(new BinaryReader(new MemoryStream(Data(r,2*1024*1024))));var bond=new BOND(new BinaryReader(new MemoryStream(Data(r,2*1024*1024))));float weight=r.ReadSingle();if(!float.IsFinite(weight))throw new Exception("weight");bond.weight=weight;int count=r.ReadInt32();if(count<1||count>10000)throw new Exception("count");var p=new float[count][];var n=new float[count][];var b=new byte[count][];var w=new byte[count][];for(int i=0;i<count;i++){p[i]=F(r);n[i]=F(r);b[i]=r.ReadBytes(4);w[i]=r.ReadBytes(4);}int pc=r.ReadInt32();if(pc<1||pc>256)throw new Exception("palette");var palette=new uint[pc];for(int i=0;i<pc;i++)palette[i]=r.ReadUInt32();if(s.Position!=s.Length)throw new Exception("trailing");var geom=new GEOM(p,n,b,w,palette);PreviewControl.LoadBONDMorph(null,new[]{geom},bond,rig,false);Console.Write(JsonSerializer.Serialize(new{positionHash=Hash(geom,false),normalHash=Hash(geom,true)}));}
}
