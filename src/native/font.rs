use std::path::Path;

use parking_lot::Mutex;
use skia_safe::{FontMgr, textlayout::TypefaceFontProvider};

use crate::native::error::NativeError;

/// Owned font registry for the Rust facade. Holds typefaces registered
/// from disk or from in-memory bytes and exposes them for paragraph
/// layout (Chunk 7B). Internal state lives behind `parking_lot::Mutex`
/// so the same manager can be shared across threads without exposing
/// `RefCell` to consumers.
pub struct NativeFontManager {
    inner: Mutex<NativeFontManagerInner>,
}

struct NativeFontManagerInner {
    /// Skia paragraph-side provider that maps registered family names
    /// to typefaces. Used internally by paragraph builders in Chunk 7B.
    provider: TypefaceFontProvider,
    /// System `FontMgr` used to parse byte streams into `Typeface`s.
    font_mgr: FontMgr,
    /// Registered family names in registration order.
    families: Vec<String>,
}

impl Default for NativeFontManager {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeFontManager {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(NativeFontManagerInner {
                provider: TypefaceFontProvider::new(),
                font_mgr: FontMgr::new(),
                families: Vec::new(),
            }),
        }
    }

    /// Register a typeface loaded from `bytes` (TTF/OTF/WOFF/WOFF2,
    /// depending on Skia's available decoders) under the given family
    /// alias. Multiple typefaces can share a family alias; layout will
    /// pick one matching weight/slant.
    pub fn register_font_from_data(&self, family: &str, bytes: &[u8]) -> Result<(), NativeError> {
        let mut inner = self.inner.lock();
        let typeface =
            inner
                .font_mgr
                .new_from_data(bytes, None)
                .ok_or_else(|| NativeError::FontRegister {
                    reason: format!("could not parse typeface for family {family:?}"),
                })?;
        inner.provider.register_typeface(typeface, Some(family));
        if !inner.families.iter().any(|f| f == family) {
            inner.families.push(family.to_string());
        }
        Ok(())
    }

    /// Register a typeface loaded from a file under `path` under the
    /// given family alias. To register multiple files (e.g. one per
    /// weight) for a single family, call this method multiple times.
    pub fn register_font_from_path(
        &self,
        family: &str,
        path: impl AsRef<Path>,
    ) -> Result<(), NativeError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| NativeError::FontRegister {
            reason: format!("could not read font file {}: {e}", path.display()),
        })?;
        self.register_font_from_data(family, &bytes)
    }

    /// Whether a family alias has at least one registered typeface.
    pub fn has_font(&self, family: &str) -> bool {
        let inner = self.inner.lock();
        inner.families.iter().any(|f| f == family)
    }

    /// All registered family aliases in registration order. Duplicates
    /// are deduplicated; calling `register_font_from_data` repeatedly
    /// for the same family does not duplicate the alias.
    pub fn families(&self) -> Vec<String> {
        let inner = self.inner.lock();
        inner.families.clone()
    }
}
