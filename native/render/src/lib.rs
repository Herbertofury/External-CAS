//! Native original-shader execution boundary. No game executable or private
//! engine ABI is loaded. The installed assets, bindings and frame policy remain
//! explicit; a D3D bridge is not complete CAS rendering acceptance.
#[cfg(windows)]
mod on12;
#[cfg(windows)]
pub use on12::*;
