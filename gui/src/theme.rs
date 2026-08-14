//! Central place for the app's visual identity. `apply` installs a cohesive
//! dark theme (palette, spacing, rounding, font sizes) so the rest of the UI
//! can stay focused on layout and pull accent colors from the `pub` consts.

use egui::{Color32, CornerRadius, FontId, Margin, Stroke, TextStyle};

// ---- Palette --------------------------------------------------------------
// A near-black slate base with a single indigo accent. Canvas cards, panels
// and widgets all draw from these so the app reads as one system.

/// Canvas / workspace background.
pub const BG: Color32 = Color32::from_rgb(17, 18, 24);
/// Toolbar / inspector panel fill.
pub const PANEL: Color32 = Color32::from_rgb(24, 25, 33);
/// Interactive surfaces: buttons, combos, cards.
pub const SURFACE: Color32 = Color32::from_rgb(38, 40, 52);
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(50, 52, 66);
/// Hairline borders.
pub const BORDER: Color32 = Color32::from_rgb(58, 60, 76);

/// Bright accent for strokes, selection and identity.
pub const ACCENT: Color32 = Color32::from_rgb(129, 140, 248);
/// Muted accent for filled primary buttons / selected card body.
pub const ACCENT_DIM: Color32 = Color32::from_rgb(67, 71, 152);

pub const TEXT: Color32 = Color32::from_rgb(228, 229, 238);
pub const TEXT_MUTED: Color32 = Color32::from_rgb(150, 153, 170);

pub const OK: Color32 = Color32::from_rgb(74, 201, 155);
pub const WARN: Color32 = Color32::from_rgb(240, 185, 92);
pub const ERR: Color32 = Color32::from_rgb(240, 116, 116);

/// Monitor card fills.
pub const CARD: Color32 = Color32::from_rgb(46, 49, 63);
pub const CARD_SELECTED: Color32 = Color32::from_rgb(52, 56, 92);
pub const CARD_DISABLED: Color32 = Color32::from_rgb(30, 31, 38);

/// Install the theme onto the egui context. Call once at startup.
pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Deliberate spacing rhythm — roomier than egui defaults.
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.window_margin = Margin::same(10);
    style.spacing.menu_margin = Margin::same(8);
    style.spacing.interact_size.y = 28.0;

    // Larger, more legible type scale.
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(20.0)),
        (TextStyle::Body, FontId::proportional(14.5)),
        (TextStyle::Monospace, FontId::monospace(13.0)),
        (TextStyle::Button, FontId::proportional(14.5)),
        (TextStyle::Small, FontId::proportional(11.5)),
    ]
    .into();

    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.faint_bg_color = SURFACE;
    v.extreme_bg_color = BG;
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.window_corner_radius = CornerRadius::same(10);

    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0_f32, ACCENT);

    let radius = CornerRadius::same(6);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    v.widgets.noninteractive.corner_radius = radius;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);

    v.widgets.inactive.bg_fill = SURFACE;
    v.widgets.inactive.weak_bg_fill = SURFACE;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.hovered.bg_fill = SURFACE_HOVER;
    v.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    v.widgets.active.bg_fill = ACCENT_DIM;
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);

    style.visuals = v;
    ctx.set_style(style);
}
