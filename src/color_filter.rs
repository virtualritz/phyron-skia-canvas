#![allow(non_snake_case)]
use neon::prelude::*;
use skia_safe::{color_filters, ColorFilter as SkColorFilter};
use std::cell::RefCell;

use crate::utils::*;

pub type BoxedColorFilter = JsBox<RefCell<ColorFilter>>;
impl Finalize for ColorFilter {}

#[derive(Clone)]
pub struct ColorFilter {
    pub inner: SkColorFilter,
    deleted: bool,
}

impl ColorFilter {
    pub fn is_deleted(&self) -> bool {
        self.deleted
    }
}

/// Box a ColorFilter directly (for functions that always succeed)
macro_rules! box_color_filter {
    ($cx:expr, $filter:expr) => {{
        let cf = ColorFilter {
            inner: $filter,
            deleted: false,
        };
        Ok($cx.boxed(RefCell::new(cf)).upcast())
    }};
}

/// ColorFilter.MakeMatrix(matrix: ArrayLike<number>) - 20 elements
pub fn makeMatrix(mut cx: FunctionContext) -> JsResult<JsValue> {
    let matrix_vec = float_array_arg(&mut cx, 1, 20)?;

    // Validate no NaN/Infinity
    for (i, &val) in matrix_vec.iter().enumerate() {
        if !val.is_finite() {
            return cx.throw_type_error(format!(
                "Matrix element {} is not a finite number",
                i
            ));
        }
    }

    // Convert Vec to fixed-size array
    let matrix: [f32; 20] = matrix_vec
        .try_into()
        .expect("float_array_arg should have validated length");

    let filter = color_filters::matrix_row_major(&matrix, None);
    let cf = ColorFilter {
        inner: filter,
        deleted: false,
    };
    Ok(cx.boxed(RefCell::new(cf)).upcast())
}

/// ColorFilter.MakeSRGBToLinearGamma()
pub fn makeSRGBToLinearGamma(mut cx: FunctionContext) -> JsResult<JsValue> {
    box_color_filter!(cx, color_filters::srgb_to_linear_gamma())
}

/// ColorFilter.MakeLinearToSRGBGamma()
pub fn makeLinearToSRGBGamma(mut cx: FunctionContext) -> JsResult<JsValue> {
    box_color_filter!(cx, color_filters::linear_to_srgb_gamma())
}

pub fn repr(mut cx: FunctionContext) -> JsResult<JsString> {
    let this = cx.argument::<BoxedColorFilter>(0)?;
    let label = if this.borrow().deleted {
        "deleted"
    } else {
        "Matrix"
    };
    Ok(cx.string(label))
}

pub fn delete(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let this = cx.argument::<BoxedColorFilter>(0)?;
    this.borrow_mut().deleted = true;
    Ok(cx.undefined())
}

pub fn checkDeleted(cx: &mut FunctionContext, filter: &BoxedColorFilter) -> NeonResult<()> {
    if filter.borrow().deleted {
        cx.throw_error("ColorFilter has been deleted")
    } else {
        Ok(())
    }
}
