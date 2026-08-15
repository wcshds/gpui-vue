//! In-memory assets embedded in a native application binary.

use std::{borrow::Cow, collections::BTreeMap};

use gpui::{AssetSource, Result, SharedString};

/// A collection of named, statically embedded application assets.
///
/// `EmbeddedAssets` lets applications install bytes from [`include_bytes!`]
/// without implementing or importing GPUI's lower-level asset-source trait.
/// Paths are UTF-8 logical identifiers rather than filesystem paths: loading
/// compares a path exactly, while listing returns every full path beginning
/// with the supplied prefix in lexicographic order. No path normalization is
/// performed, and an empty prefix lists every asset.
///
/// Adding the same path more than once replaces its previous bytes.
///
/// # Examples
///
/// ```
/// use gpui_vue::EmbeddedAssets;
///
/// static ICON: &[u8] = b"embedded image bytes";
///
/// let assets = EmbeddedAssets::new().with_file("icons/app.png", ICON);
/// assert_eq!(assets.get("icons/app.png"), Some(ICON));
/// ```
#[derive(Clone, Default)]
pub struct EmbeddedAssets {
    /// Embedded bytes keyed by their logical application path.
    files: BTreeMap<String, &'static [u8]>,
}

impl EmbeddedAssets {
    /// Creates an empty asset collection.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    /// Adds an embedded file and returns the updated collection.
    ///
    /// This builder accepts the static byte slice produced by
    /// [`include_bytes!`] directly.
    #[must_use]
    pub fn with_file(mut self, path: impl Into<String>, bytes: &'static [u8]) -> Self {
        self.insert(path, bytes);
        self
    }

    /// Adds an embedded file, returning the bytes previously stored at `path`.
    ///
    /// Paths are retained exactly as supplied, including Unicode and repeated
    /// separators.
    pub fn insert(
        &mut self,
        path: impl Into<String>,
        bytes: &'static [u8],
    ) -> Option<&'static [u8]> {
        self.files.insert(path.into(), bytes)
    }

    /// Returns the bytes stored at an exact logical path.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&'static [u8]> {
        self.files.get(path).copied()
    }

    /// Returns whether the collection has no files.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Returns the number of embedded files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Lists full logical paths beginning with `prefix` in lexicographic order.
    #[must_use]
    pub fn list(&self, prefix: &str) -> Vec<&str> {
        self.files
            .keys()
            .filter(|path| path.starts_with(prefix))
            .map(String::as_str)
            .collect()
    }
}

impl AssetSource for EmbeddedAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(self.get(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(EmbeddedAssets::list(self, path)
            .into_iter()
            .map(|path| SharedString::from(path.to_owned()))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static FIRST_ICON: &[u8] = b"first";
    static REPLACEMENT_ICON: &[u8] = b"replacement";
    static UNICODE_ICON: &[u8] = b"unicode";
    static OTHER_FILE: &[u8] = b"other";

    fn fixtures() -> EmbeddedAssets {
        EmbeddedAssets::new()
            .with_file("icons/z-last.png", FIRST_ICON)
            .with_file("icons/字形/永.png", UNICODE_ICON)
            .with_file("icons/a-first.png", FIRST_ICON)
            .with_file("metadata/app.json", OTHER_FILE)
    }

    #[test]
    fn exact_load_preserves_static_bytes_and_unicode_paths() {
        let assets = fixtures();

        assert_eq!(assets.get("icons/字形/永.png"), Some(UNICODE_ICON));
        assert_eq!(assets.get("icons/字形"), None);
        assert_eq!(assets.get("icons/字形/永.PNG"), None);

        let loaded = AssetSource::load(&assets, "icons/字形/永.png")
            .expect("embedded load should be infallible")
            .expect("fixture should exist");
        assert!(matches!(loaded, Cow::Borrowed(bytes) if bytes == UNICODE_ICON));
        assert!(
            AssetSource::load(&assets, "icons/missing.png")
                .expect("missing embedded load should be infallible")
                .is_none()
        );
    }

    #[test]
    fn list_uses_literal_prefix_full_paths_and_stable_order() {
        let assets = fixtures();

        assert_eq!(
            assets.list("icons/"),
            vec!["icons/a-first.png", "icons/z-last.png", "icons/字形/永.png"]
        );
        assert_eq!(assets.list("icons/字"), vec!["icons/字形/永.png"]);
        assert_eq!(
            assets.list(""),
            vec![
                "icons/a-first.png",
                "icons/z-last.png",
                "icons/字形/永.png",
                "metadata/app.json"
            ]
        );
        assert!(assets.list("missing/").is_empty());

        let native_paths =
            AssetSource::list(&assets, "icons/").expect("embedded listing should be infallible");
        assert_eq!(
            native_paths,
            vec![
                SharedString::from("icons/a-first.png"),
                SharedString::from("icons/z-last.png"),
                SharedString::from("icons/字形/永.png")
            ]
        );
    }

    #[test]
    fn duplicate_paths_are_replaced_without_changing_count() {
        let mut assets = EmbeddedAssets::new();
        assert!(assets.is_empty());
        assert_eq!(assets.insert("icons/app.png", FIRST_ICON), None);
        assert_eq!(assets.len(), 1);
        assert_eq!(
            assets.insert("icons/app.png", REPLACEMENT_ICON),
            Some(FIRST_ICON)
        );
        assert_eq!(assets.len(), 1);
        assert_eq!(assets.get("icons/app.png"), Some(REPLACEMENT_ICON));
    }

    #[test]
    fn paths_are_logical_and_are_not_normalized() {
        let assets = EmbeddedAssets::new()
            .with_file("/icons//app.png", FIRST_ICON)
            .with_file("icons/app.png", OTHER_FILE);

        assert_eq!(assets.get("/icons//app.png"), Some(FIRST_ICON));
        assert_eq!(assets.get("icons/app.png"), Some(OTHER_FILE));
        assert_eq!(assets.get("/icons/app.png"), None);
    }
}
