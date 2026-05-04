# Studio Renderer Native API Gap Closure Implementation Plan

> **For agentic workers:** REQUIRED: Use `superpowers:subagent-driven-development` (if subagents available) or `superpowers:executing-plans` to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extend `phyron_skia_canvas::native` until it can cover the required `@phyron/studio-renderer` backend surface from the Studio TypeScript package, so `studio-render-native` can depend on `phyron-skia-canvas` instead of `skia-safe` without losing renderer parity.

**Architecture:** Promote the mature Skia functionality already present behind the Node/Canvas2D binding into a Rust-only facade. Keep `skia_safe` and Neon private. Add a real surface/canvas/paint/filter/text/image API that mirrors the renderer contract in `packages/renderer`, then migrate Studio against that contract.

**Tech Stack:** Rust 2024, private `skia-safe` internals, `phyron-skia-canvas` native facade, `parking_lot` for shared Rust-facing state when needed, `just`, linked `.blueprints` submodule, downstream Studio `@phyron/studio-renderer` contract.

---

## Status And Approval

API changes are approved by Moritz on 2026-05-01 for this work. This plan is a follow-up to `docs/superpowers/plans/2026-05-01-native-rust-api-boundary.md`.

### Implementation status (2026-05-02)

- **Chunks 1-2 (Tasks 1-4): complete** on branch `plan/studio-renderer-gap-closure`, stacked on `plan/native-rust-api-boundary` (PR #5).
  - `NativeBackend`, `NativeSurface` with `with_canvas`, `snapshot`, `create_offscreen`, `flush`.
  - `PixelColorSpace` (6 variants), `PixelDepth` (Uint8/F16/F32), `PixelExportOptions`, `ExportedPixels`.
  - `read_pixels`, `read_pixels_raw`, `read_pixels_as`, `read_pixels_linear`, `write_pixels`, `write_pixels_linear`.
  - 7 contract tests in `tests/native_studio_renderer_contract.rs` -- all green.
- **Chunk 3A (Task 5): complete** on the same branch. Per reviewer feedback Chunk 3 was sub-split:
  - `NativePaint` with color/style/stroke_width/stroke_cap/dash/anti_alias/alpha/blend_mode plus `Option<NativeShader/ImageFilter/ColorFilter>` typed placeholders.
  - `PaintStyle::{Fill, Stroke}`, `StrokeCap::{Butt, Round, Square}`, `DashPattern`.
  - Full `BlendMode` enum (Canvas-compatible plus `PlusLighter`).
  - `ShapePaint` removed -- `NativePaint::fill`/`stroke` constructors replace it.
  - 7 new tests cover defaults, constructors, alpha modulation, blend mode distinctness, every-blend-mode plumbing, stroke cap state, and dash state.
- **Chunk 3B (canvas state + layer subset of Task 6): complete** on the same branch.
  - `NativeAffine` (CSS DOMMatrix2DInit form) with `IDENTITY`, `translation`, `scale`, `rotation_radians`, `rotation_degrees` constructors.
  - `NativeCanvas::scale`, `concat_transform`, `save_layer(Option<&NativePaint>)`, `clip_rect`, `clip_rrect`, `draw_surface`.
  - 9 new tests cover: clip_rect masking, clip_rrect rounded corners, transform translation, transform scale, scale-helper-equals-affine, layer opacity isolation, layer blend isolation, draw_surface offset, draw_surface with paint alpha.
- **Chunk 3C (paths, line, draw_image_src, sampling): complete** on the same branch.
  - `NativePath::from_svg(svg_data, fill_rule)` parses SVG path data via `skia_safe::utils::parse_path::from_svg` and applies the fill rule.
  - `FillRule::{NonZero, EvenOdd}`.
  - `NativeCanvas::clip_path`, `draw_path`, `draw_line`, `draw_image_src`.
  - `SamplingMode::{Nearest, Linear, Mipmapped}`.
  - 9 new tests cover: SVG path renders, fill rule difference on a nested same-direction path, clip_path masks, draw_line stroke width, draw_line round-cap extension past endpoints, draw_line dash gaps, draw_image_src cropping, Nearest sharp edges, Linear/Mipmapped smoke.
- **Chunk 4A (filters / color filters subset of Task 7): complete** on the same branch. Per reviewer feedback Chunk 4 was sub-split:
  - `NativeImageFilter::{blur, drop_shadow, color_matrix, from_color_filter, compose}` returning `Result<NativeImageFilter, NativeError>`.
  - `NativeColorFilter::{luma, srgb_to_linear_gamma, linear_to_srgb_gamma, compose}`.
  - `NativePaint::set_image_filter` / `set_color_filter` setters; `to_skia_paint` applies both.
  - `NativeImageFilter` and `NativeColorFilter` moved out of `paint.rs` into `src/native/filter.rs` with private `inner` skia handles.
  - 7 new tests cover: blur expanding alpha, drop shadow offset pixels, color matrix RGB swap, color filter wrapped as image filter, image filter compose chains inner-then-outer, luma maps luminance to alpha (white visible / black invisible), gamma compose round-trip.
- **Chunk 4B (shaders subset of Task 7): complete** on the same branch.
  - `NativeShader::linear_gradient(start, end, stops, interpolation_space)` returns `Result<NativeShader, NativeError>`. Validates stop count (>= 2), sorted positions, and 0..=1 range; failures return `NativeError::InvalidGradient`.
  - `GradientStop { position, color }` carries `RgbaLinear` colors in the surface working color space.
  - `GradientInterpolation::{Srgb, Oklch}` maps to Skia's `interpolation::ColorSpace::SRGBLinear` and `OKLCH`. Both go through Skia's gradient pipeline directly -- no silent fallback.
  - `NativePaint::set_shader(Option<NativeShader>)` setter; `to_skia_paint` plumbs the shader through.
  - 5 new tests cover: unsorted stops error, two-stop count error, sRGB endpoints render correctly, three-stop ordered render, OKLCH midpoint differs from sRGB, `set_shader(None)` falls back to paint color.
- **Chunk 5 (Task 8 -- raw image creation): complete** on the same branch.
  - `NativeImage::from_pixels(bytes, width, height, stride, pixel_format, color_space)` returns `Result<NativeImage, NativeError>`. Strict validation: zero dimensions return `InvalidDimensions`, stride < width * bpp returns `InvalidStride`, byte length mismatch returns `InvalidByteLength`. Pixels are copied into a Skia raster image.
  - Color space input is `PixelColorSpace` (not the surface-only `LinearColorSpace`), so callers explicitly state whether their pixels are gamma-coded sRGB / Display P3 / Rec.2020 or the linear-light counterparts. No implicit sRGB fallback.
  - Pixel formats: `Rgba8UnormPremul`, `Rgba8UnormUnpremul`, `Rgba16fPremul`, `Rgba32fPremul`. The intended bridge for rsmpeg-decoded video frames and Citra-generated images.
  - `NativeImage::is_premultiplied()` exposes the alpha mode of the underlying image.
  - 7 new tests cover: RGBA8 unpremul draws end-to-end, zero dimensions error, invalid stride error, invalid byte length error, F32 HDR (r=2.0) preservation through draw + linear readback, F16 HDR preservation, Rgba8UnormPremul round-trip preserves premultiplied values.
- **Chunk 6 (Task 9 -- SVG behavior): complete** on the same branch.
  - Confirmed `NativePath::from_svg` accepts the full SVG mini-language including relative commands and quadratic/cubic curves; added a Studio-style mixed-command path test.
  - Pinned that `NativeImage::from_encoded` does NOT decode SVG XML (it is a raster-codec API); the wrapper surfaces this as a typed `DecodeImage` error.
  - Added explicit `NativeImage::from_svg_xml(svg, width, height)` using `skia_safe::svg::Dom` + a raster surface snapshot. Container size is set from `(width, height)`. Zero dimensions return `InvalidDimensions`; malformed XML returns `DecodeImage`.
  - 5 new tests cover: relative-and-curve SVG path renders, `from_encoded` SVG returns a decode error, `from_svg_xml` rasterizes a minimal `<rect>` SVG, zero-dimension input rejected, malformed XML rejected.
- **Chunk 7A (Task 10 -- font manager): complete** on the same branch. Per reviewer feedback Chunk 7 was sub-split into 7A (fonts), 7B (simple paragraph layout), 7C (rich spans + metrics).
  - `NativeFontManager::new` builds an empty registry.
  - `register_font_from_data(family, bytes)` parses TTF/OTF/WOFF/WOFF2 byte streams via `FontMgr::new_from_data` and registers the typeface under the family alias.
  - `register_font_from_path(family, path)` reads the file then delegates.
  - `has_font(family)` and `families()` query the registered aliases.
  - Internal state lives behind `parking_lot::Mutex` (added as a direct dependency); no `RefCell` leaks. The manager is single-threaded by Skia's binding constraints (`TypefaceFontProvider` is not `Send`); cross-thread sharing is not promised.
  - New `NativeError::FontRegister { reason }` reports parse failures and IO errors.
  - 8 new contract tests: starts empty, register-from-data lists family, register-from-path lists family, duplicate registration does not duplicate alias, multiple families tracked in registration order, garbage bytes return `FontRegister`, missing path returns `FontRegister`, interior mutability through `&NativeFontManager` (no `RefCell`).
- **Chunk 7B (Task 11 -- plain paragraph layout): complete** on the same branch.
  - `TextStyle { font_families, font_size, font_weight, slant, color, align, line_height_multiplier }`. Plain-text only; rich spans, decorations, shadows, letter/word spacing, baseline shifts stay deferred to 7C.
  - `TextSlant::{Upright, Italic, Oblique}` -- the slant axis exposed at the public boundary.
  - `NativeTextEngine::new(&NativeFontManager)` wires the registered typefaces into a Skia `FontCollection` (with system fallback). `with_system_fonts()` is the no-registry convenience.
  - `NativeTextEngine::layout_text(text, style, max_width)` returns a `NativeTextLayout`.
  - `NativeTextLayout::width()` returns the **measured longest-line width** (matches the TS renderer's `TextLayout.width` semantics), not the wrapping budget. `max_width()` recovers the caller-requested budget. `height()`, `line_count()`, and `first_line_ascent()` expose the rest of the metrics needed for vertical alignment and box sizing.
  - `NativeCanvas::draw_text_layout(layout, x, y)` paints the laid-out paragraph at `(x, y)`.
  - WOFF/WOFF2 byte streams are accepted by Skia's `FontMgr::new_from_data` under the project's `freetype` + `freetype-woff2` features; verified by a contract test using the Monoton fixtures.
  - 8 contract tests cover: visible text pixels, center/right alignment shifts the inked column, narrow wrapping increases line count and height, first-line ascent grows with font size, layout metrics are well-formed and respect the budget, `width()` reflects measured content (not max_width), registered fonts produce different ink than system fallbacks when requested by family name, WOFF and WOFF2 register through the font manager.
- **Chunk 7C (Task 11 -- rich spans + metrics): complete** on the same branch.
  - `TextStyle` extended with `letter_spacing`, `word_spacing`, `decoration` (`TextDecoration { underline, overline, line_through }`), `decoration_style` (`TextDecorationStyle::{Solid, Double, Dotted, Dashed, Wavy}`), `decoration_color`, `decoration_thickness`, `shadows: Vec<TextShadow>`, `baseline_shift`.
  - `RichTextSpan { text, style }` carries per-span style overrides.
  - `NativeTextEngine::layout_rich_text(spans, base_style, max_width)` builds a paragraph by pushing each span's style and pop-ing after each span.
  - `NativeTextLayout::line_metrics()` returns `Vec<NativeLineMetrics>` with per-line ascent/descent/height/baseline/left/width and the byte range `start_index..end_index`.
  - `NativeTextLayout::get_rects_for_range(range)` returns the paragraph's bounding rects for a byte range, suitable for selection rendering and baseline-shift overlay placement.
  - 10 new contract tests cover: per-span color rendering, letter spacing widens layout, word spacing widens multi-word, underline decoration changes ink, drop shadow adds offset coverage, baseline shift moves a span vertically, rects-for-range returns valid glyph bounds, single-character rects fit inside the full-range rect, line_metrics matches line_count and surfaces ascent, variable font weight (350) passes through layout.
- **Chunk 8A (Task 12 -- p-s-c adapter contract test): complete** on the same branch. Chunk 8 was sub-split per reviewer guidance into 8A (adapter test), 8B (docs and final verify), 8C (downstream Studio proof in `desktop-app`).
  - `tests/native_studio_renderer_adapter.rs` defines a local `RendererAdapter` mirroring TS `DrawBackend` method names and renders a representative frame end-to-end through `phyron_skia_canvas::native` only.
  - The frame exercises every required surface area: solid background, SVG path with fill rule, linear gradient via shader, raw RGBA8 image (no PNG/JPEG/WebP round trip), clip-path mask, image filter inside `save_layer`, paragraph layout with a registered font, and offscreen composition with `BlendMode::PlusLighter`.
  - 2 tests pass: `adapter_renders_full_frame_through_native_facade_only` (full-frame readback) and `adapter_uses_only_native_namespace` (compile-time pin against future Skia escape hatches).
  - Audit `rg -n "use skia_safe" tests/native_studio_renderer_adapter.rs` returns only doc-comment hits referring to the audit itself; the test imports nothing outside `phyron_skia_canvas::native`. Audit `rg -n 'from_encoded\|png_encoder\|jpeg_encoder\|webp_encoder' tests/native_studio_renderer_adapter.rs` is similarly clean for the hot path.
- **Chunk 8B (Tasks 14-15 -- docs and final verification): complete** on the same branch.
  - README "Rust Library Consumers" section updated to current API: `NativeBackend` + `NativeSurface` + `NativePaint` (the original `NativeRecorder`/`ShapePaint` example is replaced).
  - New `docs/api/native-rust.md` consolidates the consumer-facing reference: stability commitment, working vs export color spaces, premultiplied alpha conventions, pixel formats and depths, surface/recorder/canvas, paint, paths, shaders, filters, images, text, fonts, errors, and verification commands.
  - All gates green on Linux with feature subset `vulkan,window,freetype`:
    - `just fmt-check` passes.
    - `just check` passes.
    - `just lint-check` passes (zero warnings).
    - `cargo test --features "vulkan,window,freetype" --test native_api_contract` -- 5 pass.
    - `cargo test --features "vulkan,window,freetype" --test native_studio_renderer_contract` -- 83 pass.
    - `cargo test --features "vulkan,window,freetype" --test native_studio_renderer_adapter` -- 2 pass.
    - Total: 90 contract tests passing.
  - Audits clean: `pub` signatures in `src/native` carry no `skia_safe`/Neon/`RefCell` types; `src/native` and tests carry no `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!`; the adapter test imports nothing outside `phyron_skia_canvas::native` plus `anyhow`/`std`.
- **Chunk 8C (Task 13 -- downstream Studio proof in `desktop-app`): not started** -- replace direct `skia-safe` dependency with `phyron-skia-canvas`, run `studio-render-native` parity checks. Different repo, separate PR.
- Per reviewer feedback, tests for later chunks land alongside their implementation chunks to keep every commit green.

The first plan delivered a minimum Rust facade:

- `NativeRecorder`.
- Basic `NativeCanvas`.
- Linear sRGB/Display P3/Rec.2020 working spaces.
- Encoded image decode.
- Basic shape/text drawing.
- Raw frame readback.

That is not enough for the required Studio renderer surface. This plan uses required usage from the TypeScript renderer package, not current `studio-render-native` usage.

Required usage sources audited:

- `/home/moritz/code/typescript/studio/packages/renderer/src/backend/types.ts`.
- `/home/moritz/code/typescript/studio/packages/renderer/src/backend/skia-canvas/*.ts`.
- `/home/moritz/code/typescript/studio/packages/renderer/src/render/frameRenderer.ts`.
- `/home/moritz/code/typescript/studio/packages/renderer/src/render/items/*.ts`.
- `/home/moritz/code/typescript/studio/packages/renderer/src/render/renderHelpers.ts`.
- `/home/moritz/code/typescript/studio/packages/renderer/src/render/pixelExport.ts`.
- `/home/moritz/code/typescript/studio/packages/renderer/src/effects/*.ts`.

Before implementing, read:

- `AGENTS.md`.
- `.blueprints/base/AGENTS.md`.
- `.blueprints/base/api-changes.md`.
- `.blueprints/base/test-ownership.md`.
- `.blueprints/lang/rust/AGENTS.md`.
- `.blueprints/lang/rust/testing.md`.

Do not edit `.blueprints`.

## Current Gap Assessment

### Gap Matrix

| Renderer requirement from `packages/renderer` | Current `native` facade | Required action |
| --- | --- | --- |
| Long-lived offscreen surfaces with `getCanvas()`, `snapshot()`, `createOffscreen()`, `flush()`, `dispose()`. | No. `NativeRecorder` records a page and renders at the end. | Add `NativeSurface` owning a private Skia surface and exposing a borrowed `NativeCanvas`. |
| Read/write pixel data during rendering for Citra corner pin, liquid resize, resample, ID buffer, and export. | Partial. Only final `render_raw()` readback exists. | Add `read_pixels()`, `read_pixels_as()`, `read_pixels_linear()`, `write_pixels()`, and `write_pixels_linear()`. |
| Pixel export spaces: `srgb`, `srgb-linear`, `display-p3`, `display-p3-linear`, `rec2020`, `rec2020-linear`, with `uint8`, `f16`, and `f32`. | Partial. `OutputColorSpace` has only three names and does not distinguish gamma-coded vs linear exports. | Add a strict `PixelColorSpace`/`PixelDepth` export API. Preserve internal linear working space. |
| Studio internal alpha is premultiplied. `putImageData` bridge may request unpremultiplied RGBA8. | Partial. `RgbaLinear` is documented as premultiplied, but the paint helper unpremultiplies before Skia paint. | Add explicit alpha-mode tests and document that only readback/write boundaries may convert alpha mode. |
| Working spaces are linear-light sRGB, linear-light Display P3, and linear-light Rec.2020. Values above `1.0` are valid. | Mostly present. Needs wider tests across surfaces, gradients, read/write, and image creation. | Add HDR/out-of-gamut tests for all three working spaces. Default remains linear sRGB. |
| Canvas state: `save`, `restore`, `saveLayer`, translate, rotate, scale, clipping. | Partial. No `scale`, no `save_layer`, no clipping in Rust facade. | Add `scale`, `concat_transform`, `clip_rect`, `clip_rrect`, `clip_path`, and `save_layer`. |
| Drawing: rect, round rect, oval, line, SVG path with fill rules, image src/dst, surface compositing. | Partial. No line, path, source-rect image draw, or surface draw. | Add `NativePath`, `draw_line`, `draw_path`, `draw_image_src`, `draw_surface`. |
| Paint state: alpha, blend mode, style, stroke width/cap, dash pattern, anti-alias, shader, image filter, color filter. | Partial. `ShapePaint` only covers fill/stroke basics. | Add `NativePaint` and keep `ShapePaint` only as a convenience wrapper if useful. |
| Blend modes: Canvas-compatible Porter-Duff and artistic modes including `plus-lighter`. | No. | Add a strict `BlendMode` enum and conversion to Skia blend modes. |
| Filters: blur, drop shadow, color matrix, luma color filter, gamma filters, compose, color-filter-as-image-filter. | No Rust facade. Existing JS binding has the internals. | Add `NativeImageFilter`, `NativeColorFilter`, and factory methods. |
| Shaders: linear gradients with `srgb`/`oklch` interpolation. | No Rust facade. Existing JS binding has gradient interpolation extensions. | Add `NativeShader::linear_gradient()`. |
| Images: encoded image decode, raw RGBA/F16/F32 image creation, dimensions, draw with nearest/linear/mipmap sampling. | Partial. Encoded decode only. | Add `NativeImage::from_pixels()` and sampling options. |
| Browser/server video frame path equivalent: decoded frame bytes become a drawable image without PNG re-encode. | No. | Use `NativeImage::from_pixels()` for rsmpeg decoded frames. |
| Text: paragraph layout, rich spans, font fallback, font registration, line metrics, rects-for-range, decorations, shadows, letter/word spacing, variable weights. | Very partial. `draw_text_box()` covers only simple text. | Add paragraph/text engine facade and font manager facade. |
| SVG path/image support. | Partial/unclear. `NativeImage::from_encoded()` may not cover SVG the same way the JS backend does. | Add contract tests for SVG XML bytes and `NativePath::from_svg()`. Add explicit API if needed. |
| ID buffer and mask stacks. | Blocked by missing surface, saveLayer, color filters, luma filter, blend modes, and raw readback. | Covered by the surface, filter, blend, and readback chunks. |
| Citra post-processing integration. | Blocked by missing linear F32 read/write and raw image creation. | Covered by pixel IO and image-from-pixels chunks. |
| Server/CLI encoded output. | Raw frame exists, but no PNG/JPEG Rust facade equivalent to `DrawBackend.toBuffer()`. | Add encoded output after raw parity, or document that encoding is a downstream concern. |

### Binding Quality Findings

- The existing Node binding is broad and useful. It already contains Path2D, image filters, color filters, paragraph layout, font library, image data, clipping, sampling, and Canvas2D state internals. The right work is exposing these through a Rust facade, not reimplementing them in Studio.
- The current Rust facade is intentionally minimal. It proves the API boundary, but `NativeRecorder` is the wrong primitive for full renderer parity because Studio needs offscreen surfaces, intermediate snapshots, read/write pixels, mask surfaces, and post-processing during a frame.
- `ShapePaint` is too narrow for Studio. The TS renderer treats paint as a mutable style accumulator with blend/filter/shader/stroke/dash/AA state.
- The text facade is far below Studio requirements. The TS renderer depends on paragraph layout, rich spans, baseline-shift overlays, line metrics, rects-for-range, font registration, variable/static weight resolution, shadows, decorations, letter spacing, word spacing, and wrapping/alignment.
- Global/shared state needs a Rust-facing policy. If the facade exposes shared font/filter/resource registries, use `parking_lot::Mutex` or owned managers. Do not expose `RefCell`-based globals to Rust consumers.
- The public Rust facade must continue to hide `skia_safe` and Neon. Existing JS/Neon modules may continue exposing their historical API.

## Non-Goals

- Do not remove the existing JS/Neon API.
- Do not rewrite Studio's TypeScript renderer in this crate.
- Do not regenerate visual baselines.
- Do not run `cargo clean`.
- Do not add GPU shared-buffer transport here.
- Do not implement FFmpeg decoding here.
- Do not add a SharedArrayBuffer transport dependency.

## Target Public Rust Shape

The final Studio-facing shape should be conceptually close to this:

```rust
use phyron_skia_canvas::native::{
    BlendMode, LinearColorSpace, NativeBackend, NativeImage, NativePaint, PixelColorSpace,
    PixelDepth, PixelExportOptions, Point, Rect, RgbaLinear, SamplingMode, SurfaceOptions,
};

let backend = NativeBackend::new();
let mut surface = backend.create_surface(
    1920,
    1080,
    SurfaceOptions {
        color_space: LinearColorSpace::Srgb,
        ..SurfaceOptions::default()
    },
)?;

let mut paint = NativePaint::default();
paint.set_color(RgbaLinear::opaque(1.0, 0.0, 0.0));
paint.set_blend_mode(BlendMode::SourceOver);

surface.with_canvas(|canvas| {
    canvas.clear(RgbaLinear::new_premultiplied(0.0, 0.0, 0.0, 0.0));
    canvas.save();
    canvas.translate(Point::new(100.0, 100.0));
    canvas.draw_rect(Rect::from_xywh(0.0, 0.0, 200.0, 100.0), &paint);
    canvas.restore();
});

let pixels = surface.read_pixels_as(PixelExportOptions {
    color_space: PixelColorSpace::Srgb,
    depth: PixelDepth::Uint8,
    premultiplied: false,
})?;
```

Names may change during implementation, but the contract must cover the required capabilities above.

## File Structure

Expected new or expanded files:

- `src/native/backend.rs` -- `NativeBackend`, global resource policy, construction helpers.
- `src/native/surface.rs` -- `NativeSurface`, snapshot, offscreen, read/write pixels.
- `src/native/canvas.rs` -- expanded `NativeCanvas`.
- `src/native/paint.rs` -- `NativePaint`, style, stroke, blend, filters, shaders.
- `src/native/filter.rs` -- `NativeImageFilter`, `NativeColorFilter`, filter factories.
- `src/native/shader.rs` -- `NativeShader`, gradients.
- `src/native/path.rs` -- `NativePath`, SVG path parsing, fill rule.
- `src/native/image.rs` -- encoded and raw-pixel image constructors, sampling support.
- `src/native/text.rs` -- text style, paragraph layout, rich spans, paragraph metrics.
- `src/native/font.rs` -- Rust-facing font manager using owned state or `parking_lot::Mutex`.
- `src/native/pixels.rs` -- pixel export/read/write options and strict color-space/depth enums.
- `tests/native_studio_renderer_contract.rs` -- Rust contract tests matching `packages/renderer`.

Expected modified files:

- `src/native/mod.rs` -- public exports.
- `src/native/color.rs` -- stricter working/export color-space split.
- `src/native/error.rs` -- new typed errors.
- `src/lib.rs` -- keep only an export note if needed.
- `README.md` or `docs/api/*.md` -- document the Rust consumer facade.

Do not move or delete the existing Neon modules unless a separate cleanup is approved.

## Chunk 1: Contract Tests And API Inventory

### Task 1: Add renderer-surface contract tests.

**Files:**

- Create: `tests/native_studio_renderer_contract.rs`.

Steps:

- [x] Add a test that creates a `NativeSurface`, clears it, draws a rect, snapshots it, draws the snapshot to another surface, and reads tight RGBA8 pixels.
- [x] Add a test that creates same-config offscreen surfaces and composites them with `BlendMode::SourceOver`.
- [x] Add a test that `read_pixels_as()` supports `srgb`, `srgb-linear`, `display-p3`, `display-p3-linear`, `rec2020`, and `rec2020-linear` where Skia supports them, with typed errors for unsupported combinations.
- [x] Add a test that `read_pixels_linear()` returns F32 linear data and `write_pixels_linear()` writes it back without changing dimensions.
- [x] Add a test that all internal linear working spaces accept HDR values above `1.0` without clamping before export.
- [x] Add a test that premultiplied alpha is preserved internally and only converts when explicitly reading/writing unpremultiplied pixels.

Run:

```bash
cargo test --features "vulkan,window,freetype" --test native_studio_renderer_contract
```

Expected result: tests fail to compile until Chunks 2-5 add the API. Do not use `--all-features` on Linux; `metal` enables Apple framework-linked crates.

### Task 2: Add public leak audits.

**Files:**

- Create or extend: `tests/native_api_contract.rs`.

Steps:

- [x] Add an audit command note in the test comments for `rg -n "pub .*skia_safe|pub .*FunctionContext|pub .*JsBox|pub .*Handle<|pub .*RefCell" src/native`.
- [x] Add a Rust test that imports only `phyron_skia_canvas::native::*` and exercises the new public types.
- [x] Confirm no public `native` signatures expose `skia_safe`, Neon, `RefCell`, or JS types.

## Chunk 2: Surface And Pixel IO

### Task 3: Add `NativeBackend` and `NativeSurface`.

**Files:**

- Create: `src/native/backend.rs`.
- Create: `src/native/surface.rs`.
- Modify: `src/native/mod.rs`.
- Modify: `src/native/error.rs`.

Steps:

- [x] Add `NativeBackend::new()` and `create_surface(width, height, SurfaceOptions) -> Result<NativeSurface, NativeError>`.
- [x] Make `NativeSurface` own the private Skia surface or equivalent p-s-c page/surface object.
- [x] Add `NativeSurface::width()`, `height()`, `flush()`, `snapshot()`, and `create_offscreen()`.
- [x] Add `NativeSurface::with_canvas(|canvas| ...)` to borrow a `NativeCanvas` without exposing `skia_safe`.
- [x] Keep `NativeRecorder` working as an existing convenience API. Do not make the Studio renderer depend on it.

### Task 4: Add pixel export/import parity.

**Files:**

- Modify: `src/native/pixels.rs`.
- Modify: `src/native/surface.rs`.
- Modify: `src/native/color.rs`.

Steps:

- [x] Add `PixelColorSpace::{Srgb, SrgbLinear, DisplayP3, DisplayP3Linear, Rec2020, Rec2020Linear}`.
- [x] Add `PixelDepth::{Uint8, F16, F32}`.
- [x] Add `PixelExportOptions { color_space, depth, premultiplied }`.
- [x] Add `ExportedPixels` or extend `RawFrame` so callers can inspect color space, depth, alpha mode, stride, width, height, and bytes.
- [x] Add `NativeSurface::read_pixels()`, `read_pixels_raw()`, `read_pixels_as()`, `read_pixels_linear()`, `write_pixels()`, and `write_pixels_linear()`.
- [x] Return typed `NativeError` variants for unsupported color-space/depth/alpha combinations. Do not silently fall back to sRGB or RGBA8.

Tests:

- [x] Surface readback returns tight rows unless a caller explicitly asks for padded rows.
- [x] F16 and F32 exports preserve values above `1.0` in linear spaces.
- [x] Unpremultiplied RGBA8 readback round-trips for the Tauri `putImageData` bridge.
- [x] Premultiplied F16/F32 read/write round-trips for Studio internal use.

## Chunk 3: Canvas, Paint, Blend, Path, And Image Drawing

### Task 5: Replace `ShapePaint`-only drawing with `NativePaint`.

**Files:**

- Modify: `src/native/paint.rs`.
- Modify: `src/native/canvas.rs` or `src/native/recorder.rs` if `NativeCanvas` stays there.

Steps:

- [x] Add `NativePaint` with color, alpha, style, stroke width, stroke cap, dash pattern, anti-alias, blend mode, shader, image filter, and color filter fields.
- [x] Add `PaintStyle::{Fill, Stroke}`.
- [x] Add `StrokeCap::{Butt, Round, Square}`.
- [x] Add `BlendMode` covering the full TypeScript `BlendMode` union from `packages/renderer/src/backend/types.ts`.
- [x] Keep `ShapePaint` as a convenience wrapper only if it can delegate to `NativePaint`.
- [x] Add `clone`/copy semantics that do not share mutable internal state unsafely.

### Task 6: Expand `NativeCanvas`.

**Files:**

- Create: `src/native/canvas.rs`, or expand `src/native/recorder.rs` and split later.
- Create: `src/native/path.rs`.
- Modify: `src/native/image.rs`.

Steps:

- [x] Add `save_layer(paint: Option<&NativePaint>)`.
- [x] Add `scale(sx, sy)`.
- [x] Add `concat_transform()` with an affine matrix options type. This covers future transforms without exposing Skia matrices.
- [x] Add `clip_rect()`, `clip_rrect()`, and `clip_path()`.
- [ ] Add `draw_line()`.
- [x] Add `NativePath::from_svg(svg_path_data, fill_rule)`.
- [ ] Add `draw_path(path, paint)`.
- [ ] Add `draw_image(image, dst, paint, sampling)`.
- [ ] Add `draw_image_src(image, src, dst, paint, sampling)`.
- [x] Add `draw_surface(surface, x, y, paint)`.
- [ ] Add `SamplingMode::{Nearest, Linear, Mipmapped}`.

Tests:

- [x] Path fill rules render different pixels for `nonzero` vs `evenodd`.
- [x] Rounded clipping masks image corners.
- [x] `save_layer()` applies opacity/filter/blend only to the isolated layer.
- [ ] `plus-lighter` accumulation does not clamp on F16/F32 surfaces.
- [ ] `draw_image_src()` crops the expected source rect.
- [ ] `draw_image(..., SamplingMode::Nearest)` keeps hard edges for preprocessed Citra output.

## Chunk 4: Filters, Color Filters, And Shaders

### Task 7: Add filter facade.

**Files:**

- Create: `src/native/filter.rs`.
- Create: `src/native/shader.rs`.
- Modify: `src/native/mod.rs`.
- Modify: `src/native/paint.rs`.

Steps:

- [x] Add `NativeImageFilter`.
- [x] Add `NativeColorFilter`.
- [x] Add `NativeImageFilter::blur(sigma_x, sigma_y, input)`.
- [x] Add `NativeImageFilter::drop_shadow(dx, dy, sigma_x, sigma_y, color, input)`.
- [x] Add `NativeImageFilter::color_matrix(matrix_4x5, input)`.
- [x] Add `NativeColorFilter::luma()`.
- [x] Add `NativeColorFilter::srgb_to_linear_gamma()`.
- [x] Add `NativeColorFilter::linear_to_srgb_gamma()`.
- [x] Add `NativeColorFilter::compose(outer, inner)`.
- [x] Add `NativeImageFilter::from_color_filter(color_filter, input)`.
- [x] Add `NativeImageFilter::compose(outer, inner)`.
- [x] Add `NativeShader::linear_gradient(start, end, stops, interpolation)`.

Tests:

- [x] Blur expands/softens non-transparent pixels.
- [x] Drop shadow produces offset pixels.
- [x] Color matrix can replace RGB for ID-buffer rendering while thresholding alpha.
- [x] Luma color filter works with `destination-in`/`destination-out` mask paths.
- [x] Linear gradient renders sorted color stops.
- [x] `oklch` interpolation is either implemented or rejected with a typed error. Do not silently use sRGB if the caller requested `oklch`.

## Chunk 5: Images, SVG, And Raw Decoded Frames

### Task 8: Add raw image creation.

**Files:**

- Modify: `src/native/image.rs`.
- Modify: `src/native/pixels.rs`.
- Modify: `src/native/error.rs`.

Steps:

- [x] Add `NativeImage::from_pixels(bytes, width, height, stride, pixel_format, color_space)`.
- [x] Add a borrowed-slice constructor if it is safe; otherwise document the copy and keep owned data.
- [x] Validate stride, dimensions, byte length, alpha mode, and color space.
- [x] Support RGBA8 premul/unpremul, RGBA16F premul, and RGBA32F premul.
- [x] Use this as the intended bridge for rsmpeg decoded video frames and Citra-generated images.

Tests:

- [x] RGBA8 raw image draws without PNG/JPEG re-encode.
- [x] F16/F32 raw image preserves HDR values until SDR export.
- [x] Invalid stride returns a typed error.
- [x] Alpha mode is explicit and tested.

### Task 9: Pin SVG behavior.

**Files:**

- Modify: `src/native/image.rs`.
- Modify: `src/native/path.rs`.
- Create fixtures if needed under `tests/fixtures/`.

Steps:

- [x] Add a contract test for `NativePath::from_svg()` using the same path-data form as `ShapeItem.pathData`.
- [x] Add a contract test for loading SVG XML bytes through `NativeImage::from_encoded()`.
- [x] If encoded SVG image decode does not match the JS backend, add a named `NativeImage::from_svg_xml(svg, width, height)` API instead of relying on ambiguous codec behavior.

## Chunk 6: Text And Fonts

### Task 10: Add Rust-facing font manager.

**Files:**

- Create: `src/native/font.rs`.
- Modify: `src/native/text.rs`.
- Modify: `src/native/mod.rs`.

Steps:

- [x] Add `NativeFontManager` or `NativeFontLibrary`.
- [x] Support `register_font_from_path(family, paths)`.
- [x] Support `register_font_from_data(family, bytes)`.
- [x] Support `has_font(family)` and `families()`.
- [x] Use owned state where possible. If shared global state is required, wrap it with `parking_lot::Mutex`, not `RefCell`.
- [x] Preserve existing JS `FontLibrary` behavior.

### Task 11: Add paragraph layout facade.

**Files:**

- Modify: `src/native/text.rs`.

Steps:

- [x] Add `TextStyle` with font family list, font size, font weight, slant, linear color, align, line-height multiplier, letter spacing, word spacing, OpenType features, variation settings, decoration, shadows, and baseline shift.
- [x] Add `ParagraphStyle`.
- [x] Add `RichTextSpan`.
- [x] Add `NativeTextEngine::layout_text(text, style, max_width)`.
- [x] Add `NativeTextEngine::layout_rich_text(spans, base_style, max_width)`.
- [x] Add `NativeTextLayout` with width, height, line count, first-line ascent, line metrics, and rects-for-range.
- [x] Add `NativeCanvas::draw_text_layout(layout, x, y)`.
- [x] Keep `draw_text_box()` as a convenience API if still useful, but do not make Studio rely on it.

Tests:

- [x] Simple text draws visible pixels.
- [x] Center/right alignment uses max width.
- [x] Wrapping changes line count and height.
- [x] Rich spans preserve per-span color, weight, letter spacing, and baseline shift.
- [x] `get_rects_for_range()` supports baseline-shift overlay placement.
- [x] Registered fonts are used before generic fallbacks.
- [x] Variable font weight can pass through as a non-integer value when supported.

## Chunk 7: Studio Renderer Contract Adapter Proof

### Task 12: Add a local Rust adapter test in p-s-c.

**Files:**

- Create: `tests/native_studio_renderer_adapter.rs`.

Steps:

- [x] Write a small Rust-only adapter that mirrors the TypeScript `DrawBackend` contract names locally inside the test.
- [x] Render a shape, image, text, mask, filter, and offscreen composition through that adapter.
- [x] Assert the adapter never imports `skia_safe`.
- [x] Assert no PNG/JPEG/WebP encode is used in the hot path.

This test is intentionally in p-s-c so API gaps are caught before Studio tries to migrate.

### Task 13: Add downstream Studio proof after p-s-c lands.

**Files in Studio desktop worktree:**

- `/home/moritz/.config/superpowers/worktrees/studio/desktop-app/rust/crates/studio-render-native/Cargo.toml`.
- `/home/moritz/.config/superpowers/worktrees/studio/desktop-app/rust/crates/studio-render-native/src/**/*.rs`.

Steps:

- [ ] Replace direct `skia-safe` dependency with `phyron-skia-canvas`.
- [ ] Remove direct `skia_safe` imports from `studio-render-native`.
- [ ] Implement Studio renderer calls using the new native facade.
- [ ] Keep color-space identifiers aligned with the v3 contract: default linear sRGB, plus linear Display P3 and linear Rec.2020.
- [ ] Keep internal alpha premultiplied.
- [ ] Use `NativeImage::from_pixels()` for rsmpeg decoded frames.
- [ ] Run the existing Studio preview parity suite with `native-video`/`native-video-rsmpeg`.

Verification in Studio:

```bash
cargo test -p studio-render-native --features native-video-rsmpeg
cargo clippy -p studio-render-native --all-targets --features native-video-rsmpeg -- -D warnings
```

## Chunk 8: Documentation And Quality Gates

### Task 14: Document the Rust API.

**Files:**

- Modify: `README.md`.
- Modify or add: `docs/api/native-rust.md`.
- Modify: `docs/superpowers/plans/2026-05-01-native-rust-api-boundary.md`.

Steps:

- [x] Document that `phyron_skia_canvas::native` is the only supported Rust consumer API.
- [x] Document that `skia_safe` remains a private implementation detail.
- [x] Document linear working spaces clearly: `Srgb`, `DisplayP3`, and `Rec2020` are all linear-light spaces with their own primaries. They are not aliases for linear sRGB.
- [x] Document that HDR channel values above `1.0` are valid internally.
- [x] Document premultiplied alpha as the internal Studio convention.
- [x] Document which readback modes return unpremultiplied pixels for external APIs like `putImageData`.

### Task 15: Final verification.

Run:

```bash
just fmt-check
just check
just lint-check
cargo test --features "vulkan,window,freetype" --test native_api_contract
cargo test --features "vulkan,window,freetype" --test native_studio_renderer_contract
cargo test --features "vulkan,window,freetype" --test native_studio_renderer_adapter
```

If `lib/skia.node` exists or a build is acceptable in the current environment, also run:

```bash
just test
```

Do not run visual baseline update commands.

## Recommended Execution Order

1. Land Chunks 1-2 first. Without a real `NativeSurface` and pixel IO, the rest cannot prove Studio parity.
2. Land Chunks 3-4 next. This unlocks masks, effects, blend modes, gradients, ID buffer, and shape parity.
3. Land Chunk 5 before rsmpeg integration. Direct decoded-frame upload is the practical reason to avoid shelling out/PNG round-trips.
4. Land Chunk 6 before claiming editor preview parity for text-heavy projects.
5. Land Chunk 7 as the migration go/no-go. If the adapter cannot be written without `skia_safe`, the facade is still incomplete.
6. Only then switch `studio-render-native` from `skia-safe` to `phyron-skia-canvas`.

## Decision

Desktop-app should not switch from `skia-safe` to `phyron-skia-canvas` yet.

The current p-s-c native facade is good as a boundary proof, but it does not cover the required `@phyron/studio-renderer` surface. Switching now would either lose renderer behavior or force Studio to keep private `skia_safe` escape hatches. The correct next work is to close the p-s-c native API gaps above, then migrate Studio in one bounded downstream proof.
