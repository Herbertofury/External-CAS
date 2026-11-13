// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{Audit, PreparedResource, RuntimeEvent};
use ruffle_core::{
    backend::navigator::*,
    loader::Error,
    socket::{ConnectionState, SocketAction, SocketHandle},
};
use std::{borrow::Cow, collections::BTreeMap, sync::Arc, time::Duration};
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
}
impl Navigator {
    pub fn new(
        resources: BTreeMap<String, PreparedResource>,
        spawner: NullSpawner,
        audit: Audit,
    ) -> Self {
        Self {
            resources,
            spawner,
            audit,
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
