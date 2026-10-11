//! The side sheet (docs/DESIGN.md §3.6): Layers · Colors · Settings, on
//! the right below the top chrome. The map stays interactive beside it.

use crate::design::components::{self, Kit, interact, segmented, toggle};
use crate::design::tokens::{Type, radius, size, space};
use crate::model::{Model, SheetTab};
use crate::pickers;
use eframe::egui::{self, Align2, Id, LayerId, Order, Rect, Ui, WidgetType, pos2, vec2};
use hookecho_core::config::{Speed, ThemePref, Units};

/// Where the sheet goes: right edge, under the top chips, to the bottom margin.
pub fn sheet_rect(screen: Rect, m: f32) -> Rect {
    let w = size::SHEET_W.min(screen.width() - 2.0 * m);
    Rect::from_min_max(
        pos2(
            screen.max.x - m - w,
            screen.min.y + m + size::CHIP_H + space::S2,
        ),
        pos2(screen.max.x - m, screen.max.y - m),
    )
}

/// Lays out rows top to bottom inside the scrolled body.
struct Body<'a> {
    painter: &'a egui::Painter,
    x0: f32,
    x1: f32,
    y: f32,
}

impl Body<'_> {
    fn eyebrow(&mut self, kit: &Kit, text: &str) {
        let h = size::ROW_H * 0.6;
        kit.text(
            self.painter,
            pos2(self.x0 + space::S2, self.y + h / 2.0),
            Align2::LEFT_CENTER,
            text,
            Type::Caption,
            kit.theme.text_tertiary,
        );
        self.y += h;
    }

    fn hint(&mut self, kit: &Kit, text: &str) {
        kit.text(
            self.painter,
            pos2(self.x0 + space::S2, self.y + size::HINT_H / 2.0),
            Align2::LEFT_CENTER,
            text,
            Type::Caption,
            kit.theme.text_tertiary,
        );
        self.y += size::HINT_H;
    }

    fn gap(&mut self) {
        self.y += size::SECTION_GAP;
    }

    fn row(&mut self) -> Rect {
        let r = Rect::from_min_size(
            pos2(self.x0, self.y),
            vec2(self.x1 - self.x0, size::SETTING_ROW_H),
        );
        self.y += size::SETTING_ROW_H;
        r
    }

    /// A row label on the left.
    fn label(&self, kit: &Kit, row: Rect, text: &str) {
        kit.text(
            self.painter,
            pos2(row.min.x + space::S2, row.center().y),
            Align2::LEFT_CENTER,
            text,
            Type::Body,
            kit.theme.text_primary,
        );
    }

    /// A toggle row; returns the new value.
    fn toggle(&mut self, ui: &mut Ui, kit: &Kit, id: &str, text: &str, on: bool) -> bool {
        let row = self.row();
        self.label(kit, row, text);
        on ^ toggle(ui, kit, self.painter, row, Id::new(("sheet", id)), text, on)
    }

    /// A labeled segmented control; returns the index clicked, if any.
    fn choice(
        &mut self,
        ui: &mut Ui,
        kit: &mut Kit,
        id: &str,
        text: &str,
        items: &[&str],
        selected: usize,
    ) -> Option<usize> {
        let row = self.row();
        self.label(kit, row, text);
        let w = size::SETTING_SEGMENT_W * items.len() as f32;
        let r = Rect::from_min_size(
            pos2(
                row.max.x - space::S2 - w,
                row.center().y - size::SETTING_SEGMENT_H / 2.0,
            ),
            vec2(w, size::SETTING_SEGMENT_H),
        );
        segmented(
            ui,
            kit,
            self.painter,
            r,
            Id::new(("sheet", id)),
            items,
            selected,
            Type::Caption,
        )
    }
}

pub fn side_sheet(ui: &mut Ui, kit: &mut Kit, model: &mut Model, rect: Rect) {
    let Some(mut sheet) = model.sheet.clone() else {
        return;
    };
    let layer = LayerId::new(Order::Foreground, Id::new("side-sheet"));
    let painter = ui.ctx().layer_painter(layer);
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    // Swallow clicks and drags on the sheet's background so the map under
    // it doesn't pan.
    ui.interact(rect, Id::new("sheet-bg"), egui::Sense::click_and_drag());
    kit.glass(&painter, rect, radius::LG, 3);

    let pad = space::S3;
    let tabs = Rect::from_min_size(
        rect.min + vec2(pad, pad),
        vec2(rect.width() - 2.0 * pad, size::SEGMENTED_H),
    );
    let labels: Vec<&str> = SheetTab::ALL.iter().map(|t| t.label()).collect();
    let cur = SheetTab::ALL
        .iter()
        .position(|&x| x == sheet.tab)
        .unwrap_or(0);
    if let Some(i) = segmented(
        &mut ui,
        kit,
        &painter,
        tabs,
        Id::new("sheet-tabs"),
        &labels,
        cur,
        Type::Label,
    ) && SheetTab::ALL[i] != sheet.tab
    {
        sheet.tab = SheetTab::ALL[i];
        sheet.scroll = 0.0;
    }

    let area = Rect::from_min_max(
        pos2(rect.min.x, tabs.max.y + space::S2),
        pos2(rect.max.x, rect.max.y - pad),
    );
    if ui.rect_contains_pointer(area) {
        sheet.scroll -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    let clipped = painter.with_clip_rect(area);
    let mut body = Body {
        painter: &clipped,
        x0: rect.min.x + space::S2,
        x1: rect.max.x - space::S2,
        y: area.min.y - sheet.scroll,
    };
    let top = body.y;
    // Widgets scrolled out of view mustn't take clicks.
    let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(area));
    ui.set_clip_rect(area);
    match sheet.tab {
        SheetTab::Layers => layers(&mut ui, kit, model, &mut body),
        SheetTab::Colors => colors(&mut ui, kit, model, &mut body),
        SheetTab::Settings => settings(&mut ui, kit, model, &mut body, &mut sheet),
    }
    let content_h = body.y - top;
    sheet.scroll = sheet
        .scroll
        .clamp(0.0, (content_h - area.height()).max(0.0));
    components::scroll_indicator(kit, &painter, area, sheet.scroll, content_h);
    // A control may have closed the sheet meanwhile.
    if model.sheet.is_some() {
        model.sheet = Some(sheet);
    }
}

fn layers(ui: &mut Ui, kit: &mut Kit, model: &mut Model, b: &mut Body) {
    b.eyebrow(kit, "MAP");
    let map = &mut model.config.map;
    map.counties = b.toggle(ui, kit, "counties", "County lines", map.counties);
    map.roads = b.toggle(ui, kit, "roads", "Highways", map.roads);
    map.labels = b.toggle(ui, kit, "labels", "Place names", map.labels);
    b.gap();
    b.eyebrow(kit, "RADAR");
    let c = &mut model.config;
    c.mask_dualpol = b.toggle(ui, kit, "mask", "Hide dual-pol noise", c.mask_dualpol);
    b.hint(kit, "CC, ZDR and PHI only where there's echo");
    b.gap();
    b.hint(kit, "Warnings, outlooks and lightning come in a");
    b.hint(kit, "later update.");
}

fn colors(ui: &mut Ui, kit: &mut Kit, model: &mut Model, b: &mut Body) {
    let title = format!("{} COLORS", model.moment.short_name().to_uppercase());
    b.eyebrow(kit, &title);
    let mut y = b.y;
    pickers::palette_rows(ui, kit, b.painter, model, b.x0, b.x1, &mut y);
    b.y = y + space::S2;
    b.hint(kit, "Drop a GRLevelX .pal file anywhere on the");
    b.hint(kit, "window to add your own.");
    b.gap();
    let row = b.row();
    if text_button(ui, kit, b.painter, row, "restore", "Restore default colors") {
        model.palettes.restore_defaults(&mut model.config);
    }
}

fn settings(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    b: &mut Body,
    sheet: &mut crate::model::Sheet,
) {
    let c = &mut model.config;
    b.eyebrow(kit, "APPEARANCE");
    let themes = [ThemePref::Dark, ThemePref::Light, ThemePref::System];
    let i = themes.iter().position(|&x| x == c.theme).unwrap_or(0);
    if let Some(i) = b.choice(ui, kit, "theme", "Theme", &["Dark", "Light", "System"], i) {
        c.theme = themes[i];
    }
    let scales = [0.85, 1.0, 1.15, 1.3];
    let i = scales
        .iter()
        .position(|&x| (x - c.label_scale).abs() < 0.01)
        .unwrap_or(usize::MAX);
    if let Some(i) = b.choice(
        ui,
        kit,
        "label-scale",
        "Map text",
        &["S", "M", "L", "XL"],
        i,
    ) {
        c.label_scale = scales[i];
    }
    c.reduced_motion = b.toggle(ui, kit, "motion", "Reduce motion", c.reduced_motion);
    model.reduced_motion = c.reduced_motion;
    b.gap();

    b.eyebrow(kit, "UNITS AND TIME");
    let units = [Units::Imperial, Units::Metric];
    let i = units.iter().position(|&x| x == c.units).unwrap_or(0);
    if let Some(i) = b.choice(ui, kit, "units", "Distance", &["mi, ft", "km, m"], i) {
        c.units = units[i];
    }
    let speeds = [Speed::Knots, Speed::Mph, Speed::Kmh, Speed::Ms];
    // Auto shows as whatever it resolves to.
    let shown = match c.speed {
        Speed::Auto => match c.units {
            Units::Imperial => Speed::Knots,
            Units::Metric => Speed::Ms,
        },
        s => s,
    };
    let i = speeds.iter().position(|&x| x == shown).unwrap_or(0);
    if let Some(i) = b.choice(
        ui,
        kit,
        "speed-units",
        "Wind",
        &["kt", "mph", "km/h", "m/s"],
        i,
    ) {
        c.speed = speeds[i];
    }
    if let Some(i) = b.choice(ui, kit, "utc", "Clock", &["Local", "UTC"], c.utc as usize) {
        c.utc = i == 1;
    }
    b.gap();

    b.eyebrow(kit, "LOOP");
    let frames = [10, 30, 50];
    // A value from an older or hand-edited config shows no selection
    // until the user picks one.
    let i = frames
        .iter()
        .position(|&x| x == c.loop_frames)
        .unwrap_or(usize::MAX);
    if let Some(i) = b.choice(ui, kit, "frames", "Frames", &["10", "30", "50"], i) {
        c.loop_frames = frames[i];
    }
    let speeds = [0.5, 1.0, 2.0];
    let i = speeds
        .iter()
        .position(|&x| (x - c.loop_speed).abs() < 0.01)
        .unwrap_or(usize::MAX);
    if let Some(i) = b.choice(ui, kit, "speed", "Speed", &["½×", "1×", "2×"], i) {
        c.loop_speed = speeds[i];
    }
    b.gap();

    b.eyebrow(kit, "DATA");
    let row = b.row();
    let used = sheet.cache_bytes.map_or("…".to_string(), |n| {
        format!("Radar cache: {}", human_bytes(n))
    });
    b.label(kit, row, &used);
    let w = size::SETTING_SEGMENT_W;
    let r = Rect::from_min_size(
        pos2(
            row.max.x - space::S2 - w,
            row.center().y - size::SETTING_SEGMENT_H / 2.0,
        ),
        vec2(w, size::SETTING_SEGMENT_H),
    );
    if text_button(ui, kit, b.painter, r, "clear", "Clear") {
        sheet.clear_cache = true;
    }
    b.hint(kit, "Live radar comes from NOAA via Unidata's open");
    b.hint(kit, "data on AWS. Nothing about you is sent.");
    b.gap();

    b.eyebrow(kit, "ABOUT");
    b.hint(kit, &format!("Hook Echo {}", env!("CARGO_PKG_VERSION")));
    b.hint(kit, "Free software under the GPL, version 3 or later.");
    b.hint(kit, "Maps: US Census Bureau and Natural Earth.");
}

/// A quiet text button (the label centered on a track fill).
fn text_button(
    ui: &mut Ui,
    kit: &mut Kit,
    painter: &egui::Painter,
    rect: Rect,
    id: &str,
    label: &str,
) -> bool {
    let resp = interact(
        ui,
        rect,
        Id::new(("sheet-button", id)),
        WidgetType::Button,
        label,
        None,
    );
    painter.rect_filled(rect, egui::CornerRadius::same(radius::SM), kit.theme.track);
    kit.state_fill(painter, rect, radius::SM, &resp, false);
    kit.text(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        Type::Caption,
        kit.theme.text_primary,
    );
    kit.focus_ring(painter, rect, radius::SM, &resp);
    resp.clicked()
}

pub fn human_bytes(n: u64) -> String {
    let mb = n as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else if mb >= 10.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{mb:.1} MB")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn human_bytes() {
        assert_eq!(super::human_bytes(0), "0.0 MB");
        assert_eq!(super::human_bytes(5 * 1024 * 1024 + 300_000), "5.3 MB");
        assert_eq!(super::human_bytes(512 * 1024 * 1024), "512 MB");
        assert_eq!(super::human_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
