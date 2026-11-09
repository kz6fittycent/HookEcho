//! The left dock (docs/DESIGN.md §3.11): a column for panels pinned out of
//! the way of the map. Space weather, the station panel, warnings/watches/
//! MDs and the inspector can each float over the map or be docked here; the
//! same header (title, collapse, pin/unpin, close) works both ways. With
//! anything docked, the map and its chrome move right of the column, and
//! the map recenters so nothing jumps. Windows narrower than
//! `size::DOCK_MIN_WINDOW` float every panel instead.

use crate::design::components::{self, Kit, PanelAction, panel_header};
use crate::design::tokens::{radius, size, space};
use crate::model::{Model, Picker};
use eframe::egui::{Id, Rect, Sense, Ui, pos2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Bulletins,
    Station,
    Space,
    Inspector,
}

impl Panel {
    pub const ALL: [Panel; 4] = [
        Panel::Bulletins,
        Panel::Station,
        Panel::Space,
        Panel::Inspector,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Panel::Bulletins => "bulletins",
            Panel::Station => "station",
            Panel::Space => "space",
            Panel::Inspector => "inspector",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Panel::Bulletins => "Warnings and watches",
            Panel::Station => "Weather station",
            Panel::Space => "Space weather",
            Panel::Inspector => "Inspector",
        }
    }

    pub fn from_key(k: &str) -> Option<Panel> {
        Panel::ALL.into_iter().find(|p| p.key() == k)
    }
}

/// Whether `p` is in the dock (and the window is wide enough for one).
pub fn docked(model: &Model, screen: Rect, p: Panel) -> bool {
    rect(model, screen).is_some() && model.config.dock.iter().any(|k| k == p.key())
}

/// Whether `p` is set to dock, whatever the window size.
pub fn wants_dock(model: &Model, p: Panel) -> bool {
    model.config.dock.iter().any(|k| k == p.key())
}

/// The dock column, when anything is docked and the window is wide enough.
pub fn rect(model: &Model, screen: Rect) -> Option<Rect> {
    if model.config.dock.is_empty() || screen.width() < size::DOCK_MIN_WINDOW {
        return None;
    }
    let m = space::S4;
    Some(Rect::from_min_max(
        pos2(screen.min.x + m, screen.min.y + m),
        pos2(screen.min.x + m + size::DOCK_W, screen.max.y - m),
    ))
}

/// Where the map's chrome goes: the window right of the dock.
pub fn map_area(model: &Model, screen: Rect) -> Rect {
    match rect(model, screen) {
        Some(d) => Rect::from_min_max(pos2(d.max.x, screen.min.y), screen.max),
        None => screen,
    }
}

/// Puts `p` at the bottom of the dock, expanded, and closes its floating
/// form.
pub fn dock(model: &mut Model, p: Panel) {
    if !wants_dock(model, p) {
        model.config.dock.push(p.key().to_string());
    }
    model.config.dock_collapsed.retain(|k| k != p.key());
    match p {
        Panel::Space if matches!(model.picker, Picker::SpaceWeather { .. }) => {
            model.picker = Picker::None;
        }
        Panel::Bulletins => {
            if let Picker::Warnings { ids, .. } = &model.picker {
                model.dock_bulletins = ids.clone();
                model.picker = Picker::None;
            }
        }
        Panel::Station => model.config.station_panel = true,
        _ => {}
    }
}

/// Takes `p` out of the dock; it floats again where it does.
pub fn undock(model: &mut Model, p: Panel) {
    model.config.dock.retain(|k| k != p.key());
    model.config.dock_collapsed.retain(|k| k != p.key());
    match p {
        Panel::Space => model.picker = Picker::SpaceWeather { scroll: 0.0 },
        Panel::Bulletins if !model.dock_bulletins.is_empty() => {
            model.picker = Picker::Warnings {
                ids: std::mem::take(&mut model.dock_bulletins),
                scroll: 0.0,
            };
        }
        _ => {}
    }
}

/// Closes `p` entirely: out of the dock, and not floating either.
pub fn close(model: &mut Model, p: Panel) {
    model.config.dock.retain(|k| k != p.key());
    model.config.dock_collapsed.retain(|k| k != p.key());
    match p {
        Panel::Station => model.config.station_panel = false,
        Panel::Bulletins => model.dock_bulletins.clear(),
        _ => {}
    }
}

/// Runs a header button's action for `p`.
pub fn act(model: &mut Model, p: Panel, a: PanelAction) {
    match a {
        PanelAction::Dock => dock(model, p),
        PanelAction::Undock => undock(model, p),
        PanelAction::Close => close(model, p),
        PanelAction::Collapse => {
            let c = &mut model.config.dock_collapsed;
            if c.iter().any(|k| k == p.key()) {
                c.retain(|k| k != p.key());
            } else {
                c.push(p.key().to_string());
            }
        }
    }
}

/// One panel's body, drawn from `y` between `x0` and `x1`; returns its
/// height. The painter is clipped to what's visible.
#[allow(clippy::too_many_arguments)]
fn body(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &eframe::egui::Painter,
    p: Panel,
    x0: f32,
    x1: f32,
    y: f32,
) -> f32 {
    match p {
        Panel::Bulletins => crate::warnings::dock_body(ui, kit, model, painter, x0, x1, y),
        Panel::Station => crate::stations::body(ui, kit, model, painter, x0, x1, y),
        Panel::Space => crate::space::body(kit, model, painter, x0, x1, y),
        Panel::Inspector => crate::screen::inspector_body(kit, model, painter, x0, x1, y),
    }
}

/// Draws the dock: one glass column, each docked panel a card in it, top
/// to bottom; scrolls when they don't fit.
pub fn draw(ui: &mut Ui, kit: &mut Kit, model: &mut Model, screen: Rect) {
    let Some(rect) = rect(model, screen) else {
        model.dock_inset = 0.0;
        return;
    };
    let painter = ui.painter().clone();
    kit.glass(&painter, rect, radius::LG, 1);
    ui.interact(rect, Id::new("dock"), Sense::click_and_drag());
    let pad = space::S3;
    let area = rect.shrink(pad);
    if components::pointer_over(ui, area) {
        model.dock_scroll -= ui.input(|i| i.smooth_scroll_delta.y);
    }
    let clip = painter.with_clip_rect(area);
    let (x0, x1) = (area.min.x, area.max.x);
    let top = area.min.y - model.dock_scroll;
    let mut y = top;
    let panels: Vec<Panel> = model
        .config
        .dock
        .iter()
        .filter_map(|k| Panel::from_key(k))
        .collect();
    let mut ui = ui.new_child(eframe::egui::UiBuilder::new().max_rect(area));
    ui.set_clip_rect(area);
    for (i, p) in panels.into_iter().enumerate() {
        if i > 0 {
            kit.divider(&clip, x0, x1, y + space::S2);
            y += space::S4;
        }
        let collapsed = model.config.dock_collapsed.iter().any(|k| k == p.key());
        let header = Rect::from_min_max(pos2(x0, y), pos2(x1, y + size::PANEL_HEADER_H));
        if let Some(a) = panel_header(
            &mut ui,
            kit,
            &clip,
            header,
            Id::new(("dock", p.key())),
            p.title(),
            true,
            collapsed,
            p != Panel::Inspector,
        ) {
            act(model, p, a);
        }
        y = header.max.y;
        if !collapsed {
            y += body(&mut ui, kit, model, &clip, p, x0, x1, y);
        }
    }
    let content_h = y - top;
    model.dock_scroll = model
        .dock_scroll
        .clamp(0.0, (content_h - area.height()).max(0.0));
    components::scroll_indicator(kit, &painter, area, model.dock_scroll, content_h);
}

/// Keeps the map still as the dock opens or closes: what was in the middle
/// of the visible map stays there.
pub fn settle(model: &mut Model, screen: Rect) {
    let inset = map_area(model, screen).min.x - screen.min.x;
    let d = inset - model.dock_inset;
    if d.abs() > 0.5 {
        model.camera.pan(d as f64 / 2.0, 0.0);
        model.dock_inset = inset;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hookecho_core::config::Config;

    fn model() -> Model {
        let site = hookecho_core::sites::by_id("KTLX").unwrap();
        Model::new(Config::default(), site, [1280.0, 800.0])
    }

    #[test]
    fn dock_undock_close() {
        let mut m = model();
        let wide = Rect::from_min_max(pos2(0.0, 0.0), pos2(1600.0, 900.0));
        assert!(rect(&m, wide).is_none(), "nothing docked, no dock");
        m.picker = Picker::SpaceWeather { scroll: 0.0 };
        dock(&mut m, Panel::Space);
        assert_eq!(m.picker, Picker::None, "the floating card closes");
        assert!(docked(&m, wide, Panel::Space));
        assert_eq!(map_area(&m, wide).min.x, space::S4 + size::DOCK_W);
        let narrow = Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 600.0));
        assert!(!docked(&m, narrow, Panel::Space), "a narrow window floats");
        act(&mut m, Panel::Space, PanelAction::Collapse);
        assert_eq!(m.config.dock_collapsed, ["space"]);
        undock(&mut m, Panel::Space);
        assert!(
            matches!(m.picker, Picker::SpaceWeather { .. }),
            "floats again"
        );
        assert!(m.config.dock.is_empty() && m.config.dock_collapsed.is_empty());
        dock(&mut m, Panel::Station);
        close(&mut m, Panel::Station);
        assert!(!m.config.station_panel && m.config.dock.is_empty());
    }

    #[test]
    fn settle_keeps_the_visible_middle() {
        let mut m = model();
        let wide = Rect::from_min_max(pos2(0.0, 0.0), pos2(1600.0, 900.0));
        let before = m.map_center();
        dock(&mut m, Panel::Station);
        settle(&mut m, wide);
        let after = m.map_center();
        assert!((before.0 - after.0).abs() < 1e-6 && (before.1 - after.1).abs() < 1e-6);
    }
}
