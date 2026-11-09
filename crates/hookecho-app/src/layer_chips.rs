//! Active-layer chips (owner, testing: "it should be very easy for the
//! user to get rid of"): one small chip above the legend for each overlay
//! that's on (satellite, SPC outlook, aurora, station plots). The × (or a
//! right-click anywhere on the chip) turns it off; the name opens Layers.

use crate::design::components::{self, Kit, icon_button};
use crate::design::icons::Icon;
use crate::design::tokens::{Type, radius, size, space};
use crate::model::{Model, Sheet, SheetTab};
use eframe::egui::{Align2, Id, Painter, Rect, Ui, WidgetType, pos2, vec2};

/// The overlays that are on: (key, label).
pub fn active(model: &Model) -> Vec<(&'static str, String)> {
    let c = &model.config;
    let mut v = Vec::new();
    if let Some(b) = c
        .satellite
        .as_deref()
        .and_then(hookecho_core::goes::Band::from_key)
    {
        v.push((
            "satellite",
            format!("Satellite: {}", b.name().to_lowercase()),
        ));
    }
    if let Some(p) = c
        .outlook
        .as_deref()
        .and_then(hookecho_core::spc::Product::from_key)
    {
        v.push((
            "outlook",
            format!("Outlook day {}: {}", p.day(), p.name().to_lowercase()),
        ));
    }
    if c.aurora {
        v.push(("aurora", "Aurora forecast".to_string()));
    }
    if c.station_plots {
        v.push(("plots", "Station plots".to_string()));
    }
    v
}

/// Turns the overlay `key` off.
pub fn remove(model: &mut Model, key: &str) {
    let c = &mut model.config;
    match key {
        "satellite" => c.satellite = None,
        "outlook" => c.outlook = None,
        "aurora" => c.aurora = false,
        "plots" => c.station_plots = false,
        _ => {}
    }
}

/// Draws the chips right-aligned in a row whose bottom edge is `bottom`,
/// ending at `right`. Returns the row's rect, if any chips were drawn.
pub fn draw(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &Painter,
    right: f32,
    bottom: f32,
    left_limit: f32,
) -> Option<Rect> {
    let chips = active(model);
    if chips.is_empty() {
        return None;
    }
    let t = kit.theme.clone();
    let h = size::LAYER_CHIP_H;
    let mut x = right;
    let y = bottom - h;
    let mut row: Option<Rect> = None;
    let mut removed = None;
    let mut open_sheet = false;
    for (key, label) in chips {
        let g = kit.galley(painter, label.clone(), Type::Caption, t.text_primary);
        let w = space::S3 + g.size().x + space::S1 + size::LAYER_CHIP_X;
        if x - w < left_limit {
            break;
        }
        let r = Rect::from_min_size(pos2(x - w, y), vec2(w, h));
        kit.glass(painter, r, radius::FULL, 1);
        let name = Rect::from_min_max(r.min, pos2(r.max.x - size::LAYER_CHIP_X, r.max.y));
        let resp = components::interact(
            ui,
            name,
            Id::new(("layer-chip", key)),
            WidgetType::Button,
            &label,
            None,
        );
        kit.state_fill(painter, r, radius::FULL, &resp, false);
        kit.tooltip(ui, &resp, "Open Layers · right-click to turn off", None);
        kit.text(
            painter,
            pos2(r.min.x + space::S3, r.center().y),
            Align2::LEFT_CENTER,
            label.clone(),
            Type::Caption,
            t.text_primary,
        );
        let xr = Rect::from_min_size(
            pos2(r.max.x - size::LAYER_CHIP_X, r.min.y),
            vec2(size::LAYER_CHIP_X, h),
        );
        let close = icon_button(
            ui,
            kit,
            painter,
            xr,
            Id::new(("layer-chip-x", key)),
            Icon::X,
            &format!("Turn off {label}"),
            None,
            false,
        );
        if close.clicked() || resp.secondary_clicked() || close.secondary_clicked() {
            removed = Some(key);
        } else if resp.clicked() {
            open_sheet = true;
        }
        row = Some(row.map_or(r, |a| a.union(r)));
        x = r.min.x - space::S2;
    }
    if let Some(k) = removed {
        remove(model, k);
    }
    if open_sheet {
        model.sheet = Some(Sheet::new(SheetTab::Layers));
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_chip_can_be_removed() {
        let site = hookecho_core::sites::by_id("KTLX").unwrap();
        let mut m = Model::new(Default::default(), site, [1280.0, 800.0]);
        assert!(active(&m).is_empty());
        m.config.satellite = Some("infrared".into());
        m.config.outlook = Some("day1-cat".into());
        m.config.aurora = true;
        m.config.station_plots = true;
        let keys: Vec<_> = active(&m).into_iter().map(|c| c.0).collect();
        assert_eq!(keys, ["satellite", "outlook", "aurora", "plots"]);
        for k in keys {
            remove(&mut m, k);
        }
        assert!(active(&m).is_empty());
    }
}
