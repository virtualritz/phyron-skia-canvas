use skia_safe::{AlphaType, Data, Image as SkImage, ImageInfo, images};

use crate::native::error::NativeError;
use crate::native::pixels::{PixelColorSpace, PixelFormat};

#[derive(Debug, Clone)]
pub struct NativeImage {
    pub(crate) inner: SkImage,
}

impl NativeImage {
    /// Decode an encoded image (PNG, JPEG, WebP, etc.) into a `NativeImage`.
    /// For raw decoded video frames (rsmpeg) or generated pixel buffers
    /// (Citra), prefer `from_pixels` -- it skips the encode/decode round
    /// trip.
    pub fn from_encoded(bytes: &[u8]) -> Result<Self, NativeError> {
        let data = Data::new_copy(bytes);
        let image = SkImage::from_encoded(data).ok_or_else(|| NativeError::DecodeImage {
            reason: "skia could not decode the encoded image bytes".to_string(),
        })?;
        Ok(Self { inner: image })
    }

    /// Build a `NativeImage` directly from a raw pixel buffer. The intended
    /// bridge for rsmpeg-decoded video frames and Citra-generated images:
    /// no PNG/JPEG/WebP encode round trip is required.
    ///
    /// The caller specifies pixel layout and color metadata explicitly.
    /// `pixel_format` covers the pixel layout and alpha mode (premul vs
    /// unpremul); `color_space` is a `PixelColorSpace` (the same enum used
    /// for surface readback), so callers must explicitly state whether
    /// pixels are gamma-coded sRGB / Display P3 / Rec.2020 or their linear
    /// counterparts. There is no implicit fallback to sRGB.
    ///
    /// Validation:
    ///
    /// - `width` and `height` must be non-zero.
    /// - `stride` must be at least `width * pixel_format.bytes_per_pixel()`.
    /// - `bytes.len()` must equal `stride * height` exactly.
    ///
    /// Pixel data is copied; the returned image owns its storage. F16 / F32
    /// formats preserve HDR values without clamping.
    pub fn from_pixels(
        bytes: &[u8],
        width: u32,
        height: u32,
        stride: usize,
        pixel_format: PixelFormat,
        color_space: PixelColorSpace,
    ) -> Result<Self, NativeError> {
        if width == 0 || height == 0 {
            return Err(NativeError::InvalidDimensions {
                width: width as f32,
                height: height as f32,
            });
        }
        let bpp = pixel_format.bytes_per_pixel();
        let min_stride = (width as usize) * bpp;
        if stride < min_stride {
            return Err(NativeError::InvalidStride {
                expected: min_stride,
                actual: stride,
            });
        }
        let expected_len = stride * (height as usize);
        if bytes.len() != expected_len {
            return Err(NativeError::InvalidByteLength {
                expected: expected_len,
                actual: bytes.len(),
            });
        }

        let color_type = pixel_format.to_skia_color_type()?;
        let alpha_type = pixel_format.to_skia_alpha_type();
        let sk_color_space = color_space.to_skia_color_space()?;
        let info = ImageInfo::new(
            (width as i32, height as i32),
            color_type,
            alpha_type,
            sk_color_space,
        );

        let data = Data::new_copy(bytes);
        let image = images::raster_from_data(&info, data, stride).ok_or_else(|| {
            NativeError::DecodeImage {
                reason: format!(
                    "skia could not build image from raw pixels ({pixel_format:?} {color_space:?})"
                ),
            }
        })?;
        Ok(Self { inner: image })
    }

    pub fn width(&self) -> u32 {
        self.inner.width().max(0) as u32
    }

    pub fn height(&self) -> u32 {
        self.inner.height().max(0) as u32
    }

    /// Internal alpha mode: `AlphaType::Premul`/`Unpremul`/`Opaque`.
    /// Skia surfaces composite at premultiplied alpha; raw inputs may be
    /// either premul or unpremul depending on the originating producer.
    pub fn is_premultiplied(&self) -> bool {
        matches!(
            self.inner.alpha_type(),
            AlphaType::Premul | AlphaType::Opaque
        )
    }
}
