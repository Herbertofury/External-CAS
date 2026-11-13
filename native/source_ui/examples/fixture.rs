use ruffle_core::{PlayerBuilder,tag_utils::SwfMovieData,backend::log::LogBackend,limits::ExecutionLimit};
use std::time::Duration;
struct Log;
impl LogBackend for Log {fn avm_trace(&self,s:&str){println!("trace:{s}");}fn avm_warning(&self,s:&str){println!("warning:{s}");}fn avm_error(&self,s:&str){println!("error:{s}");}}
fn main()->Result<(),Box<dyn std::error::Error>>{let path=std::env::args().nth(1).ok_or("fixture explicit-own-test.swf")?;let bytes=std::fs::read(&path)?;let movie=SwfMovieData::from_data(&bytes,"https://source-ui.invalid/own-test.swf".into(),None,None)?;let p=PlayerBuilder::new().with_movie(movie).with_log(Log).with_autoplay(true).build();for _ in 0..3{let mut p=p.lock().unwrap();p.preload(&mut ExecutionLimit::with_max_ops_and_time(10000,Duration::from_millis(20)));p.run_frame();}Ok(())}
