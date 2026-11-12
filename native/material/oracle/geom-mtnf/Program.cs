using System.Reflection;
using System.Text.Json;
using PinnedReference;
using var input=JsonDocument.Parse(Console.In.ReadToEnd());
var output=new List<object>();
foreach(var item in input.RootElement.GetProperty("materials").EnumerateArray()){
    var bytes=Convert.FromHexString(item.GetProperty("hex").GetString());
    using var stream=new MemoryStream(bytes);using var reader=new BinaryReader(stream);
    var material=new GEOM.MTNF(reader);
    var headers=(uint[][])typeof(GEOM.MTNF).GetField("paramList",BindingFlags.NonPublic|BindingFlags.Instance).GetValue(material);
    var values=(object[][])typeof(GEOM.MTNF).GetField("dataList",BindingFlags.NonPublic|BindingFlags.Instance).GetValue(material);
    output.Add(new{key=item.GetProperty("key").GetString(),consumed=stream.Position,normal_index=material.normalIndex,emission_index=material.emissionIndex,parameters=headers.Select((header,index)=>new{hash=header[0],data_type=header[1],count=header[2],offset=header[3],words=values[index].Select(value=>value is float f?BitConverter.SingleToUInt32Bits(f):value is int n?unchecked((uint)n):(uint)value).ToArray()}).ToArray(),roundtrip_words=material.toDataArray()});
}
Console.Write(JsonSerializer.Serialize(output));
