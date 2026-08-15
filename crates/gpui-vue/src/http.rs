//! Native HTTP bridge used by image-backed `gpui-vue` elements.
//!
//! Applications normally do not need this module. It exists so a native app
//! can install a transport for [`crate::ui::image`] without depending on or
//! naming GPUI's transport module directly.

pub use gpui::http_client::{
    AsyncBody, HttpClient, Inner as BodyInner, Request, Response, Result as HttpResult, Url,
    anyhow, http,
};
