use skia_safe::{
    FontMgr, FontStyle, Paint as SkPaint,
    font_style::{Slant, Weight, Width},
    textlayout::{
        FontCollection, Paragraph as SkParagraph, ParagraphBuilder as SkParagraphBuilder,
        ParagraphStyle as SkParagraphStyle, TextAlign as SkTextAlign, TextStyle as SkTextStyle,
    },
};

use crate::native::color::RgbaLinear;
use crate::native::font::NativeFontManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerticalAlign {
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextSlant {
    #[default]
    Upright,
    Italic,
    Oblique,
}

impl TextSlant {
    fn to_skia(self) -> Slant {
        match self {
            Self::Upright => Slant::Upright,
            Self::Italic => Slant::Italic,
            Self::Oblique => Slant::Oblique,
        }
    }
}

/// Plain-text paragraph style: font selection, size, weight, slant,
/// color, alignment, and line-height multiplier. Rich spans, decorations,
/// shadows, letter/word spacing, and baseline shifts land in Chunk 7C.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    pub font_families: Vec<String>,
    pub font_size: f32,
    pub font_weight: i32,
    pub slant: TextSlant,
    pub color: RgbaLinear,
    pub align: TextAlign,
    /// Multiplier applied to the font's natural line height. `1.0` keeps
    /// Skia's default. Values above `1.0` add line spacing.
    pub line_height_multiplier: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_families: Vec::new(),
            font_size: 16.0,
            font_weight: 400,
            slant: TextSlant::Upright,
            color: RgbaLinear::opaque(0.0, 0.0, 0.0),
            align: TextAlign::Left,
            line_height_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TextBoxOptions {
    pub color: RgbaLinear,
    pub font_family: Option<String>,
    pub font_size: f32,
    pub font_weight: i32,
    pub horizontal_align: TextAlign,
    pub vertical_align: VerticalAlign,
    pub opacity: f32,
}

impl Default for TextBoxOptions {
    fn default() -> Self {
        Self {
            color: RgbaLinear::opaque(0.0, 0.0, 0.0),
            font_family: None,
            font_size: 16.0,
            font_weight: 400,
            horizontal_align: TextAlign::Left,
            vertical_align: VerticalAlign::Top,
            opacity: 1.0,
        }
    }
}

/// Build laid-out text from a `TextStyle` and a maximum line width.
/// Construct with `new(font_manager)` to use a registered font registry,
/// or `with_system_fonts()` for the platform's default fonts only.
pub struct NativeTextEngine {
    pub(crate) collection: FontCollection,
}

impl NativeTextEngine {
    /// Build using `font_manager`'s registered typefaces plus system
    /// fallbacks for unmatched family names.
    pub fn new(font_manager: &NativeFontManager) -> Self {
        let mut collection = FontCollection::new();
        collection.set_default_font_manager(FontMgr::new(), None);
        collection.set_asset_font_manager(Some(font_manager.snapshot_provider().into()));
        Self { collection }
    }

    /// Build using the platform's system fonts only. Useful when no
    /// `NativeFontManager` is needed.
    pub fn with_system_fonts() -> Self {
        let mut collection = FontCollection::new();
        collection.set_default_font_manager(FontMgr::new(), None);
        Self { collection }
    }

    /// Lay out `text` against `style`, wrapping at `max_width`. Returns
    /// a `NativeTextLayout` that can be measured or drawn via
    /// `NativeCanvas::draw_text_layout`.
    pub fn layout_text(&self, text: &str, style: &TextStyle, max_width: f32) -> NativeTextLayout {
        let sk_text_style = build_text_style(style);
        let mut paragraph_style = SkParagraphStyle::new();
        paragraph_style.set_text_align(match style.align {
            TextAlign::Left => SkTextAlign::Left,
            TextAlign::Center => SkTextAlign::Center,
            TextAlign::Right => SkTextAlign::Right,
        });
        paragraph_style.set_text_style(&sk_text_style);

        let mut builder = SkParagraphBuilder::new(&paragraph_style, self.collection.clone());
        builder.add_text(text);
        let mut paragraph = builder.build();
        paragraph.layout(max_width);
        NativeTextLayout {
            paragraph,
            requested_width: max_width,
        }
    }
}

/// Result of `NativeTextEngine::layout_text`. Owns the laid-out
/// paragraph; metrics queries are cheap and `draw_text_layout` paints
/// the same paragraph onto a canvas.
pub struct NativeTextLayout {
    pub(crate) paragraph: SkParagraph,
    requested_width: f32,
}

impl NativeTextLayout {
    /// The `max_width` requested at layout time.
    pub fn width(&self) -> f32 {
        self.requested_width
    }

    /// Total height of the laid-out paragraph after wrapping.
    pub fn height(&self) -> f32 {
        self.paragraph.height()
    }

    /// Number of laid-out lines (0 if the input was empty).
    pub fn line_count(&self) -> usize {
        self.paragraph.line_number()
    }

    /// Distance from the paragraph's top edge to the first line's
    /// baseline ascent. Useful for vertical alignment of text against a
    /// known baseline.
    pub fn first_line_ascent(&self) -> f32 {
        let metrics = self.paragraph.get_line_metrics();
        metrics.first().map(|m| m.ascent as f32).unwrap_or_default()
    }
}

fn build_text_style(style: &TextStyle) -> SkTextStyle {
    let mut sk_style = SkTextStyle::new();

    let mut paint = SkPaint::default();
    let modulated = style.color;
    let unpremul = if modulated.a > 0.0 {
        skia_safe::Color4f {
            r: modulated.r / modulated.a,
            g: modulated.g / modulated.a,
            b: modulated.b / modulated.a,
            a: modulated.a,
        }
    } else {
        skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.0)
    };
    paint.set_color4f(unpremul, None);
    paint.set_anti_alias(true);
    sk_style.set_foreground_paint(&paint);

    sk_style.set_font_size(style.font_size);
    if !style.font_families.is_empty() {
        let families: Vec<&str> = style.font_families.iter().map(String::as_str).collect();
        sk_style.set_font_families(&families);
    }
    sk_style.set_font_style(FontStyle::new(
        Weight::from(style.font_weight),
        Width::NORMAL,
        style.slant.to_skia(),
    ));
    if (style.line_height_multiplier - 1.0).abs() > f32::EPSILON {
        sk_style.set_height(style.line_height_multiplier);
        sk_style.set_height_override(true);
    }

    sk_style
}
