// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit static img:// source boundary. No filesystem or network fallback.
use external_cas_fidelity::installed::SourcePin;
use std::sync::{Arc, atomic::AtomicBool};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticImageRequest {
    pub instance: u64,
    pub request_id: Option<u64>,
}
impl StaticImageRequest {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let url = url::Url::parse(raw).map_err(|_| "Invalid original static image URI")?;
        if url.scheme() != "img"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.fragment().is_some()
            || !matches!(url.path(), "" | "/")
        {
            return Err("Unsupported original static image URI shape".into());
        }
        let host = url.host_str().ok_or("Original image instance absent")?;
        let hex = host
            .strip_prefix("0x")
            .ok_or("Original image instance is not explicit hex")?;
        if hex.is_empty() || hex.len() > 16 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Original image instance width/encoding differs".into());
        }
        let instance =
            u64::from_str_radix(hex, 16).map_err(|_| "Original image instance overflow")?;
        let request_id = match url.query() {
            None => None,
            Some(q) => {
                let v = q.strip_prefix("rid=").ok_or("Unsupported image query")?;
                if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
                    return Err("Original image request correlation invalid".into());
                }
                Some(
                    v.parse()
                        .map_err(|_| "Original image correlation overflow")?,
                )
            }
        };
        Ok(Self {
            instance,
            request_id,
        })
    }
}
pub struct ResolvedStaticImage {
    pub encoded_png: Arc<[u8]>,
    pub pins: Vec<SourcePin>,
}
impl ResolvedStaticImage {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let raw = &self.encoded_png;
        if raw.len() > 64 * 1024 * 1024
            || raw.len() < 33
            || &raw[..8] != b"\x89PNG\r\n\x1a\n"
            || &raw[8..16] != b"\0\0\0\rIHDR"
        {
            return Err("Resolved static image must be bounded encoded PNG".into());
        }
        let w = u32::from_be_bytes(raw[16..20].try_into().unwrap());
        let h = u32::from_be_bytes(raw[20..24].try_into().unwrap());
        if w == 0
            || h == 0
            || w > 8192
            || h > 8192
            || u64::from(w) * u64::from(h) * 4 > 128 * 1024 * 1024
        {
            return Err("Resolved source image dimensions exceed budget".into());
        }
        if self.pins.is_empty() {
            return Err("Resolved original static image has no source pins".into());
        }
        let mut bytes = 0usize;
        for pin in &self.pins {
            for hash in [&pin.index_sha256, &pin.payload_sha256] {
                if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("Resolved static image source hash invalid".into());
                }
            }
            bytes = bytes
                .checked_add(pin.provider_path.as_os_str().len() + 160)
                .ok_or("Image pin extent overflow")?;
            if bytes > 4 * 1024 * 1024 {
                return Err("Resolved source image pin payload exceeds budget".into());
            }
        }
        Ok(())
    }
}
/// Runs off the VM/UI thread. Resolve one unique supported full TGI and provider
/// within an explicit configured scope. Never guess resource type or precedence.
/// Implementations should cache bounded decoded results and check cancellation.
pub trait StaticImageResolver: Send + Sync + 'static {
    fn resolve(&self, instance: u64, cancel: &AtomicBool) -> Result<ResolvedStaticImage, String>;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_source_uri_identity_is_independent_of_request_correlation() {
        let a = StaticImageRequest::parse("img://0xfd1aa90d823ca727?rid=1").unwrap();
        let b = StaticImageRequest::parse("img://0xfd1aa90d823ca727?rid=3").unwrap();
        assert_eq!(a.instance, 0xfd1aa90d823ca727);
        assert_eq!(a.instance, b.instance);
        assert_ne!(a.request_id, b.request_id);
    }
    #[test]
    fn unrelated_schemes_credentials_paths_queries_and_overflow_are_refused() {
        for uri in [
            "https://0x1?rid=1",
            "file:///0x1",
            "img://user@0x1",
            "img://0x1/path",
            "img://0x1#fragment",
            "img://0x1?rid=-1",
            "img://0x1?rid=1&x=2",
            "img://0x1?rid=%31",
            "img://0x10000000000000000",
            "img://1",
        ] {
            assert!(StaticImageRequest::parse(uri).is_err(), "{uri}");
        }
    }
}
