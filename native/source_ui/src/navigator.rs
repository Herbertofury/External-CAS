// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Audit, PreparedResource, RuntimeEvent, StaticImageRequest, StaticImageResolver};
use ruffle_core::{
    backend::navigator::*,
    loader::Error,
    socket::{ConnectionState, SocketAction, SocketHandle},
};
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use url::Url;
struct Response {
    url: String,
    bytes: Arc<[u8]>,
    read: bool,
}
impl SuccessResponse for Response {
    fn url(&self) -> Cow<'_, str> {
        Cow::Borrowed(&self.url)
    }
    fn set_url(&mut self, u: String) {
        self.url = u;
    }
    fn body(self: Box<Self>) -> OwnedFuture<Vec<u8>, Error> {
        Box::pin(async move { Ok(self.bytes.to_vec()) })
    }
    fn text_encoding(&self) -> Option<&'static encoding_rs::Encoding> {
        Some(encoding_rs::UTF_8)
    }
    fn status(&self) -> u16 {
        200
    }
    fn redirected(&self) -> bool {
        false
    }
    fn next_chunk(&mut self) -> OwnedFuture<Option<Vec<u8>>, Error> {
        let bytes = if self.read {
            None
        } else {
            self.read = true;
            Some(self.bytes.to_vec())
        };
        Box::pin(async move { Ok(bytes) })
    }
    fn expected_length(&self) -> Result<Option<u64>, Error> {
        Ok(Some(self.bytes.len() as u64))
    }
}
pub(crate) struct Navigator {
    resources: BTreeMap<String, PreparedResource>,
    spawner: NullSpawner,
    audit: Audit,
    images: Option<Arc<dyn StaticImageResolver>>,
    cancel: Arc<AtomicBool>,
    in_flight: Arc<AtomicUsize>,
}
impl Navigator {
    pub fn new(
        resources: BTreeMap<String, PreparedResource>,
        spawner: NullSpawner,
        audit: Audit,
        images: Option<Arc<dyn StaticImageResolver>>,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            resources,
            spawner,
            audit,
            images,
            cancel,
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }
}
impl NavigatorBackend for Navigator {
    fn navigate_to_url(
        &self,
        url: &str,
        _: &str,
        _: Option<(NavigationMethod, indexmap::IndexMap<String, String>)>,
    ) {
        self.audit
            .event(RuntimeEvent::Denied(format!("navigation:{url}")));
    }
    fn fetch(&self, r: Request) -> OwnedFuture<Box<dyn SuccessResponse>, ErrorResponse> {
        if r.url().starts_with("img:") {
            let requested = r.url().to_owned();
            let result = (|| -> Result<_, String> {
                if r.method().to_string() != "GET" {
                    return Err("Original static image accepts only GET".into());
                }
                let image = StaticImageRequest::parse(&requested)?;
                let resolver = self
                    .images
                    .clone()
                    .ok_or("Original static image resolver is unavailable")?;
                if self.cancel.load(Ordering::Relaxed) {
                    return Err("Original image runtime cancelled".into());
                }
                let mut count = self.in_flight.load(Ordering::Relaxed);
                loop {
                    if count >= 8 {
                        return Err("Original image worker concurrency budget reached".into());
                    }
                    match self.in_flight.compare_exchange_weak(
                        count,
                        count + 1,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(actual) => count = actual,
                    }
                }
                let (send, receive) = async_channel::bounded(1);
                let cancel = self.cancel.clone();
                let flight = self.in_flight.clone();
                let audit = self.audit.clone();
                let spawned = std::thread::Builder::new()
                    .name("original-static-image".into())
                    .spawn(move || {
                        struct Permit(Arc<AtomicUsize>);
                        impl Drop for Permit {
                            fn drop(&mut self) {
                                self.0.fetch_sub(1, Ordering::AcqRel);
                            }
                        }
                        let _permit = Permit(flight);
                        let result = (|| -> Result<Arc<[u8]>, String> {
                            if cancel.load(Ordering::Relaxed) {
                                return Err("Original image runtime cancelled".into());
                            }
                            let found = resolver.resolve(image.instance, &cancel)?;
                            found.validate()?;
                            if cancel.load(Ordering::Relaxed) {
                                return Err("Obsolete original image resolution cancelled".into());
                            }
                            audit.accept_image_pins(&found.pins)?;
                            audit.event(RuntimeEvent::ImageResolved {
                                request: image,
                                pins: found.pins,
                                encoded_png_sha256: format!(
                                    "{:x}",
                                    Sha256::digest(&found.encoded_png)
                                ),
                            });
                            Ok(found.encoded_png)
                        })();
                        let _ = send.send_blocking(result);
                    });
                if let Err(e) = spawned {
                    self.in_flight.fetch_sub(1, Ordering::AcqRel);
                    return Err(format!("Original image worker could not start: {e}"));
                }
                Ok(receive)
            })();
            let audit = self.audit.clone();
            return Box::pin(async move {
                let result = match result {
                    Ok(receiver) => receiver
                        .recv()
                        .await
                        .map_err(|_| "Original image worker disconnected".to_owned())
                        .and_then(|r| r),
                    Err(e) => Err(e),
                };
                match result {
                    Ok(bytes) => Ok(Box::new(Response {
                        url: requested,
                        bytes,
                        read: false,
                    }) as Box<dyn SuccessResponse>),
                    Err(reason) => {
                        audit.event(RuntimeEvent::Denied(format!(
                            "static image {requested}: {reason}"
                        )));
                        Err(create_fetch_error(&requested, &reason))
                    }
                }
            });
        }
        let parsed = self.resolve_url(r.url());
        let selected = parsed
            .ok()
            .filter(|u| {
                u.host_str() == Some("source-ui.invalid")
                    && u.scheme() == "https"
                    && u.query().is_none()
                    && u.fragment().is_none()
                    && r.method().to_string() == "GET"
            })
            .and_then(|u| {
                let key = u.path().strip_prefix("/UI/")?.to_ascii_lowercase();
                let found = self.resources.get(&key)?;
                Some((u.to_string(), found.runtime_bytes.clone()))
            });
        let requested = r.url().to_string();
        if selected.is_none() {
            self.audit.event(RuntimeEvent::Denied(format!(
                "unresolved source resource:{requested}"
            )));
        }
        Box::pin(async move {
            match selected {
                Some((url, bytes)) => Ok(Box::new(Response {
                    url,
                    bytes,
                    read: false,
                }) as Box<dyn SuccessResponse>),
                None => Err(create_fetch_error(
                    &requested,
                    "only exact authorized in-memory source resources are available; filesystem/network disabled",
                )),
            }
        })
    }
    fn resolve_url(&self, u: &str) -> Result<Url, url::ParseError> {
        Url::parse(u).or_else(|_| Url::parse("https://source-ui.invalid/UI/")?.join(u))
    }
    fn spawn_future(&mut self, f: OwnedFuture<(), Error>) {
        self.spawner.spawn_local(f)
    }
    fn pre_process_url(&self, u: Url) -> Url {
        u
    }
    fn connect_socket(
        &mut self,
        host: String,
        _: u16,
        _: Duration,
        h: SocketHandle,
        _: async_channel::Receiver<Vec<u8>>,
        s: async_channel::Sender<SocketAction>,
    ) {
        self.audit
            .event(RuntimeEvent::Denied(format!("socket:{host}")));
        let _ = s.try_send(SocketAction::Connect(h, ConnectionState::Failed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResolvedStaticImage;
    use external_cas_assets::ResourceKey;
    use external_cas_fidelity::installed::SourcePin;
    use ruffle_core::backend::navigator::NullExecutor;
    struct Resolver {
        caller: std::thread::ThreadId,
        calls: Arc<AtomicUsize>,
    }
    impl StaticImageResolver for Resolver {
        fn resolve(&self, instance: u64, _: &AtomicBool) -> Result<ResolvedStaticImage, String> {
            assert_ne!(
                std::thread::current().id(),
                self.caller,
                "disk/decoder resolver must not run in the VM callback"
            );
            self.calls.fetch_add(1, Ordering::Relaxed);
            if instance != 1 {
                return Err("explicit instance is unavailable".into());
            }
            let mut data = Vec::new();
            let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([17, 39, 88, 255]));
            image
                .write_to(
                    &mut std::io::Cursor::new(&mut data),
                    image::ImageFormat::Png,
                )
                .unwrap();
            Ok(ResolvedStaticImage {
                encoded_png: data.into(),
                pins: vec![pin(1)],
            })
        }
    }
    fn pin(instance: u64) -> SourcePin {
        SourcePin {
            provider_path: "C:\\own-fixture.package".into(),
            key: ResourceKey::new(0x00b2d882, 0, instance),
            ordinal: 0,
            index_sha256: "a".repeat(64),
            payload_sha256: "b".repeat(64),
        }
    }
    #[test]
    fn resolver_runs_off_vm_thread_and_retains_verified_bytes_and_pins() {
        let executor = NullExecutor::new();
        let audit = Audit::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let nav = Navigator::new(
            BTreeMap::new(),
            executor.spawner(),
            audit.clone(),
            Some(Arc::new(Resolver {
                caller: std::thread::current().id(),
                calls: calls.clone(),
            })),
            cancel.clone(),
        );
        let response =
            futures::executor::block_on(nav.fetch(Request::get("img://0x1?rid=42".into())))
                .unwrap_or_else(|_| panic!("own source image resolution failed"));
        let bytes = futures::executor::block_on(response.body()).unwrap();
        assert!(bytes.starts_with(b"\x89PNG"));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(audit.0.lock().unwrap().image_pins.len(), 1);
        assert!(
            futures::executor::block_on(nav.fetch(Request::get("img://0x2?rid=43".into())))
                .is_err()
        );
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        cancel.store(true, Ordering::Relaxed);
        assert!(
            futures::executor::block_on(nav.fetch(Request::get("img://0x1?rid=44".into())))
                .is_err()
        );
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }
    #[test]
    fn changed_source_index_or_payload_cannot_mix_with_accepted_source_context() {
        let audit = Audit::new();
        audit.0.lock().unwrap().initial_pins = vec![pin(7)];
        let mut changed = pin(1);
        changed.index_sha256 = "c".repeat(64);
        assert!(
            audit
                .accept_image_pins(&[changed])
                .unwrap_err()
                .contains("index changed")
        );
        audit.accept_image_pins(&[pin(1)]).unwrap();
        let mut changed = pin(1);
        changed.payload_sha256 = "c".repeat(64);
        assert!(
            audit
                .accept_image_pins(&[changed])
                .unwrap_err()
                .contains("source changed")
        );
        assert_eq!(audit.0.lock().unwrap().image_pins.len(), 1);
    }
}
