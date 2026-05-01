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
    BlendMode, LinearColorSpace, NativeAffine, NativeBackend, NativePaint, PaintStyle,
    PixelColorSpace, PixelDepth, PixelExportOptions, Rect, RgbaLinear, StrokeCap, SurfaceOptions,
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
