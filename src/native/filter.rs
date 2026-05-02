use skia_safe::{
    ColorFilter as SkColorFilter, ImageFilter as SkImageFilter, color_filters, image_filters,
    luma_color_filter,
};

use crate::native::color::RgbaLinear;
use crate::native::error::NativeError;

/// Image-domain filter (blur, drop shadow, color matrix wrapped as image
/// filter, compose). Composed by `NativePaint` and applied to draws.
#[derive(Clone)]
pub struct NativeImageFilter {
    pub(crate) inner: SkImageFilter,
}

impl std::fmt::Debug for NativeImageFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeImageFilter").finish_non_exhaustive()
    }
}

/// Color-domain filter (luma, gamma transfers, color matrix, compose).
/// Composed by `NativePaint` or wrapped as an image filter via
/// `NativeImageFilter::from_color_filter`.
#[derive(Clone)]
pub struct NativeColorFilter {
    pub(crate) inner: SkColorFilter,
}

impl std::fmt::Debug for NativeColorFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeColorFilter").finish_non_exhaustive()
    }
}

impl NativeImageFilter {
    /// Gaussian blur with separable sigmas. `input` is the upstream filter
    /// to blur, or `None` to blur the source draw.
    pub fn blur(
        sigma_x: f32,
        sigma_y: f32,
        input: Option<NativeImageFilter>,
    ) -> Result<Self, NativeError> {
        let inner = input.map(|f| f.inner);
        image_filters::blur((sigma_x, sigma_y), None, inner, None)
            .map(|f| NativeImageFilter { inner: f })
            .ok_or_else(|| NativeError::FilterCreate {
                reason: format!("blur({sigma_x}, {sigma_y}) failed"),
            })
    }

    /// Drop shadow at `(dx, dy)` with separable blur sigmas. `color` is the
    /// shadow color (premultiplied linear; treated as already in the
    /// destination's working color space).
    pub fn drop_shadow(
        dx: f32,
        dy: f32,
        sigma_x: f32,
        sigma_y: f32,
        color: RgbaLinear,
        input: Option<NativeImageFilter>,
    ) -> Result<Self, NativeError> {
        let unpremul = if color.a > 0.0 {
            skia_safe::Color4f {
                r: color.r / color.a,
                g: color.g / color.a,
                b: color.b / color.a,
                a: color.a,
            }
        } else {
            skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.0)
        };
        let inner = input.map(|f| f.inner);
        // `None` color space means Skia treats the color as already in the
        // destination's working space (no primaries conversion).
        image_filters::drop_shadow(
            skia_safe::Vector::new(dx, dy),
            (sigma_x, sigma_y),
            unpremul,
            None,
            inner,
            None,
        )
        .map(|f| NativeImageFilter { inner: f })
        .ok_or_else(|| NativeError::FilterCreate {
            reason: format!("drop_shadow({dx}, {dy}) failed"),
        })
    }

    /// 4x5 color matrix in row-major order:
    ///
    /// ```text
    /// | r_r  r_g  r_b  r_a  r_offset |
    /// | g_r  g_g  g_b  g_a  g_offset |
    /// | b_r  b_g  b_b  b_a  b_offset |
    /// | a_r  a_g  a_b  a_a  a_offset |
    /// ```
    ///
    /// Output channel `c` = `c_r * r_in + c_g * g_in + c_b * b_in + c_a *
    /// a_in + c_offset`. Offsets are in the 0..1 range for u8 channels.
    pub fn color_matrix(
        matrix: [f32; 20],
        input: Option<NativeImageFilter>,
    ) -> Result<Self, NativeError> {
        let cf = color_filters::matrix_row_major(&matrix, None);
        let inner = input.map(|f| f.inner);
        image_filters::color_filter(cf, inner, None)
            .map(|f| NativeImageFilter { inner: f })
            .ok_or_else(|| NativeError::FilterCreate {
                reason: "color_matrix failed".to_string(),
            })
    }

    /// Wrap a `NativeColorFilter` as an image filter, optionally chained
    /// onto `input`.
    pub fn from_color_filter(
        color_filter: NativeColorFilter,
        input: Option<NativeImageFilter>,
    ) -> Result<Self, NativeError> {
        let inner = input.map(|f| f.inner);
        image_filters::color_filter(color_filter.inner, inner, None)
            .map(|f| NativeImageFilter { inner: f })
            .ok_or_else(|| NativeError::FilterCreate {
                reason: "from_color_filter failed".to_string(),
            })
    }

    /// Compose two image filters: `outer(inner(source))`.
    pub fn compose(
        outer: NativeImageFilter,
        inner: NativeImageFilter,
    ) -> Result<Self, NativeError> {
        image_filters::compose(outer.inner, inner.inner)
            .map(|f| NativeImageFilter { inner: f })
            .ok_or_else(|| NativeError::FilterCreate {
                reason: "image filter compose failed".to_string(),
            })
    }
}

impl NativeColorFilter {
    /// Skia's luma color filter: output alpha = perceived luminance of the
    /// input RGB, output RGB = 0. Useful as the `inner` filter in a
    /// `destination-in` mask path: luminance becomes the alpha mask.
    pub fn luma() -> Self {
        Self {
            inner: luma_color_filter::new(),
        }
    }

    /// Apply the linear-to-sRGB gamma transfer to the input color before
    /// downstream draws see it. Used to bridge linear-light pipelines to
    /// gamma-coded readers.
    pub fn linear_to_srgb_gamma() -> Self {
        Self {
            inner: color_filters::linear_to_srgb_gamma(),
        }
    }

    /// Inverse of `linear_to_srgb_gamma`.
    pub fn srgb_to_linear_gamma() -> Self {
        Self {
            inner: color_filters::srgb_to_linear_gamma(),
        }
    }

    /// Compose two color filters: `outer(inner(input))`.
    pub fn compose(
        outer: NativeColorFilter,
        inner: NativeColorFilter,
    ) -> Result<Self, NativeError> {
        color_filters::compose(outer.inner, inner.inner)
            .map(|f| NativeColorFilter { inner: f })
            .ok_or_else(|| NativeError::FilterCreate {
                reason: "color filter compose failed".to_string(),
            })
    }
}
