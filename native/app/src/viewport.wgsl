struct Camera { view_projection: mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> palette: array<mat4x4<f32>>;
@group(1) @binding(0) var diffuse: texture_2d<f32>;
@group(1) @binding(1) var material_sampler: sampler;
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) joints: vec4<u32>,
    @location(4) weights: vec4<f32>,
}
struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
}
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    let weights = input.weights / dot(input.weights,vec4<f32>(1.0));
    let skin = palette[input.joints.x]*weights.x + palette[input.joints.y]*weights.y + palette[input.joints.z]*weights.z + palette[input.joints.w]*weights.w;
    var output: VertexOutput;
    output.clip = camera.view_projection * skin * vec4<f32>(input.position,1.0);
    let a = skin[0].xyz;
    let b = skin[1].xyz;
    let c = skin[2].xyz;
    let cofactor = mat3x3<f32>(cross(b,c),cross(c,a),cross(a,b));
    let det = dot(a,cross(b,c));
    output.normal = cofactor * input.normal / select(-max(abs(det),0.00000001),max(abs(det),0.00000001),det >= 0.0);
    output.uv = input.uv;
    return output;
}
@fragment fn fs_main(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    let color = textureSample(diffuse, material_sampler, input.uv);
    // This first native material implements opaque/cutout diffuse only.
    // Translucency, Sims shader families and layer compositing require qualification.
    if color.a < 0.5 { discard; }
    var normal = input.normal / max(length(input.normal),0.00000001);
    if !front { normal = -normal; }
    let key = max(dot(normal,normalize(vec3<f32>(-0.6,1.0,0.75))),0.0);
    let fill = max(dot(normal,normalize(vec3<f32>(0.9,0.5,0.2))),0.0);
    let rim = max(dot(normal,normalize(vec3<f32>(0.1,0.8,-0.9))),0.0);
    let light = vec3<f32>(0.28) + vec3<f32>(0.72,0.68,0.61)*key + vec3<f32>(0.24,0.29,0.34)*fill + vec3<f32>(0.18,0.21,0.22)*rim;
    return vec4<f32>(color.rgb*light,1.0);
}

// Native backdrop fitted to the unmodified game-CAS reference. This owns no
// captured image/asset. Room models, lights and contact shadows have separate
// fidelity gates; this backdrop alone does not establish room parity.
const ROOM: array<vec3<f32>,30> = array<vec3<f32>,30>(
    vec3<f32>(29,87,145),vec3<f32>(60,113,170),vec3<f32>(84,139,177),vec3<f32>(104,170,168),vec3<f32>(74,158,139),vec3<f32>(30,123,98),
    vec3<f32>(36,102,165),vec3<f32>(72,127,187),vec3<f32>(121,166,202),vec3<f32>(156,205,194),vec3<f32>(107,189,153),vec3<f32>(35,131,101),
    vec3<f32>(44,106,173),vec3<f32>(86,136,201),vec3<f32>(151,189,220),vec3<f32>(180,223,206),vec3<f32>(120,197,159),vec3<f32>(42,142,110),
    vec3<f32>(46,109,183),vec3<f32>(60,117,185),vec3<f32>(79,135,187),vec3<f32>(95,159,181),vec3<f32>(77,158,160),vec3<f32>(47,149,131),
    vec3<f32>(44,107,176),vec3<f32>(53,116,178),vec3<f32>(66,131,179),vec3<f32>(81,148,177),vec3<f32>(63,144,158),vec3<f32>(44,147,130)
);
fn room_row(x:f32,row:u32)->vec3<f32> {
    var column=0u;
    var start=0.02;
    var end=0.25;
    if x>0.25 {column=1u;start=0.25;end=0.4;}
    if x>0.4 {column=2u;start=0.4;end=0.6;}
    if x>0.6 {column=3u;start=0.6;end=0.75;}
    if x>0.75 {column=4u;start=0.75;end=0.98;}
    return mix(ROOM[row*6u+column],ROOM[row*6u+column+1u],clamp((x-start)/(end-start),0.0,1.0))/255.0;
}
struct RoomVertex { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> }
@vertex fn vs_room(@builtin(vertex_index) index:u32)->RoomVertex {
    let positions=array<vec2<f32>,3>(vec2<f32>(-1,-1),vec2<f32>(3,-1),vec2<f32>(-1,3));
    var result:RoomVertex;
    result.position=vec4<f32>(positions[index],0.0,1.0);
    result.uv=vec2<f32>(positions[index].x*0.5+0.5,0.5-positions[index].y*0.5);
    return result;
}
@fragment fn fs_room(input:RoomVertex)->@location(0) vec4<f32> {
    let y=input.uv.y;
    var row=0u;var start=0.03;var end=0.25;
    if y>0.25 {row=1u;start=0.25;end=0.5;}
    if y>0.5 {row=2u;start=0.5;end=0.75;}
    if y>0.75 {row=3u;start=0.75;end=0.97;}
    let gamma=mix(room_row(input.uv.x,row),room_row(input.uv.x,row+1u),clamp((y-start)/(end-start),0.0,1.0));
    let linear=select(pow((gamma+0.055)/1.055,vec3<f32>(2.4)),gamma/12.92,gamma<=vec3<f32>(0.04045));
    return vec4<f32>(linear,1.0);
}
