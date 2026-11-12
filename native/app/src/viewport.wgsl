struct Camera { view_projection: mat4x4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> palette: array<mat4x4<f32>>;
struct AppearanceTransform { matrix:mat4x4<f32> }
@group(0) @binding(2) var<uniform> appearance:AppearanceTransform;
struct InstalledLighting {ambient:vec4<f32>,key_direction:array<vec4<f32>,2>,key_color:array<vec4<f32>,2>,rim_color:array<vec4<f32>,2>,reserved:array<vec4<f32>,2>}
@group(0) @binding(3) var<uniform> lighting:InstalledLighting;
@group(1) @binding(0) var diffuse: texture_2d<f32>;
@group(1) @binding(1) var material_sampler: sampler;
struct ColorModifiers { hsv_opacity:vec4<f32> }
@group(1) @binding(2) var<uniform> modifiers:ColorModifiers;
fn srgb_from_linear(c:vec3<f32>)->vec3<f32>{return select(1.055*pow(max(c,vec3<f32>(0.0)),vec3<f32>(1.0/2.4))-0.055,c*12.92,c<=vec3<f32>(0.0031308));}
fn linear_from_srgb(c:vec3<f32>)->vec3<f32>{return select(pow((c+0.055)/1.055,vec3<f32>(2.4)),c/12.92,c<=vec3<f32>(0.04045));}
fn shifted_color(encoded:vec3<f32>,shift:vec3<f32>)->vec3<f32>{
    let hi=max(max(encoded.r,encoded.g),encoded.b);let lo=min(min(encoded.r,encoded.g),encoded.b);let delta=hi-lo;
    var h=0.0;
    if delta>0.0 {
        if hi==encoded.r {h=(encoded.g-encoded.b)/delta;}
        else if hi==encoded.g {h=(encoded.b-encoded.r)/delta+2.0;}
        else {h=(encoded.r-encoded.g)/delta+4.0;}
        h=fract(h/6.0+1.0);
    }
    h=fract(h+shift.x+2.0);
    var saturation=0.0;if hi>0.0{saturation=delta/hi;}
    saturation=clamp(saturation+shift.y,0.0,1.0);let v=clamp(hi+shift.z,0.0,1.0);
    let sector=h*6.0;let f=fract(sector);let low=v*(1.0-saturation);let falling=v*(1.0-saturation*f);let rising=v*(1.0-saturation*(1.0-f));
    switch u32(floor(sector)) {case 0u:{return vec3<f32>(v,rising,low);}case 1u:{return vec3<f32>(falling,v,low);}case 2u:{return vec3<f32>(low,v,rising);}case 3u:{return vec3<f32>(low,falling,v);}case 4u:{return vec3<f32>(rising,low,v);}default:{return vec3<f32>(v,low,falling);}}
}
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
    output.clip = camera.view_projection * appearance.matrix * skin * vec4<f32>(input.position,1.0);
    let a = skin[0].xyz;
    let b = skin[1].xyz;
    let c = skin[2].xyz;
    let cofactor = mat3x3<f32>(cross(b,c),cross(c,a),cross(a,b));
    let det = dot(a,cross(b,c));
    output.normal = cofactor * input.normal / select(-max(abs(det),0.00000001),max(abs(det),0.00000001),det >= 0.0);
    output.normal=(appearance.matrix*vec4<f32>(output.normal,0.0)).xyz;
    output.uv = input.uv;
    return output;
}
@fragment fn fs_main(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var color = textureSample(diffuse, material_sampler, input.uv);
    // This first native material implements opaque/cutout diffuse only.
    // Translucency, Sims shader families and layer compositing require qualification.
    if color.a < 0.5 { discard; }
    if modifiers.hsv_opacity.w<=0.0 {discard;}
    if any(modifiers.hsv_opacity.xyz!=vec3<f32>(0.0)){
        color=vec4<f32>(linear_from_srgb(shifted_color(srgb_from_linear(color.rgb),modifiers.hsv_opacity.xyz)),color.a);
    }
    var normal = input.normal / max(length(input.normal),0.00000001);
    if !front { normal = -normal; }
    let key = max(dot(normal,normalize(vec3<f32>(-0.6,1.0,0.75))),0.0);
    let fill = max(dot(normal,normalize(vec3<f32>(0.9,0.5,0.2))),0.0);
    let rim = max(dot(normal,normalize(vec3<f32>(0.1,0.8,-0.9))),0.0);
    var light = vec3<f32>(0.28) + vec3<f32>(0.72,0.68,0.61)*key + vec3<f32>(0.24,0.29,0.34)*fill + vec3<f32>(0.18,0.21,0.22)*rim;
    if lighting.ambient.z>0.0 {
        light=vec3<f32>(mix(lighting.ambient.x,lighting.ambient.y,normal.y*0.5+0.5));
        for(var i=0u;i<2u;i+=1u){light+=lighting.key_color[i].rgb*max(dot(normal,lighting.key_direction[i].xyz),0.0);}
        // Exact source colors/strengths; rim direction and transfer function
        // remain explicitly unqualified pending original shader comparison.
        light+=lighting.rim_color[0].rgb*pow(max(-normal.x,0.0),lighting.rim_color[0].w);
        light+=lighting.rim_color[1].rgb*pow(max(normal.x,0.0),lighting.rim_color[1].w);
    }
    return vec4<f32>(color.rgb*light,modifiers.hsv_opacity.w);
}

struct RoomMeshInput{@location(0) position:vec3<f32>,@location(1) normal:vec3<f32>,@location(2) uv:vec2<f32>,@location(3) color:vec4<f32>}
struct RoomMeshOutput{@builtin(position) clip:vec4<f32>,@location(0) uv:vec2<f32>,@location(1) color:vec4<f32>}
@vertex fn vs_room_mesh(input:RoomMeshInput)->RoomMeshOutput{var output:RoomMeshOutput;output.clip=camera.view_projection*vec4<f32>(input.position,1.0);output.uv=input.uv;output.color=input.color;return output;}
@fragment fn fs_room_mesh(input:RoomMeshOutput)->@location(0)vec4<f32>{let texel=textureSample(diffuse,material_sampler,input.uv);return texel*input.color*modifiers.hsv_opacity;}

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
