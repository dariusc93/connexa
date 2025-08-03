#![cfg(target_arch = "wasm32")]

use connexa::prelude::{DefaultConnexaBuilder, Multiaddr};
use send_wrapper::SendWrapper;
use std::ops::Deref;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use web_sys::{Document, HtmlElement};

#[wasm_bindgen]
pub async fn run(address: String) -> Result<(), JsError> {
    tracing_wasm::set_as_global_default();

    let addr = address.parse::<Multiaddr>()?;

    let body = Body::from_current_window()?;

    let connexa = DefaultConnexaBuilder::new_identity()
        .enable_webrtc()
        .with_ping()
        .set_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(30)))
        .set_swarm_event_callback({
            let body = body.clone();
            move |e| {
                body.append_p(&format!("{e:?}"))
                    .expect("failed to append <p>");
            }
        })
        .build()?;

    body.append_p(&format!("Dialing {addr}"))?;

    connexa.swarm().dial(addr).await?;

    futures::future::pending::<()>().await;

    Ok(())
}

#[derive(Clone)]
struct Body {
    inner: SendWrapper<Rc<BodyInner>>,
}

impl Body {
    fn from_current_window() -> Result<Self, JsError> {
        // Use `web_sys`'s global `window` function to get a handle on the global
        // window object.
        let document = web_sys::window()
            .ok_or(js_error("no global `window` exists"))?
            .document()
            .ok_or(js_error("should have a document on window"))?;
        let body = document
            .body()
            .ok_or(js_error("document should have a body"))?;

        let inner = BodyInner { body, document };

        Ok(Body {
            inner: SendWrapper::new(Rc::new(inner)),
        })
    }
}

struct BodyInner {
    body: HtmlElement,
    document: Document,
}

impl Deref for Body {
    type Target = BodyInner;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl BodyInner {
    fn append_p(&self, msg: &str) -> Result<(), JsError> {
        let val = self
            .document
            .create_element("p")
            .map_err(|_| js_error("failed to create <p>"))?;
        val.set_text_content(Some(msg));
        self.body
            .append_child(&val)
            .map_err(|_| js_error("failed to append <p>"))?;

        Ok(())
    }
}

fn js_error(msg: &str) -> JsError {
    std::io::Error::other(msg).into()
}
