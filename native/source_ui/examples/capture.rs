use external_cas_source_ui::*;
use external_cas_assets::PackageIndex;
use std::{sync::{Arc,atomic::AtomicBool},time::Duration};
struct Host;
impl UiHost for Host {fn call(&self,r:&HostRequest)->HostReply{
    // PC source-protocol compatibility fields; actual GPU backend is reported separately.
    // distributionPlatform -1 is an unqualified standalone sentinel, not an EA/Steam identity.
    // Actual standalone editor context enum values only. No Sim/catalog data
    // or mutation success is fabricated by this source-runtime smoke test.
    if r.bridge==HostBridge::CallGameService {match r.name.as_str(){"IsGameClient"=>return HostReply::Value(HostValue::Object(std::collections::BTreeMap::from([("isDebug".into(),HostValue::Bool(false)),("usingController".into(),HostValue::Bool(false)),("usingMouseKeyboard".into(),HostValue::Bool(false)),("razerEnabledPC".into(),HostValue::Bool(false)),("swapJPControls".into(),HostValue::Bool(false)),("graphicsAPI".into(),HostValue::String("DX11".into())),("distributionPlatform".into(),HostValue::Number(-1.))]))),"GetGameState"=>return HostReply::Value(HostValue::Number(983016380.)),"CasGetCASEditMode"=>return HostReply::Value(HostValue::Number(0.)),_=>{}}}
    HostReply::Unavailable("not bound to verified editor data in this isolated test".into())
}}
fn main()->Result<(),Box<dyn std::error::Error>>{
    tracing_subscriber::fmt().with_env_filter("warn,ruffle_core=info").with_writer(std::io::stderr).init();
    let a=std::env::args().skip(1).collect::<Vec<_>>();if a.len()<2{return Err("capture explicit-UI.package private-output.png [root-name] [--full-cas]".into());}
    let index=PackageIndex::open(&a[0])?;let root=a.get(2).map(String::as_str).unwrap_or("UIContainer");
    let locale=a.iter().position(|s|s=="--locale").and_then(|i|a.get(i+1));let aliases=locale.map(|l|locale_aliases(l)).transpose()?.unwrap_or_default();
    let bundle=prepare_movie_bundle_with_aliases(&index,root,a.iter().any(|s|s=="--full-cas"),&aliases,None,&AtomicBool::new(false))?;
    let pins=bundle.source_pins.clone();let preparation=bundle.preparation.clone();
    let instance=ruffle_render_wgpu::backend::create_wgpu_instance(wgpu::Backends::DX12,wgpu::BackendOptions::default(),None);
    // The test obtains one device, then passes clones to the reusable API exactly
    // as eframe's existing RenderState would. Runtime creates no second device.
    let (adapter,device,queue)=futures::executor::block_on(ruffle_render_wgpu::backend::request_adapter_and_device(wgpu::Backends::DX12,&instance,None,wgpu::PowerPreference::HighPerformance))?;
    let info=adapter.get_info();
    let mut runtime=SourceUiRuntime::new(SharedGpu{instance,adapter,device:device.clone(),queue:queue.clone(),target_format:wgpu::TextureFormat::Rgba8Unorm},bundle,Arc::new(Host),[1000,750],1.)?;
    for _ in 0..30 {runtime.tick(Duration::from_secs_f64(1./30.));}
    let frame=runtime.render()?;
    let row=(frame.dimensions[0]*4).div_ceil(256)*256;
    let buffer=device.create_buffer(&wgpu::BufferDescriptor{label:Some("explicit source UI diagnostic readback"),size:u64::from(row)*u64::from(frame.dimensions[1]),usage:wgpu::BufferUsages::COPY_DST|wgpu::BufferUsages::MAP_READ,mapped_at_creation:false});
    let mut encoder=device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(frame.texture.as_image_copy(),wgpu::TexelCopyBufferInfo{buffer:&buffer,layout:wgpu::TexelCopyBufferLayout{offset:0,bytes_per_row:Some(row),rows_per_image:None}},wgpu::Extent3d{width:frame.dimensions[0],height:frame.dimensions[1],depth_or_array_layers:1});
    queue.submit([encoder.finish()]);
    let(tx,rx)=std::sync::mpsc::channel();buffer.slice(..).map_async(wgpu::MapMode::Read,move|r|{let _=tx.send(r);});device.poll(wgpu::PollType::wait_indefinitely())?;rx.recv()??;
    let mapped=buffer.slice(..).get_mapped_range()?;let mut rgba=Vec::new();for source_row in mapped.chunks_exact(row as usize){rgba.extend_from_slice(&source_row[..frame.dimensions[0]as usize*4]);}
    let transparent=rgba.chunks_exact(4).filter(|p|p[3]==0).count();let visible=rgba.chunks_exact(4).filter(|p|p[3]>0).count();
    image::RgbaImage::from_raw(frame.dimensions[0],frame.dimensions[1],rgba).ok_or("readback extent")?.save(&a[1])?;
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"private":true,"shared_gpu_adapter":info.name,"backend":format!("{:?}",info.backend),"source_pins":pins,"preparation":preparation,"transparent_pixels":transparent,"visible_pixels":visible,"movie_frame":runtime.movie_frame(),"status":format!("{:?}",runtime.status()),"events":format!("{:?}",runtime.drain_events()),"native_game_execution":false,"full_ui_qualified":false}))?);
    Ok(())
}
