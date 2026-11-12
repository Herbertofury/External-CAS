// Development adapter only. The separately supplied current reader is linked
// unchanged; neither its source nor resulting binaries are redistributed here.
using System.Reflection;
using System.Text.Json;
using TS4AnimationBrowser.Core.ViewerV2;
using var input=new MemoryStream();Console.OpenStandardInput().CopyTo(input);
if(input.Length>16*1024*1024)throw new Exception("Input budget");
var raw=input.ToArray();var metadata=StudioClipDecoderV2.ReadMetadata(raw);var clip=StudioClipDecoderV2.Decode(raw);
var method=typeof(StudioClipDecoderV2).GetMethod("ReadEnvelope",BindingFlags.NonPublic|BindingFlags.Static)??throw new Exception("Primary envelope method missing");
var envelope=method.Invoke(null,new object[]{raw})??throw new Exception("Envelope absent");
object Property(string name)=>envelope.GetType().GetProperty(name)!.GetValue(envelope)!;
Console.Write(JsonSerializer.Serialize(new{version=metadata.ResourceVersion,actor=metadata.RigNamespace,rig=metadata.RigInstance,codecOffset=Property("CodecOffset"),codecLength=Property("CodecLength"),name=clip.Name,sourceName=clip.SourceName,ticks=clip.MaxFrameCount,tickLength=clip.FrameDuration,tracks=clip.Tracks.Select(t=>new{hash=t.TrackKey,position=t.Position?.Frames.Select(f=>new{tick=f.FrameIndex,bits=f.Values.Select(BitConverter.SingleToInt32Bits)}),rotation=t.Orientation?.Frames.Select(f=>new{tick=f.FrameIndex,bits=f.Values.Select(BitConverter.SingleToInt32Bits)})})}));
