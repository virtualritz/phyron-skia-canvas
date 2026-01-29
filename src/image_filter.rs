#![allow(non_snake_case)]
use neon::prelude::*;
use skia_safe::{image_filters, Color4f, ImageFilter as SkImageFilter, TileMode};
use std::cell::RefCell;

use crate::color_filter::{checkDeleted as checkColorFilterDeleted, BoxedColorFilter};
use crate::utils::css_to_color;

pub type BoxedImageFilter = JsBox<RefCell<ImageFilter>>;
impl Finalize for ImageFilter {}

#[derive(Clone)]
pub struct ImageFilter {
    pub inner: SkImageFilter,
    deleted: bool,
}

impl ImageFilter {
    pub fn is_deleted(&self) -> bool {
        self.deleted
    }
}

/// Wrap an Option<SkImageFilter> result: Some -> boxed ImageFilter, None -> null
macro_rules! wrap_image_filter {
    ($cx:expr, $result:expr) => {
        match $result {
            Some(filter) => {
                let imgf = ImageFilter {
                    inner: filter,
                    deleted: false,
                };
                Ok($cx.boxed(RefCell::new(imgf)).upcast())
            }
            None => Ok($cx.null().upcast()),
        }
    };
}

/// Parse optional input ImageFilter from argument at index
macro_rules! opt_input_filter {
    ($cx:expr, $idx:expr) => {
        match $cx.argument_opt($idx) {
            Some(arg)
                if !arg.is_a::<JsNull, _>($cx) && !arg.is_a::<JsUndefined, _>($cx) =>
            {
                let prev = arg.downcast_or_throw::<BoxedImageFilter, _>($cx)?;
                checkDeleted($cx, &prev)?;
                Some(prev.borrow().inner.clone())
            }
            _ => None,
        }
    };
}

/// Parse color from argument (CSS string or [r,g,b,a] array) -> Color4f
macro_rules! parse_color4f {
    ($cx:expr, $idx:expr, $default:expr) => {
        match $cx.argument_opt($idx) {
            Some(arg) if arg.is_a::<JsString, _>($cx) => {
                let color_str = arg.downcast_or_throw::<JsString, _>($cx)?.value($cx);
                css_to_color(&color_str)
                    .map(|c| Color4f::from(c))
                    .unwrap_or($default)
            }
            Some(arg) if arg.is_a::<JsArray, _>($cx) => {
                // CanvasKit style: [r, g, b, a] as floats 0-1
                let arr = arg.downcast_or_throw::<JsArray, _>($cx)?;
                let r = arr.get::<JsNumber, _, _>($cx, 0)?.value($cx) as f32;
                let g = arr.get::<JsNumber, _, _>($cx, 1)?.value($cx) as f32;
                let b = arr.get::<JsNumber, _, _>($cx, 2)?.value($cx) as f32;
                let a = arr.get::<JsNumber, _, _>($cx, 3)?.value($cx) as f32;
                Color4f::new(r, g, b, a)
            }
            _ => $default,
        }
    };
}

/// Parse TileMode from string argument
fn parse_tile_mode(cx: &mut FunctionContext, idx: usize) -> TileMode {
    match cx.argument_opt(idx) {
        Some(arg) if !arg.is_a::<JsNull, _>(cx) && !arg.is_a::<JsUndefined, _>(cx) => {
            if let Ok(s) = arg.downcast::<JsString, _>(cx) {
                match s.value(cx).to_lowercase().as_str() {
                    "clamp" => TileMode::Clamp,
                    "repeat" => TileMode::Repeat,
                    "mirror" => TileMode::Mirror,
                    _ => TileMode::Decal,
                }
            } else {
                TileMode::Decal
            }
        }
        _ => TileMode::Decal,
    }
}

/// ImageFilter.MakeColorFilter(colorFilter, input?)
pub fn makeColorFilter(mut cx: FunctionContext) -> JsResult<JsValue> {
    let cf = cx.argument::<BoxedColorFilter>(1)?;
    checkColorFilterDeleted(&mut cx, &cf)?;
    let input = opt_input_filter!(&mut cx, 2);
    wrap_image_filter!(cx, image_filters::color_filter(cf.borrow().inner.clone(), input, None))
}

/// ImageFilter.MakeCompose(outer, inner)
pub fn makeCompose(mut cx: FunctionContext) -> JsResult<JsValue> {
    let outer = cx.argument::<BoxedImageFilter>(1)?;
    let inner = cx.argument::<BoxedImageFilter>(2)?;
    checkDeleted(&mut cx, &outer)?;
    checkDeleted(&mut cx, &inner)?;
    wrap_image_filter!(
        cx,
        image_filters::compose(outer.borrow().inner.clone(), inner.borrow().inner.clone())
    )
}

/// ImageFilter.MakeBlur(sigmaX, sigmaY, tileMode?, input?)
pub fn makeBlur(mut cx: FunctionContext) -> JsResult<JsValue> {
    let sigma_x = cx.argument::<JsNumber>(1)?.value(&mut cx) as f32;
    let sigma_y = cx.argument::<JsNumber>(2)?.value(&mut cx) as f32;
    let tile_mode = parse_tile_mode(&mut cx, 3);
    let input = opt_input_filter!(&mut cx, 4);
    wrap_image_filter!(cx, image_filters::blur((sigma_x, sigma_y), tile_mode, input, None))
}

/// ImageFilter.MakeDropShadow(dx, dy, sigmaX, sigmaY, color, input?)
pub fn makeDropShadow(mut cx: FunctionContext) -> JsResult<JsValue> {
    let dx = cx.argument::<JsNumber>(1)?.value(&mut cx) as f32;
    let dy = cx.argument::<JsNumber>(2)?.value(&mut cx) as f32;
    let sigma_x = cx.argument::<JsNumber>(3)?.value(&mut cx) as f32;
    let sigma_y = cx.argument::<JsNumber>(4)?.value(&mut cx) as f32;
    let color = parse_color4f!(&mut cx, 5, Color4f::new(0.0, 0.0, 0.0, 1.0));
    let input = opt_input_filter!(&mut cx, 6);
    // Args: offset, sigma, color, color_space, input, crop_rect
    wrap_image_filter!(
        cx,
        image_filters::drop_shadow((dx, dy), (sigma_x, sigma_y), color, None, input, None)
    )
}

/// ImageFilter.MakeDropShadowOnly(dx, dy, sigmaX, sigmaY, color, input?)
pub fn makeDropShadowOnly(mut cx: FunctionContext) -> JsResult<JsValue> {
    let dx = cx.argument::<JsNumber>(1)?.value(&mut cx) as f32;
    let dy = cx.argument::<JsNumber>(2)?.value(&mut cx) as f32;
    let sigma_x = cx.argument::<JsNumber>(3)?.value(&mut cx) as f32;
    let sigma_y = cx.argument::<JsNumber>(4)?.value(&mut cx) as f32;
    let color = parse_color4f!(&mut cx, 5, Color4f::new(0.0, 0.0, 0.0, 1.0));
    let input = opt_input_filter!(&mut cx, 6);
    // Args: offset, sigma, color, color_space, input, crop_rect
    wrap_image_filter!(
        cx,
        image_filters::drop_shadow_only((dx, dy), (sigma_x, sigma_y), color, None, input, None)
    )
}

pub fn repr(mut cx: FunctionContext) -> JsResult<JsString> {
    let this = cx.argument::<BoxedImageFilter>(0)?;
    let b = this.borrow();
    let label = if b.deleted { "deleted" } else { "active" };
    Ok(cx.string(label))
}

pub fn delete(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let this = cx.argument::<BoxedImageFilter>(0)?;
    this.borrow_mut().deleted = true;
    Ok(cx.undefined())
}

fn checkDeleted(cx: &mut FunctionContext, filter: &BoxedImageFilter) -> NeonResult<()> {
    if filter.borrow().deleted {
        cx.throw_error("ImageFilter has been deleted")
    } else {
        Ok(())
    }
}
