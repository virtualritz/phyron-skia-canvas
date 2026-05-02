//! # Native Rust API
//!
//! Stable Rust-only facade for `phyron-skia-canvas`. Rust library consumers
//! should use the types in this module; the Neon/JS modules at the crate root
//! are kept for Node addon compatibility and intentionally leak Skia and Neon
//! types in their public signatures.
//!
//! Public signatures in this module never expose `skia_safe` or `neon` types.

pub mod backend;
pub mod color;
pub mod error;
pub mod filter;
pub mod font;
pub mod geometry;
pub mod image;
pub mod paint;
pub mod path;
pub mod pixels;
pub mod recorder;
pub mod shader;
pub mod surface;
pub mod text;

pub use backend::NativeBackend;
pub use color::{LinearColorSpace, OutputColorSpace, RgbaLinear};
pub use error::NativeError;
pub use filter::{NativeColorFilter, NativeImageFilter};
pub use font::NativeFontManager;
pub use geometry::{NativeAffine, Point, Rect, Size};
pub use image::NativeImage;
pub use paint::{BlendMode, DashPattern, NativePaint, PaintStyle, StrokeCap};
pub use path::{FillRule, NativePath};
pub use pixels::{
    AlphaMode, ExportedPixels, PixelColorSpace, PixelDepth, PixelExportOptions, PixelFormat,
    RawFrame, RawFrameOptions, SamplingMode, SurfaceOptions,
};
pub use recorder::{NativeCanvas, NativeRecorder};
pub use shader::{GradientInterpolation, GradientStop, NativeShader};
pub use surface::NativeSurface;
pub use text::{TextAlign, TextBoxOptions, VerticalAlign};
