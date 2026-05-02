//! Renderer contract tests for the `phyron_skia_canvas::native` facade.
//!
//! Tests in this file exercise the surface, pixel IO, and paint/blend
//! subsets (Chunks 2 and 3A of the Studio renderer gap closure plan).
//! Path/filter/shader/font/paragraph tests land alongside their
//! implementation chunks; this file intentionally avoids importing those
//! types so the branch stays compiling and green.
//!
//! Public-leak audit (run from repo root):
//!   rg -n "pub .*skia_safe|pub .*FunctionContext|pub .*JsBox|pub .*Handle<|pub .*RefCell" src/native

use anyhow::Result;
use phyron_skia_canvas::native::{
    BlendMode, FillRule, GradientInterpolation, GradientStop, LinearColorSpace, NativeAffine,
    NativeBackend, NativeColorFilter, NativeError, NativeImageFilter, NativePaint, NativePath,
    NativeShader, PaintStyle, PixelColorSpace, PixelDepth, PixelExportOptions, Point, Rect,
    RgbaLinear, SamplingMode, StrokeCap, SurfaceOptions,
};

const ALPHA_HALF_U8: u8 = 128;

fn red_premul(alpha: f32) -> RgbaLinear {
    // Premultiplied linear red at the given alpha (alpha is straight 0..1).
    RgbaLinear::new_premultiplied(alpha, 0.0, 0.0, alpha)
}

#[test]
fn surface_create_clear_draw_snapshot_compose_readback() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(8, 8, SurfaceOptions::default())?;
    assert_eq!(surface.width(), 8);
    assert_eq!(surface.height(), 8);

    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_rect(
            Rect::from_xywh(2.0, 2.0, 4.0, 4.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
    });

    // Snapshot, then composite the snapshot onto a second surface via
    // `draw_image_rect` (default source-over). BlendMode::SourceOver lands
    // with Chunk 3.
    let snapshot = surface.snapshot();
    let mut composed = backend.create_surface(8, 8, SurfaceOptions::default())?;
    composed.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_image_rect(&snapshot, Rect::from_xywh(0.0, 0.0, 8.0, 8.0), 1.0);
    });

    let frame = composed.read_pixels()?;
    assert_eq!(frame.width(), 8);
    assert_eq!(frame.height(), 8);
    assert_eq!(frame.stride(), 8 * 4);
    assert_eq!(frame.depth(), PixelDepth::Uint8);
    assert_eq!(frame.color_space(), PixelColorSpace::Srgb);
    assert!(!frame.premultiplied());
    assert!(frame.pixels().iter().any(|c| *c != 0));
    Ok(())
}

#[test]
fn create_offscreen_inherits_color_space_and_composes() -> Result<()> {
    let backend = NativeBackend::new();
    let mut main = backend.create_surface(
        8,
        8,
        SurfaceOptions {
            color_space: LinearColorSpace::DisplayP3,
            ..SurfaceOptions::default()
        },
    )?;
    let mut offscreen = main.create_offscreen(4, 4)?;
    assert_eq!(offscreen.width(), 4);
    assert_eq!(offscreen.height(), 4);
    assert_eq!(offscreen.color_space(), LinearColorSpace::DisplayP3);

    offscreen.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::opaque(0.0, 1.0, 0.0));
    });
    let off_snapshot = offscreen.snapshot();
    main.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_image_rect(&off_snapshot, Rect::from_xywh(0.0, 0.0, 4.0, 4.0), 1.0);
    });
    let exported = main.read_pixels()?;
    assert!(exported.pixels().iter().any(|c| *c > 32));
    Ok(())
}

#[test]
fn read_pixels_as_supports_required_color_spaces() -> Result<()> {
    let backend = NativeBackend::new();
    for color_space in [
        PixelColorSpace::Srgb,
        PixelColorSpace::SrgbLinear,
        PixelColorSpace::DisplayP3,
        PixelColorSpace::DisplayP3Linear,
        PixelColorSpace::Rec2020,
        PixelColorSpace::Rec2020Linear,
    ] {
        let mut surface = backend.create_surface(2, 2, SurfaceOptions::default())?;
        surface.with_canvas(|canvas| canvas.clear(RgbaLinear::opaque(0.5, 0.5, 0.5)));
        let exported = surface.read_pixels_as(PixelExportOptions {
            color_space,
            depth: PixelDepth::Uint8,
            premultiplied: false,
        })?;
        assert_eq!(exported.color_space(), color_space);
        assert_eq!(exported.width(), 2);
        assert_eq!(exported.height(), 2);
        assert_eq!(exported.stride(), 2 * 4);
    }
    Ok(())
}

#[test]
fn read_write_pixels_linear_round_trips_dimensions() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(4, 4, SurfaceOptions::default())?;
    surface.with_canvas(|canvas| canvas.clear(RgbaLinear::opaque(0.25, 0.5, 0.75)));
    let exported = surface.read_pixels_linear()?;
    assert_eq!(exported.depth(), PixelDepth::F32);
    assert_eq!(exported.color_space(), PixelColorSpace::SrgbLinear);
    assert!(exported.premultiplied());
    assert_eq!(exported.stride(), 4 * 16);
    assert_eq!(exported.pixels().len(), 4 * 16 * 4);

    // Round-trip back into a fresh surface and re-read; dimensions and depth
    // must be preserved.
    let mut destination = backend.create_surface(4, 4, SurfaceOptions::default())?;
    destination.write_pixels_linear(exported.pixels())?;
    let round_tripped = destination.read_pixels_linear()?;
    assert_eq!(round_tripped.width(), 4);
    assert_eq!(round_tripped.height(), 4);
    assert_eq!(round_tripped.depth(), PixelDepth::F32);
    assert_eq!(round_tripped.pixels().len(), exported.pixels().len());
    Ok(())
}

#[test]
fn linear_working_spaces_accept_hdr_values_above_one() -> Result<()> {
    let backend = NativeBackend::new();
    for color_space in [
        LinearColorSpace::Srgb,
        LinearColorSpace::DisplayP3,
        LinearColorSpace::Rec2020,
    ] {
        let mut surface = backend.create_surface(
            2,
            2,
            SurfaceOptions {
                color_space,
                ..SurfaceOptions::default()
            },
        )?;
        surface.with_canvas(|canvas| canvas.clear(RgbaLinear::opaque(2.0, 0.0, 0.0)));
        let exported = surface.read_pixels_linear()?;
        assert_eq!(exported.depth(), PixelDepth::F32);
        // First pixel's red channel as a little-endian f32.
        let bytes = exported.pixels();
        let r = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        assert!(r > 1.5, "expected HDR red >1.5 in {color_space:?}, got {r}");
    }
    Ok(())
}

#[test]
fn premultiplied_alpha_preserved_across_read_modes() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(4, 4, SurfaceOptions::default())?;
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
            &NativePaint::fill(red_premul(0.5)),
        );
    });

    // Premultiplied Uint8 in the surface's own linear space: r ≈ 0.5 * 255.
    let premul = surface.read_pixels_as(PixelExportOptions {
        color_space: PixelColorSpace::SrgbLinear,
        depth: PixelDepth::Uint8,
        premultiplied: true,
    })?;
    let p_r = premul.pixels()[0];
    let p_a = premul.pixels()[3];
    assert!(
        p_r.abs_diff(ALPHA_HALF_U8) <= 4,
        "premul red ≈ 128, got {p_r}"
    );
    assert!(
        p_a.abs_diff(ALPHA_HALF_U8) <= 4,
        "premul alpha ≈ 128, got {p_a}"
    );

    // Unpremultiplied Uint8 in the same color space: r = 1.0 * 255, alpha = 128.
    let unpremul = surface.read_pixels_as(PixelExportOptions {
        color_space: PixelColorSpace::SrgbLinear,
        depth: PixelDepth::Uint8,
        premultiplied: false,
    })?;
    let u_r = unpremul.pixels()[0];
    let u_a = unpremul.pixels()[3];
    assert!(u_r > 240, "unpremul red ≈ 255, got {u_r}");
    assert!(
        u_a.abs_diff(ALPHA_HALF_U8) <= 4,
        "unpremul alpha ≈ 128, got {u_a}"
    );
    Ok(())
}

/// Compile-time leak audit: importing only `phyron_skia_canvas::native::*`
/// must be sufficient for surface + pixel IO contract use. This test
/// references the new public types at run time so any accidental private
/// scoping breaks the build.
#[test]
fn public_types_are_reachable_from_native_namespace_only() -> Result<()> {
    let backend = NativeBackend::new();
    let surface = backend.create_surface(1, 1, SurfaceOptions::default())?;
    let _ = (
        surface.color_space(),
        PixelColorSpace::Srgb,
        PixelDepth::F16,
        PixelExportOptions::default(),
    );
    Ok(())
}

// --- Chunk 3A: paint and blend ---------------------------------------------

#[test]
fn native_paint_default_matches_canvas_defaults() {
    let p = NativePaint::default();
    assert_eq!(p.style, PaintStyle::Fill);
    assert_eq!(p.stroke_width, 1.0);
    assert_eq!(p.stroke_cap, StrokeCap::Butt);
    assert!(p.dash.is_none());
    assert!(p.anti_alias);
    assert_eq!(p.alpha, 1.0);
    assert_eq!(p.blend_mode, BlendMode::SourceOver);
    assert!(p.shader.is_none());
    assert!(p.image_filter.is_none());
    assert!(p.color_filter.is_none());
}

#[test]
fn native_paint_constructors_set_style() {
    let f = NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0));
    assert_eq!(f.style, PaintStyle::Fill);
    let s = NativePaint::stroke(RgbaLinear::opaque(0.0, 1.0, 0.0), 3.5);
    assert_eq!(s.style, PaintStyle::Stroke);
    assert_eq!(s.stroke_width, 3.5);
}

/// `paint.alpha` modulates the final color: a 1.0 paint produces ~255,
/// while alpha 0.5 produces ~half pixel coverage on the same opaque red.
#[test]
fn native_paint_alpha_modulates_output() -> Result<()> {
    let backend = NativeBackend::new();
    let mut full = backend.create_surface(2, 2, SurfaceOptions::default())?;
    full.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 2.0, 2.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
    });
    let full_px = full.read_pixels()?;
    assert!(full_px.pixels()[0] > 240, "full alpha red ≈ 255");
    assert_eq!(full_px.pixels()[3], 255);

    let mut half = backend.create_surface(2, 2, SurfaceOptions::default())?;
    half.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        let mut paint = NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0));
        paint.set_alpha(0.5);
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), &paint);
    });
    let half_px = half.read_pixels()?;
    let alpha_byte = half_px.pixels()[3];
    assert!(
        (110..=145).contains(&alpha_byte),
        "alpha 0.5 ≈ 128, got {alpha_byte}"
    );
    Ok(())
}

/// Different blend modes produce different output for the same overlap.
/// Asserts SourceOver, Multiply, and PlusLighter all diverge.
#[test]
fn blend_modes_produce_distinct_outputs() -> Result<()> {
    let backend = NativeBackend::new();
    let render_with = |mode: BlendMode| -> Result<[u8; 4]> {
        let mut surface = backend.create_surface(2, 2, SurfaceOptions::default())?;
        surface.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::opaque(0.5, 0.5, 0.5));
            let mut paint = NativePaint::fill(RgbaLinear::opaque(0.5, 0.0, 0.5));
            paint.set_blend_mode(mode);
            canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), &paint);
        });
        let px = surface.read_pixels()?;
        Ok([
            px.pixels()[0],
            px.pixels()[1],
            px.pixels()[2],
            px.pixels()[3],
        ])
    };
    let src_over = render_with(BlendMode::SourceOver)?;
    let multiply = render_with(BlendMode::Multiply)?;
    let plus_lighter = render_with(BlendMode::PlusLighter)?;
    assert_ne!(src_over, multiply, "SourceOver and Multiply must differ");
    assert_ne!(
        src_over, plus_lighter,
        "SourceOver and PlusLighter must differ"
    );
    assert_ne!(
        multiply, plus_lighter,
        "Multiply and PlusLighter must differ"
    );
    Ok(())
}

/// Exhaustive blend-mode plumbing: every Canvas blend mode round-trips
/// through to a non-panicking draw and a successful pixel readback. This
/// catches typos in the `to_skia` mapping.
#[test]
fn every_blend_mode_renders_without_error() -> Result<()> {
    let backend = NativeBackend::new();
    let modes = [
        BlendMode::SourceOver,
        BlendMode::SourceIn,
        BlendMode::SourceOut,
        BlendMode::SourceAtop,
        BlendMode::DestinationOver,
        BlendMode::DestinationIn,
        BlendMode::DestinationOut,
        BlendMode::DestinationAtop,
        BlendMode::Copy,
        BlendMode::Xor,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::PlusLighter,
    ];
    for mode in modes {
        let mut surface = backend.create_surface(2, 2, SurfaceOptions::default())?;
        surface.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::opaque(0.4, 0.4, 0.4));
            let mut paint = NativePaint::fill(RgbaLinear::opaque(0.6, 0.2, 0.8));
            paint.set_blend_mode(mode);
            canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), &paint);
        });
        let _ = surface.read_pixels()?;
    }
    Ok(())
}

/// State-level checks for stroke cap and dash. Visual verification waits
/// for Chunk 3C (`draw_line` / `draw_path`); rectangles use joins not
/// caps, so stroke caps are not visually exercised on `draw_rect`.
#[test]
fn stroke_cap_state_round_trips_through_paint() {
    let mut paint = NativePaint::stroke(RgbaLinear::opaque(1.0, 1.0, 1.0), 4.0);
    assert_eq!(paint.stroke_cap, StrokeCap::Butt);
    paint.set_stroke_cap(StrokeCap::Round);
    assert_eq!(paint.stroke_cap, StrokeCap::Round);
    paint.set_stroke_cap(StrokeCap::Square);
    assert_eq!(paint.stroke_cap, StrokeCap::Square);
}

#[test]
fn dash_pattern_state_round_trips_through_paint() -> Result<()> {
    let mut paint = NativePaint::stroke(RgbaLinear::opaque(1.0, 1.0, 1.0), 2.0);
    assert!(paint.dash.is_none());
    paint.set_dash(vec![4.0, 4.0], 0.0);
    let dash = paint
        .dash
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("dash should be set after set_dash"))?;
    assert_eq!(dash.intervals, vec![4.0, 4.0]);
    assert_eq!(dash.phase, 0.0);
    let _ = dash; // release immutable borrow before clear_dash takes &mut.
    paint.clear_dash();
    assert!(paint.dash.is_none());
    Ok(())
}

// --- Chunk 3B: canvas state + layer basics ---------------------------------

/// `clip_rect` masks draws to the rectangle. Pixels outside the clip rect
/// must remain at the cleared (transparent) color.
#[test]
fn clip_rect_masks_drawing() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(8, 8, SurfaceOptions::default())?;
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.save();
        canvas.clip_rect(Rect::from_xywh(2.0, 2.0, 4.0, 4.0));
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 8.0, 8.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
        canvas.restore();
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    let alpha_at = |x: usize, y: usize| px.pixels()[y * stride + x * 4 + 3];
    assert_eq!(alpha_at(0, 0), 0, "top-left outside clip stays transparent");
    assert_eq!(
        alpha_at(7, 7),
        0,
        "bottom-right outside clip stays transparent"
    );
    assert!(alpha_at(3, 3) > 240, "inside clip stays red opaque");
    Ok(())
}

/// `clip_rrect` rounds the corners. The four extreme corners of the clip
/// rect must be transparent while the center remains opaque.
#[test]
fn clip_rrect_rounds_corners() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(16, 16, SurfaceOptions::default())?;
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.save();
        canvas.clip_rrect(Rect::from_xywh(0.0, 0.0, 16.0, 16.0), 8.0);
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 16.0, 16.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 1.0, 1.0)),
        );
        canvas.restore();
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    let alpha_at = |x: usize, y: usize| px.pixels()[y * stride + x * 4 + 3];
    assert_eq!(alpha_at(0, 0), 0, "top-left rounded corner is transparent");
    assert_eq!(
        alpha_at(15, 0),
        0,
        "top-right rounded corner is transparent"
    );
    assert_eq!(
        alpha_at(0, 15),
        0,
        "bottom-left rounded corner is transparent"
    );
    assert_eq!(
        alpha_at(15, 15),
        0,
        "bottom-right rounded corner is transparent"
    );
    assert!(alpha_at(8, 8) > 240, "center is fully opaque");
    Ok(())
}

/// `concat_transform` with a translation moves subsequent draws. Drawing a
/// 4x4 rect at (0,0) with a +6 horizontal translation must hit pixels in
/// the (6..10, 0..4) region instead of the origin.
#[test]
fn concat_transform_translates_subsequent_draws() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(16, 8, SurfaceOptions::default())?;
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.save();
        canvas.concat_transform(NativeAffine::translation(6.0, 0.0));
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
        canvas.restore();
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    let alpha_at = |x: usize, y: usize| px.pixels()[y * stride + x * 4 + 3];
    assert_eq!(alpha_at(0, 0), 0, "origin stays empty after translation");
    assert!(
        alpha_at(7, 1) > 240,
        "translated rect occupies (6..10, 0..4)"
    );
    Ok(())
}

/// `concat_transform` with a scale stretches subsequent draws. A 2x2 rect
/// scaled 3x covers a 6x6 region.
#[test]
fn concat_transform_scales_subsequent_draws() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(8, 8, SurfaceOptions::default())?;
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.save();
        canvas.concat_transform(NativeAffine::scale(3.0, 3.0));
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 2.0, 2.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
        canvas.restore();
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    let alpha_at = |x: usize, y: usize| px.pixels()[y * stride + x * 4 + 3];
    assert!(alpha_at(0, 0) > 240, "scaled rect covers origin");
    assert!(alpha_at(5, 5) > 240, "scaled rect covers (5,5)");
    assert_eq!(alpha_at(7, 7), 0, "outside the 6x6 region stays empty");
    Ok(())
}

/// `canvas.scale(sx, sy)` is a convenience equivalent to
/// `concat_transform(NativeAffine::scale(sx, sy))`.
#[test]
fn scale_method_matches_concat_scale_transform() -> Result<()> {
    let backend = NativeBackend::new();
    let render = |use_scale_helper: bool| -> Result<Vec<u8>> {
        let mut surface = backend.create_surface(8, 8, SurfaceOptions::default())?;
        surface.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
            canvas.save();
            if use_scale_helper {
                canvas.scale(2.0, 2.0);
            } else {
                canvas.concat_transform(NativeAffine::scale(2.0, 2.0));
            }
            canvas.draw_rect(
                Rect::from_xywh(0.0, 0.0, 3.0, 3.0),
                &NativePaint::fill(RgbaLinear::opaque(0.0, 1.0, 0.0)),
            );
            canvas.restore();
        });
        Ok(surface.read_pixels()?.into_pixels())
    };
    let helper = render(true)?;
    let direct = render(false)?;
    assert_eq!(helper, direct);
    Ok(())
}

/// Layer opacity isolation: drawing two opaque rects inside a layer with
/// `paint.alpha = 0.5` produces a different result than drawing each rect
/// directly with alpha 0.5. The layer composes the two rects internally
/// (last-wins for src-over) and only halves the final layer; the direct
/// approach blends each rect at 0.5 onto the destination, leaving residual
/// color from earlier rects.
#[test]
fn save_layer_opacity_isolates_inner_compositing() -> Result<()> {
    let backend = NativeBackend::new();

    let mut layered = backend.create_surface(4, 4, SurfaceOptions::default())?;
    layered.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::opaque(0.0, 0.0, 0.0));
        let mut layer_paint = NativePaint::default();
        layer_paint.set_alpha(0.5);
        canvas.save_layer(Some(&layer_paint));
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
            &NativePaint::fill(RgbaLinear::opaque(0.0, 1.0, 0.0)),
        );
        canvas.restore();
    });

    let mut direct = backend.create_surface(4, 4, SurfaceOptions::default())?;
    direct.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::opaque(0.0, 0.0, 0.0));
        let mut red = NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0));
        red.set_alpha(0.5);
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), &red);
        let mut green = NativePaint::fill(RgbaLinear::opaque(0.0, 1.0, 0.0));
        green.set_alpha(0.5);
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), &green);
    });

    let l = layered.read_pixels()?;
    let d = direct.read_pixels()?;
    let lr = l.pixels()[0];
    let dr = d.pixels()[0];
    assert!(
        lr < dr.saturating_sub(16),
        "layer should hide red residual: layered_r={lr} direct_r={dr}"
    );
    Ok(())
}

/// Layer blend mode applies on layer composite, not per-draw. Drawing a
/// red rect inside a `PlusLighter` layer onto a non-trivial background
/// gives a different result than drawing the red rect directly with
/// `PlusLighter`.
#[test]
fn save_layer_blend_mode_applies_to_layer_composite() -> Result<()> {
    let backend = NativeBackend::new();

    let mut layered = backend.create_surface(4, 4, SurfaceOptions::default())?;
    layered.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::opaque(0.4, 0.4, 0.4));
        let mut layer_paint = NativePaint::default();
        layer_paint.set_blend_mode(BlendMode::PlusLighter);
        canvas.save_layer(Some(&layer_paint));
        // Two draws inside the layer: only the layer composite uses
        // PlusLighter, the inner draws use src-over by default.
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
            &NativePaint::fill(RgbaLinear::opaque(0.5, 0.0, 0.0)),
        );
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 4.0, 4.0),
            &NativePaint::fill(RgbaLinear::opaque(0.0, 0.5, 0.0)),
        );
        canvas.restore();
    });

    let mut direct = backend.create_surface(4, 4, SurfaceOptions::default())?;
    direct.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::opaque(0.4, 0.4, 0.4));
        let mut a = NativePaint::fill(RgbaLinear::opaque(0.5, 0.0, 0.0));
        a.set_blend_mode(BlendMode::PlusLighter);
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), &a);
        let mut b = NativePaint::fill(RgbaLinear::opaque(0.0, 0.5, 0.0));
        b.set_blend_mode(BlendMode::PlusLighter);
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), &b);
    });

    let l = layered.read_pixels()?;
    let d = direct.read_pixels()?;
    assert_ne!(
        &l.pixels()[..4],
        &d.pixels()[..4],
        "layered PlusLighter must differ from sequential PlusLighter"
    );
    Ok(())
}

/// `draw_surface` composites a source surface's pixels onto this canvas at
/// `(x, y)`. A 4x4 red source drawn at (2, 2) on an 8x8 destination must
/// fill exactly the 4x4 region at that offset.
#[test]
fn draw_surface_composites_at_offset() -> Result<()> {
    let backend = NativeBackend::new();
    let mut source = backend.create_surface(4, 4, SurfaceOptions::default())?;
    source.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::opaque(1.0, 0.0, 0.0));
    });

    let mut dest = backend.create_surface(8, 8, SurfaceOptions::default())?;
    dest.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_surface(&mut source, 2.0, 2.0, None);
    });
    let px = dest.read_pixels()?;
    let stride = px.stride();
    let alpha_at = |x: usize, y: usize| px.pixels()[y * stride + x * 4 + 3];
    let red_at = |x: usize, y: usize| px.pixels()[y * stride + x * 4];
    assert_eq!(alpha_at(0, 0), 0, "outside source bounds stays transparent");
    assert!(red_at(3, 3) > 240, "source red opaque at offset");
    assert!(red_at(5, 5) > 240, "source red opaque at offset");
    assert_eq!(
        alpha_at(6, 6),
        0,
        "right of source bounds stays transparent"
    );
    Ok(())
}

/// `draw_surface` honours the optional paint's alpha multiplier so the
/// destination receives a half-strength composite when alpha is 0.5.
#[test]
fn draw_surface_with_paint_modulates_alpha() -> Result<()> {
    let backend = NativeBackend::new();
    let mut source = backend.create_surface(4, 4, SurfaceOptions::default())?;
    source.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::opaque(1.0, 0.0, 0.0));
    });

    let mut dest = backend.create_surface(4, 4, SurfaceOptions::default())?;
    dest.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        let mut paint = NativePaint::default();
        paint.set_alpha(0.5);
        canvas.draw_surface(&mut source, 0.0, 0.0, Some(&paint));
    });
    let px = dest.read_pixels()?;
    let alpha = px.pixels()[3];
    assert!((110..=145).contains(&alpha), "alpha 0.5 ≈ 128, got {alpha}");
    Ok(())
}

// --- Chunk 3C: paths, line, draw_image_src, sampling ----------------------

/// SVG path data renders visible pixels.
#[test]
fn svg_path_draws_visible_pixels() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(8, 8, SurfaceOptions::default())?;
    let path = NativePath::from_svg("M0 0 L8 0 L8 8 L0 8 Z", FillRule::NonZero)?;
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_path(&path, &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)));
    });
    let px = surface.read_pixels()?;
    assert!(px.pixels()[3] > 240, "filled square covers (0,0)");
    Ok(())
}

/// On a self-overlapping path (two same-direction concentric squares), the
/// inner region winds twice. `NonZero` fills the inner region (count != 0);
/// `EvenOdd` leaves the inner region empty (count mod 2 == 0). The two
/// renderings must produce different pixels at the inner region's center.
#[test]
fn fill_rule_evenodd_differs_from_nonzero_on_nested_path() -> Result<()> {
    let backend = NativeBackend::new();
    // Two same-direction (CW) concentric squares: outer 8x8, inner 4x4 hole.
    let svg = "M0 0 L8 0 L8 8 L0 8 Z M2 2 L6 2 L6 6 L2 6 Z";

    let nonzero = {
        let mut surface = backend.create_surface(8, 8, SurfaceOptions::default())?;
        let path = NativePath::from_svg(svg, FillRule::NonZero)?;
        surface.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
            canvas.draw_path(&path, &NativePaint::fill(RgbaLinear::opaque(1.0, 1.0, 1.0)));
        });
        surface.read_pixels()?
    };

    let evenodd = {
        let mut surface = backend.create_surface(8, 8, SurfaceOptions::default())?;
        let path = NativePath::from_svg(svg, FillRule::EvenOdd)?;
        surface.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
            canvas.draw_path(&path, &NativePaint::fill(RgbaLinear::opaque(1.0, 1.0, 1.0)));
        });
        surface.read_pixels()?
    };

    let stride = nonzero.stride();
    let alpha_at = |buf: &[u8], x: usize, y: usize| buf[y * stride + x * 4 + 3];

    // Inner pixel (4, 4): NonZero fills (winding=2), EvenOdd leaves empty.
    let nz_inner = alpha_at(nonzero.pixels(), 4, 4);
    let eo_inner = alpha_at(evenodd.pixels(), 4, 4);
    assert!(nz_inner > 240, "NonZero fills inner: alpha={nz_inner}");
    assert_eq!(eo_inner, 0, "EvenOdd leaves inner empty: alpha={eo_inner}");

    // Outer ring pixel (1, 1) is filled by both rules.
    assert!(alpha_at(nonzero.pixels(), 1, 1) > 240);
    assert!(alpha_at(evenodd.pixels(), 1, 1) > 240);
    Ok(())
}

/// `clip_path` masks subsequent draws to the path interior.
#[test]
fn clip_path_clips_drawing() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(8, 8, SurfaceOptions::default())?;
    let clip = NativePath::from_svg("M2 2 L6 2 L6 6 L2 6 Z", FillRule::NonZero)?;
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.save();
        canvas.clip_path(&clip);
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 8.0, 8.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
        canvas.restore();
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    let alpha_at = |x: usize, y: usize| px.pixels()[y * stride + x * 4 + 3];
    assert_eq!(alpha_at(0, 0), 0, "outside path stays transparent");
    assert_eq!(alpha_at(7, 7), 0, "outside path stays transparent");
    assert!(alpha_at(4, 4) > 240, "inside path is filled");
    Ok(())
}

/// `draw_line` honours stroke width: a horizontal line with width=4 covers
/// the y rows immediately above/below the line midpoint.
#[test]
fn draw_line_respects_stroke_width() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(8, 8, SurfaceOptions::default())?;
    let mut paint = NativePaint::stroke(RgbaLinear::opaque(1.0, 1.0, 1.0), 4.0);
    paint.set_anti_alias(false);
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_line(Point::new(0.0, 4.0), Point::new(8.0, 4.0), &paint);
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    let alpha_at = |x: usize, y: usize| px.pixels()[y * stride + x * 4 + 3];
    // Line is centered at y=4 with width 4, so y in {2,3,4,5} are covered.
    assert_eq!(alpha_at(4, 0), 0, "row 0 outside stroke band");
    assert!(alpha_at(4, 3) > 240, "row 3 inside stroke band");
    assert!(alpha_at(4, 4) > 240, "row 4 (line center) covered");
    assert_eq!(alpha_at(4, 7), 0, "row 7 outside stroke band");
    Ok(())
}

/// `draw_line` with a round cap extends past the line endpoints, while a
/// butt cap stops cleanly. Sample alpha just before x=0 with the same line
/// segment under both caps.
#[test]
fn draw_line_round_cap_extends_past_endpoints() -> Result<()> {
    let backend = NativeBackend::new();
    let alpha_at = |cap: StrokeCap| -> Result<u8> {
        let mut surface = backend.create_surface(16, 8, SurfaceOptions::default())?;
        let mut paint = NativePaint::stroke(RgbaLinear::opaque(1.0, 1.0, 1.0), 4.0);
        paint.set_stroke_cap(cap);
        paint.set_anti_alias(false);
        surface.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
            // Line from (4,4) to (12,4): butt cap stops at x=4, round cap
            // extends ~2px further (radius == half stroke width).
            canvas.draw_line(Point::new(4.0, 4.0), Point::new(12.0, 4.0), &paint);
        });
        let px = surface.read_pixels()?;
        let stride = px.stride();
        Ok(px.pixels()[4 * stride + 2 * 4 + 3])
    };
    let butt = alpha_at(StrokeCap::Butt)?;
    let round = alpha_at(StrokeCap::Round)?;
    assert_eq!(butt, 0, "butt cap stops at x=4");
    assert!(
        round > 200,
        "round cap extends past endpoint: alpha={round}"
    );
    Ok(())
}

/// `draw_line` with a dash leaves periodic gaps along the path.
#[test]
fn draw_line_dash_creates_periodic_gaps() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(40, 4, SurfaceOptions::default())?;
    let mut paint = NativePaint::stroke(RgbaLinear::opaque(1.0, 1.0, 1.0), 2.0);
    paint.set_dash(vec![4.0, 4.0], 0.0);
    paint.set_anti_alias(false);
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_line(Point::new(0.0, 2.0), Point::new(40.0, 2.0), &paint);
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    // Sample row y=1 (just above the y=2 line center; the 2-px stroke
    // covers y=1 and y=2 with butt caps and no anti-alias).
    let row_start = stride;
    let row = &px.pixels()[row_start..row_start + stride];
    let alphas: Vec<u8> = row.chunks_exact(4).map(|p| p[3]).collect();
    let visible = alphas.iter().filter(|a| **a > 0).count();
    let invisible = alphas.iter().filter(|a| **a == 0).count();
    assert!(visible > 0, "dashed line should still produce some pixels");
    assert!(invisible > 0, "dashed line should leave some gaps");
    Ok(())
}

/// `draw_image_src` crops the source rect when scaling. A 4x4 source with
/// red top-left and blue bottom-right: drawing src=(0,0,2,2) (red region)
/// stretched to fill an 8x8 destination must be uniformly red.
#[test]
fn draw_image_src_crops_source_rect() -> Result<()> {
    let backend = NativeBackend::new();
    let mut source = backend.create_surface(4, 4, SurfaceOptions::default())?;
    source.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::opaque(0.0, 0.0, 1.0));
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 2.0, 2.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
    });
    let image = source.snapshot();

    let mut dest = backend.create_surface(8, 8, SurfaceOptions::default())?;
    dest.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_image_src(
            &image,
            Rect::from_xywh(0.0, 0.0, 2.0, 2.0),
            Rect::from_xywh(0.0, 0.0, 8.0, 8.0),
            None,
            SamplingMode::Nearest,
        );
    });
    let px = dest.read_pixels()?;
    let stride = px.stride();
    let pixel_at = |x: usize, y: usize| -> [u8; 4] {
        let off = y * stride + x * 4;
        [
            px.pixels()[off],
            px.pixels()[off + 1],
            px.pixels()[off + 2],
            px.pixels()[off + 3],
        ]
    };
    // Every destination pixel should reflect the red source region. With
    // unpremul Uint8, red opaque is approximately (255, 0, 0, 255).
    for y in 0..8 {
        for x in 0..8 {
            let p = pixel_at(x, y);
            assert!(p[0] > 240, "({x},{y}) red expected, got {p:?}");
            assert!(p[2] < 16, "({x},{y}) blue should be absent, got {p:?}");
        }
    }
    Ok(())
}

/// `SamplingMode::Nearest` preserves the hard edge between adjacent
/// source pixels of different colors when upscaling. A 2x2 red/blue
/// checker scaled to 8x8 must show a sharp red->blue transition at x=4.
#[test]
fn sampling_nearest_preserves_hard_edges() -> Result<()> {
    let backend = NativeBackend::new();
    let mut source = backend.create_surface(2, 2, SurfaceOptions::default())?;
    source.with_canvas(|canvas| {
        canvas.draw_rect(
            Rect::from_xywh(0.0, 0.0, 1.0, 1.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
        canvas.draw_rect(
            Rect::from_xywh(1.0, 0.0, 1.0, 1.0),
            &NativePaint::fill(RgbaLinear::opaque(0.0, 0.0, 1.0)),
        );
        canvas.draw_rect(
            Rect::from_xywh(0.0, 1.0, 1.0, 1.0),
            &NativePaint::fill(RgbaLinear::opaque(0.0, 0.0, 1.0)),
        );
        canvas.draw_rect(
            Rect::from_xywh(1.0, 1.0, 1.0, 1.0),
            &NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0)),
        );
    });
    let image = source.snapshot();

    let mut dest = backend.create_surface(8, 8, SurfaceOptions::default())?;
    dest.with_canvas(|canvas| {
        canvas.draw_image_src(
            &image,
            Rect::from_xywh(0.0, 0.0, 2.0, 2.0),
            Rect::from_xywh(0.0, 0.0, 8.0, 8.0),
            None,
            SamplingMode::Nearest,
        );
    });
    let px = dest.read_pixels()?;
    let stride = px.stride();
    // Just before transition (x=3) and just after (x=4) on row 0.
    let left = (px.pixels()[3 * 4], px.pixels()[3 * 4 + 2]);
    let right = (px.pixels()[4 * 4], px.pixels()[4 * 4 + 2]);
    let _ = stride; // unused: row 0 starts at byte 0.
    assert!(left.0 > 240, "left of edge: red dominant, got r={}", left.0);
    assert!(left.1 < 16, "left of edge: blue absent, got b={}", left.1);
    assert!(right.0 < 16, "right of edge: red absent, got r={}", right.0);
    assert!(
        right.1 > 240,
        "right of edge: blue dominant, got b={}",
        right.1
    );
    Ok(())
}

/// `SamplingMode::Linear` and `Mipmapped` produce non-empty output without
/// panicking. Exact pixels are backend-sensitive, so we only smoke-test
/// that the draw succeeds and writes some non-zero pixels.
#[test]
fn sampling_linear_and_mipmapped_smoke() -> Result<()> {
    let backend = NativeBackend::new();
    let mut source = backend.create_surface(2, 2, SurfaceOptions::default())?;
    source.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::opaque(0.5, 0.5, 0.5));
    });
    let image = source.snapshot();

    for mode in [SamplingMode::Linear, SamplingMode::Mipmapped] {
        let mut dest = backend.create_surface(8, 8, SurfaceOptions::default())?;
        dest.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
            canvas.draw_image_src(
                &image,
                Rect::from_xywh(0.0, 0.0, 2.0, 2.0),
                Rect::from_xywh(0.0, 0.0, 8.0, 8.0),
                None,
                mode,
            );
        });
        let px = dest.read_pixels()?;
        assert!(
            px.pixels().iter().any(|c| *c != 0),
            "{mode:?} should produce non-zero pixels"
        );
    }
    Ok(())
}

// --- Chunk 4A: filters and color filters -----------------------------------

/// Blur softens and expands non-transparent regions: pixels just outside
/// the original sharp rect must gain non-zero alpha.
#[test]
fn image_filter_blur_expands_alpha() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(16, 16, SurfaceOptions::default())?;
    let mut paint = NativePaint::fill(RgbaLinear::opaque(1.0, 1.0, 1.0));
    paint.set_image_filter(Some(NativeImageFilter::blur(3.0, 3.0, None)?));
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_rect(Rect::from_xywh(6.0, 6.0, 4.0, 4.0), &paint);
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    let alpha_at = |x: usize, y: usize| px.pixels()[y * stride + x * 4 + 3];
    // Interior alpha softens under strong blur but remains substantially
    // above zero.
    assert!(
        alpha_at(7, 7) > 32,
        "rect interior keeps alpha under blur, got {}",
        alpha_at(7, 7)
    );
    // Just outside the rect on each side: blur leaks alpha into the halo.
    assert!(alpha_at(4, 8) > 8, "left of rect should have blur halo");
    assert!(alpha_at(11, 8) > 8, "right of rect should have blur halo");
    assert!(alpha_at(8, 4) > 8, "above rect should have blur halo");
    assert!(alpha_at(8, 11) > 8, "below rect should have blur halo");
    Ok(())
}

/// Drop shadow renders an offset blur of the source. Pixels at the offset
/// position (outside the source rect) must show the shadow color.
#[test]
fn image_filter_drop_shadow_offsets_pixels() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(16, 16, SurfaceOptions::default())?;
    let mut paint = NativePaint::fill(RgbaLinear::opaque(1.0, 1.0, 1.0));
    paint.set_image_filter(Some(NativeImageFilter::drop_shadow(
        4.0,
        4.0,
        1.0,
        1.0,
        RgbaLinear::opaque(1.0, 0.0, 0.0),
        None,
    )?));
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_rect(Rect::from_xywh(2.0, 2.0, 4.0, 4.0), &paint);
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    // The shadow centers at (offset + rect): around (6..10, 6..10). Sample
    // a pixel inside the shadow region (well outside the source rect).
    let off = 8 * stride + 8 * 4;
    let r = px.pixels()[off];
    let a = px.pixels()[off + 3];
    assert!(a > 32, "drop shadow region has alpha: {a}");
    assert!(r > 64, "drop shadow region carries shadow red: r={r}");
    Ok(())
}

/// A color matrix can replace RGB. Using a matrix that swaps red and blue,
/// drawing a blue rect produces red pixels.
#[test]
fn image_filter_color_matrix_replaces_rgb() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(4, 4, SurfaceOptions::default())?;
    // Swap red and blue (rows are RGBA; columns are R G B A offset).
    let swap_rb: [f32; 20] = [
        0.0, 0.0, 1.0, 0.0, 0.0, // r_out = b_in
        0.0, 1.0, 0.0, 0.0, 0.0, // g_out = g_in
        1.0, 0.0, 0.0, 0.0, 0.0, // b_out = r_in
        0.0, 0.0, 0.0, 1.0, 0.0, // a_out = a_in
    ];
    let mut paint = NativePaint::fill(RgbaLinear::opaque(0.0, 0.0, 1.0));
    paint.set_image_filter(Some(NativeImageFilter::color_matrix(swap_rb, None)?));
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), &paint);
    });
    let px = surface.read_pixels()?;
    let r = px.pixels()[0];
    let b = px.pixels()[2];
    assert!(r > 240, "red dominant after RB swap: {r}");
    assert!(b < 16, "blue absent after RB swap: {b}");
    Ok(())
}

/// `from_color_filter` wraps a color filter as an image filter and
/// produces the same effect when applied via `paint.image_filter`.
#[test]
fn image_filter_from_color_filter_applies_as_image_filter() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(4, 4, SurfaceOptions::default())?;
    let cf = NativeColorFilter::linear_to_srgb_gamma();
    let if_ = NativeImageFilter::from_color_filter(cf, None)?;
    let mut paint = NativePaint::fill(RgbaLinear::opaque(0.5, 0.5, 0.5));
    paint.set_image_filter(Some(if_));
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0), &paint);
    });
    let px = surface.read_pixels()?;
    // Wrapping a color filter as an image filter should still produce
    // non-zero pixels for an opaque draw.
    assert!(px.pixels()[3] > 240, "opaque draw remains opaque");
    Ok(())
}

/// Composing two image filters chains them: outer(inner(source)).
/// Compose blur(8) outer with color_matrix(swap RB) inner. Inner runs first
/// (turns blue source into red source in the filter pipeline), then blur
/// expands. Sample outside the source rect: blurred red appears.
#[test]
fn image_filter_compose_chains_inner_then_outer() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(16, 16, SurfaceOptions::default())?;
    let swap_rb: [f32; 20] = [
        0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        1.0, 0.0,
    ];
    let inner = NativeImageFilter::color_matrix(swap_rb, None)?;
    let outer = NativeImageFilter::blur(2.0, 2.0, None)?;
    let composed = NativeImageFilter::compose(outer, inner)?;
    let mut paint = NativePaint::fill(RgbaLinear::opaque(0.0, 0.0, 1.0));
    paint.set_image_filter(Some(composed));
    surface.with_canvas(|canvas| {
        canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
        canvas.draw_rect(Rect::from_xywh(6.0, 6.0, 4.0, 4.0), &paint);
    });
    let px = surface.read_pixels()?;
    let stride = px.stride();
    let off = 8 * stride + 8 * 4;
    let r = px.pixels()[off];
    let b = px.pixels()[off + 2];
    assert!(
        r > 200,
        "composed pipeline turned blue input into red: r={r}"
    );
    assert!(b < 32, "blue should be absent: b={b}");
    Ok(())
}

/// Luma color filter: alpha = perceived luminance, RGB = 0. Drawing a white
/// fill with luma applied keeps the surface visible (alpha 1); drawing a
/// black fill becomes invisible (alpha 0). This is the building block for
/// destination-in mask paths.
#[test]
fn color_filter_luma_maps_luminance_to_alpha() -> Result<()> {
    let backend = NativeBackend::new();
    let render_with = |color: RgbaLinear| -> Result<u8> {
        let mut surface = backend.create_surface(2, 2, SurfaceOptions::default())?;
        let mut paint = NativePaint::fill(color);
        paint.set_color_filter(Some(NativeColorFilter::luma()));
        surface.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
            canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), &paint);
        });
        let px = surface.read_pixels()?;
        Ok(px.pixels()[3])
    };
    let white_alpha = render_with(RgbaLinear::opaque(1.0, 1.0, 1.0))?;
    let black_alpha = render_with(RgbaLinear::opaque(0.0, 0.0, 0.0))?;
    assert!(
        white_alpha > 240,
        "luma maps white -> high alpha, got {white_alpha}"
    );
    assert!(
        black_alpha < 16,
        "luma maps black -> ~0 alpha, got {black_alpha}"
    );
    Ok(())
}

/// `srgb_to_linear_gamma` and `linear_to_srgb_gamma` round-trip: applying
/// inner srgb_to_linear and outer linear_to_srgb produces visually similar
/// output to the original draw on an sRGB-coded readback.
#[test]
fn color_filter_gamma_round_trip_through_compose() -> Result<()> {
    let backend = NativeBackend::new();
    let direct = {
        let mut surface = backend.create_surface(2, 2, SurfaceOptions::default())?;
        surface.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
            canvas.draw_rect(
                Rect::from_xywh(0.0, 0.0, 2.0, 2.0),
                &NativePaint::fill(RgbaLinear::opaque(0.5, 0.5, 0.5)),
            );
        });
        surface.read_pixels()?
    };

    let composed = {
        let outer = NativeColorFilter::linear_to_srgb_gamma();
        let inner = NativeColorFilter::srgb_to_linear_gamma();
        let cf = NativeColorFilter::compose(outer, inner)?;
        let mut paint = NativePaint::fill(RgbaLinear::opaque(0.5, 0.5, 0.5));
        paint.set_color_filter(Some(cf));
        let mut surface = backend.create_surface(2, 2, SurfaceOptions::default())?;
        surface.with_canvas(|canvas| {
            canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
            canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 2.0, 2.0), &paint);
        });
        surface.read_pixels()?
    };

    let dr = (direct.pixels()[0] as i16 - composed.pixels()[0] as i16).abs();
    let dg = (direct.pixels()[1] as i16 - composed.pixels()[1] as i16).abs();
    let db = (direct.pixels()[2] as i16 - composed.pixels()[2] as i16).abs();
    assert!(
        dr <= 8 && dg <= 8 && db <= 8,
        "gamma round-trip differs by at most 8/255: dr={dr} dg={dg} db={db}"
    );
    Ok(())
}

// --- Chunk 4B: shaders -----------------------------------------------------

/// Gradient stops must be sorted by position. An out-of-order stop list
/// must surface a typed error rather than silently rendering.
#[test]
fn gradient_unsorted_stops_returns_invalid_gradient_error() {
    let result = NativeShader::linear_gradient(
        Point::new(0.0, 0.0),
        Point::new(16.0, 0.0),
        &[
            GradientStop {
                position: 0.5,
                color: RgbaLinear::opaque(1.0, 0.0, 0.0),
            },
            GradientStop {
                position: 0.0,
                color: RgbaLinear::opaque(0.0, 0.0, 1.0),
            },
        ],
        GradientInterpolation::Srgb,
    );
    assert!(
        matches!(result, Err(NativeError::InvalidGradient { .. })),
        "expected InvalidGradient, got {result:?}"
    );
}

#[test]
fn gradient_requires_at_least_two_stops() {
    let result = NativeShader::linear_gradient(
        Point::new(0.0, 0.0),
        Point::new(16.0, 0.0),
        &[GradientStop {
            position: 0.0,
            color: RgbaLinear::opaque(1.0, 0.0, 0.0),
        }],
        GradientInterpolation::Srgb,
    );
    assert!(matches!(result, Err(NativeError::InvalidGradient { .. })));
}

/// Linear sRGB gradient renders the expected endpoints. A 16x1 horizontal
/// red->blue gradient must show ~red at x=0 and ~blue at x=15.
#[test]
fn linear_gradient_srgb_renders_endpoints() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(16, 1, SurfaceOptions::default())?;
    let shader = NativeShader::linear_gradient(
        Point::new(0.0, 0.0),
        Point::new(16.0, 0.0),
        &[
            GradientStop {
                position: 0.0,
                color: RgbaLinear::opaque(1.0, 0.0, 0.0),
            },
            GradientStop {
                position: 1.0,
                color: RgbaLinear::opaque(0.0, 0.0, 1.0),
            },
        ],
        GradientInterpolation::Srgb,
    )?;
    let mut paint = NativePaint::default();
    paint.set_shader(Some(shader));
    surface.with_canvas(|canvas| {
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 16.0, 1.0), &paint);
    });
    let px = surface.read_pixels()?;
    // Pixel centers are at x=0.5 and x=15.5, so a small linear
    // contribution from the far stop survives. With sRGB-gamma Uint8
    // readback, a 0.03 linear blue rounds to ~50 in u8, so thresholds
    // are loose. The dominant channel must remain clearly dominant.
    let r0 = px.pixels()[0];
    let b0 = px.pixels()[2];
    assert!(r0 > 200, "left endpoint ~ red, got r={r0}");
    assert!(b0 < 80, "left endpoint mostly red, got b={b0}");
    let r15 = px.pixels()[15 * 4];
    let b15 = px.pixels()[15 * 4 + 2];
    assert!(r15 < 80, "right endpoint mostly blue, got r={r15}");
    assert!(b15 > 200, "right endpoint ~ blue, got b={b15}");
    Ok(())
}

/// Stops at non-extreme positions are honoured. Three-stop red->green->blue
/// gradient: x=0 red, x=8 green, x=15 blue.
#[test]
fn linear_gradient_three_stops_renders_in_order() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(16, 1, SurfaceOptions::default())?;
    let shader = NativeShader::linear_gradient(
        Point::new(0.0, 0.0),
        Point::new(16.0, 0.0),
        &[
            GradientStop {
                position: 0.0,
                color: RgbaLinear::opaque(1.0, 0.0, 0.0),
            },
            GradientStop {
                position: 0.5,
                color: RgbaLinear::opaque(0.0, 1.0, 0.0),
            },
            GradientStop {
                position: 1.0,
                color: RgbaLinear::opaque(0.0, 0.0, 1.0),
            },
        ],
        GradientInterpolation::Srgb,
    )?;
    let mut paint = NativePaint::default();
    paint.set_shader(Some(shader));
    surface.with_canvas(|canvas| {
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 16.0, 1.0), &paint);
    });
    let px = surface.read_pixels()?;
    let r0 = px.pixels()[0];
    let g8 = px.pixels()[8 * 4 + 1];
    let b15 = px.pixels()[15 * 4 + 2];
    assert!(r0 > 200, "x=0 ~ red, got r={r0}");
    assert!(g8 > 150, "x=8 ~ green, got g={g8}");
    assert!(b15 > 200, "x=15 ~ blue, got b={b15}");
    Ok(())
}

/// OKLCH interpolation produces a perceptually different intermediate
/// from sRGB interpolation. Red->blue at the midpoint:
/// - sRGB linear: leans toward dark purple/grey, very low green.
/// - OKLCH: passes through more saturated colors with higher visible
///   intensity in non-red, non-blue channels.
///
/// We assert that the midpoint pixel differs by at least one channel
/// across the two interpolations. Exact values are backend-sensitive.
#[test]
fn linear_gradient_oklch_differs_from_srgb_at_midpoint() -> Result<()> {
    let backend = NativeBackend::new();
    let render = |interp: GradientInterpolation| -> Result<[u8; 4]> {
        let mut surface = backend.create_surface(16, 1, SurfaceOptions::default())?;
        let shader = NativeShader::linear_gradient(
            Point::new(0.0, 0.0),
            Point::new(16.0, 0.0),
            &[
                GradientStop {
                    position: 0.0,
                    color: RgbaLinear::opaque(1.0, 0.0, 0.0),
                },
                GradientStop {
                    position: 1.0,
                    color: RgbaLinear::opaque(0.0, 0.0, 1.0),
                },
            ],
            interp,
        )?;
        let mut paint = NativePaint::default();
        paint.set_shader(Some(shader));
        surface.with_canvas(|canvas| {
            canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 16.0, 1.0), &paint);
        });
        let px = surface.read_pixels()?;
        Ok([
            px.pixels()[8 * 4],
            px.pixels()[8 * 4 + 1],
            px.pixels()[8 * 4 + 2],
            px.pixels()[8 * 4 + 3],
        ])
    };
    let srgb = render(GradientInterpolation::Srgb)?;
    let oklch = render(GradientInterpolation::Oklch)?;
    assert_ne!(
        srgb, oklch,
        "OKLCH must produce a different midpoint than sRGB-linear; got equal {srgb:?}"
    );
    Ok(())
}

/// `paint.set_shader(None)` clears the shader so the paint's `color`
/// drives the draw again.
#[test]
fn paint_set_shader_none_falls_back_to_color() -> Result<()> {
    let backend = NativeBackend::new();
    let mut surface = backend.create_surface(2, 1, SurfaceOptions::default())?;
    let mut paint = NativePaint::fill(RgbaLinear::opaque(1.0, 0.0, 0.0));
    let shader = NativeShader::linear_gradient(
        Point::new(0.0, 0.0),
        Point::new(2.0, 0.0),
        &[
            GradientStop {
                position: 0.0,
                color: RgbaLinear::opaque(0.0, 1.0, 0.0),
            },
            GradientStop {
                position: 1.0,
                color: RgbaLinear::opaque(0.0, 1.0, 0.0),
            },
        ],
        GradientInterpolation::Srgb,
    )?;
    paint.set_shader(Some(shader));
    paint.set_shader(None);
    surface.with_canvas(|canvas| {
        canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 2.0, 1.0), &paint);
    });
    let px = surface.read_pixels()?;
    let r0 = px.pixels()[0];
    let g0 = px.pixels()[1];
    assert!(r0 > 200, "color path renders red, got r={r0}");
    assert!(g0 < 32, "no green when shader cleared, got g={g0}");
    Ok(())
}
