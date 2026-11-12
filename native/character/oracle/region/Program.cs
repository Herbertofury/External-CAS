// SPDX-License-Identifier: GPL-3.0-or-later
// Links unchanged source RegionMap/TGI/Enums; CASP shim only satisfies an unused
// SortLODs method and does not participate in these reader/query comparisons.
using System;using System.IO;using System.Linq;using System.Text.Json;using TS4SimRipper;
namespace TS4SimRipper {public class CASP{public int getLOD(TGI key)=>throw new Exception("unused");}}
class Program{static void Main(){using var s=new MemoryStream();Console.OpenStandardInput().CopyTo(s);if(s.Length>8*1024*1024)throw new Exception("bound");s.Position=0;var map=new RegionMap(new BinaryReader(s));Console.Write(JsonSerializer.Serialize(Enumerable.Range(0,map.NumberRegions).Select(i=>new{region=map.GetRegion(i),layer=map.GetLayer(i),isReplacement=map.GetIsReplacement(i),keys=map.GetTGIlist(i).Select(k=>new{type=k.Type,group=k.Group,instance=k.Instance})})));}}
