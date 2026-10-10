//! Site and product pickers (docs/DESIGN.md §3.7): glass popovers anchored
//! to their chips, keyboard-driven, built from design components only.

use crate::design::components::{self, Kit, interact, popover_rect, scroll_indicator};
use crate::design::icons::Icon;
use crate::design::tokens::{Type, radius, size, space};
use crate::model::{self, Model, PRODUCTS, Picker, TiltState};
use eframe::egui::{
    self, Align2, CornerRadius, Id, Key, LayerId, Order, Rect, Ui, WidgetType, pos2, vec2,
};

/// Closes the picker if the user clicked outside it.
fn close_on_outside_click(ui: &Ui, model: &mut Model, rect: Rect, anchor: Rect) {
    let clicked_outside = ui.input(|i| {
        i.pointer.any_click()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !rect.contains(p) && !anchor.contains(p))
    });
    if clicked_outside {
        model.picker = Picker::None;
    }
}

pub fn site_picker(ui: &mut Ui, kit: &mut Kit, model: &mut Model, anchor: Rect) {
    let Picker::Site {
        mut query,
        mut selected,
        mut scroll,
    } = model.picker.clone()
    else {
        return;
    };
    let screen = kit.screen;
    let rect = popover_rect(screen, anchor, size::PICKER_W, size::PICKER_MAX_H);
    let layer = LayerId::new(Order::Foreground, Id::new("site-picker"));
    let painter = ui.ctx().layer_painter(layer);
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    kit.glass(&painter, rect, radius::LG, 2);
    let t = kit.theme.clone();
    let pad = space::S3;

    let field = Rect::from_min_size(
        rect.min + vec2(pad, pad),
        vec2(rect.width() - pad * 2.0, size::FIELD_H),
    );
    let field_id = Id::new("site-search");
    // The search field keeps keyboard focus while the picker is open, except
    // on the frame of a click (so clicking a row isn't stolen).
    if !ui.memory(|m| m.has_focus(field_id)) && !ui.input(|i| i.pointer.any_click()) {
        ui.memory_mut(|m| m.request_focus(field_id));
    }
    if components::text_field(
        &mut ui,
        kit,
        &painter,
        field,
        field_id,
        &mut query,
        "Search radars by ID, city or state",
    ) {
        selected = 0;
        scroll = 0.0;
    }

    let list = Rect::from_min_max(
        pos2(rect.min.x + space::S2, field.max.y + space::S2),
        pos2(rect.max.x - space::S2, rect.max.y - space::S2),
    );
    let items = model::site_list(&query, &model.config.favorites);
    // Rows: section headers interleaved with sites.
    enum Row<'a> {
        Header(String),
        Site(&'a hookecho_core::sites::Site, bool),
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut last: Option<String> = None;
    for &(s, fav) in &items {
        let section = if fav {
            "Favorites".to_string()
        } else {
            state_name(&s.state)
        };
        if last.as_ref() != Some(&section) {
            rows.push(Row::Header(section.clone()));
            last = Some(section);
        }
        rows.push(Row::Site(s, fav));
    }
    let site_rows: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| matches!(r, Row::Site(..)))
        .map(|(i, _)| i)
        .collect();
    let header_h = size::ROW_H * 0.7;
    let heights: Vec<f32> = rows
        .iter()
        .map(|r| {
            if matches!(r, Row::Header(_)) {
                header_h
            } else {
                size::ROW_H
            }
        })
        .collect();
    let content_h: f32 = heights.iter().sum();

    // Keyboard: arrows move, Enter picks, Escape closes (handled globally).
    let (up, down, enter, wheel) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::Enter),
            i.smooth_scroll_delta.y,
        )
    });
    if !site_rows.is_empty() {
        if down {
            selected = (selected + 1).min(site_rows.len() - 1);
        }
        if up {
            selected = selected.saturating_sub(1);
        }
        selected = selected.min(site_rows.len() - 1);
    }
    let hovered = ui
        .input(|i| i.pointer.hover_pos())
        .is_some_and(|p| list.contains(p));
    if hovered {
        scroll -= wheel;
    }
    // Keep the keyboard selection visible.
    if up || down {
        let top: f32 = heights[..site_rows[selected]].iter().sum();
        if top < scroll {
            scroll = top - header_h;
        } else if top + size::ROW_H > scroll + list.height() {
            scroll = top + size::ROW_H - list.height();
        }
    }
    scroll = scroll.clamp(0.0, (content_h - list.height()).max(0.0));

    let clip = painter.with_clip_rect(list);
    let mut y = list.min.y - scroll;
    let mut picked = None;
    let mut toggle = None;
    for (i, r) in rows.iter().enumerate() {
        let h = heights[i];
        let row_rect = Rect::from_min_size(pos2(list.min.x, y), vec2(list.width(), h));
        y += h;
        if row_rect.max.y < list.min.y || row_rect.min.y > list.max.y {
            continue;
        }
        match r {
            Row::Header(name) => {
                kit.text(
                    &clip,
                    pos2(row_rect.min.x + space::S2, row_rect.center().y + 2.0),
                    Align2::LEFT_CENTER,
                    name.to_uppercase(),
                    Type::Caption,
                    t.text_tertiary,
                );
            }
            Row::Site(s, fav) => {
                let k = site_rows.iter().position(|&x| x == i).unwrap_or(0);
                let visible = row_rect.intersect(list);
                let resp = interact(
                    &mut ui,
                    visible,
                    Id::new(("site-row", &s.id)),
                    WidgetType::Button,
                    &format!("{} {}, {}", s.id, s.name, s.state),
                    Some(k == selected),
                );
                let current = s.id == model.site.id;
                kit.row_fill(&clip, row_rect, &resp, k == selected);
                let cy = row_rect.center().y;
                // Accent text only on plain glass (contrast test); a selected
                // row's fill already marks it.
                let id_color = if current && k != selected {
                    t.accent
                } else {
                    t.text_primary
                };
                kit.text(
                    &clip,
                    pos2(row_rect.min.x + space::S2, cy),
                    Align2::LEFT_CENTER,
                    s.id.clone(),
                    Type::Label,
                    id_color,
                );
                kit.text(
                    &clip,
                    pos2(row_rect.min.x + space::S2 + size::PICKER_ID_COL, cy),
                    Align2::LEFT_CENTER,
                    s.name.clone(),
                    Type::Body,
                    t.text_secondary,
                );
                kit.text(
                    &clip,
                    pos2(row_rect.max.x - space::S2 - size::ICON_MD - space::S2, cy),
                    Align2::RIGHT_CENTER,
                    s.state.clone(),
                    Type::Caption,
                    t.text_tertiary,
                );
                let star = Rect::from_center_size(
                    pos2(row_rect.max.x - space::S2 - size::ICON_SM / 2.0 - 2.0, cy),
                    vec2(size::ICON_MD, size::ICON_MD),
                );
                let star_resp = interact(
                    &mut ui,
                    star.intersect(list),
                    Id::new(("fav", &s.id)),
                    WidgetType::Checkbox,
                    &format!("Favorite {}", s.id),
                    Some(*fav),
                );
                let star_color = if *fav {
                    t.status_aging
                } else if star_resp.hovered() {
                    t.text_secondary
                } else {
                    t.text_tertiary.gamma_multiply(0.6)
                };
                kit.icon(&clip, star, Icon::Star, size::ICON_SM, star_color);
                if star_resp.clicked() {
                    toggle = Some(s.id.clone());
                } else if resp.clicked() {
                    picked = Some(*s);
                }
                if resp.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                    selected = k;
                }
            }
        }
    }
    if rows.is_empty() {
        kit.text(
            &painter,
            pos2(list.center().x, list.min.y + space::S6),
            Align2::CENTER_CENTER,
            "No radars match",
            Type::Body,
            t.text_tertiary,
        );
    }
    scroll_indicator(kit, &painter, list, scroll, content_h);
    if enter
        && !site_rows.is_empty()
        && let Row::Site(s, _) = rows[site_rows[selected]]
    {
        picked = Some(s);
    }
    if let Some(id) = toggle {
        model.toggle_favorite(&id);
    }
    model.picker = Picker::Site {
        query,
        selected,
        scroll,
    };
    if let Some(s) = picked {
        let size = [screen.width() as f64, screen.height() as f64];
        model.select_site(s, size);
        model.picker = Picker::None;
        return;
    }
    close_on_outside_click(&ui, model, rect, anchor);
}

pub fn product_picker(ui: &mut Ui, kit: &mut Kit, model: &mut Model, anchor: Rect) {
    let Picker::Product { mut row, mut col } = model.picker.clone() else {
        return;
    };
    let t = kit.theme.clone();
    let tilts = model.loaded_here().map(|l| l.tilts()).unwrap_or_default();
    let pad = space::S3;
    let label_w = size::PICKER_LABEL_W;
    let header_h = size::ROW_H * 0.75;
    let cols_shown = tilts.len().max(1);
    let w = (pad * 2.0 + label_w + cols_shown as f32 * (size::TILT_CELL_W + space::S1))
        .min(kit.screen.width() - space::S6);
    let h = pad * 2.0 + header_h + PRODUCTS.len() as f32 * size::ROW_H;
    let rect = popover_rect(kit.screen, anchor, w, h);
    let layer = LayerId::new(Order::Foreground, Id::new("product-picker"));
    let painter = ui.ctx().layer_painter(layer);
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    kit.glass(&painter, rect, radius::LG, 2);

    // Columns that don't fit are scrolled into view by keyboard focus.
    let grid_x = rect.min.x + pad + label_w;
    let fit =
        (((rect.max.x - pad - grid_x) / (size::TILT_CELL_W + space::S1)).floor() as usize).max(1);
    let (up, down, left, right, enter) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::ArrowLeft),
            i.key_pressed(Key::ArrowRight),
            i.key_pressed(Key::Enter),
        )
    });
    if down {
        row = (row + 1).min(PRODUCTS.len() - 1);
    }
    if up {
        row = row.saturating_sub(1);
    }
    if right {
        col = (col + 1).min(tilts.len().saturating_sub(1));
    }
    if left {
        col = col.saturating_sub(1);
    }
    let first = col.saturating_sub(fit - 1);

    kit.text(
        &painter,
        pos2(
            rect.min.x + pad + space::S2,
            rect.min.y + pad + header_h / 2.0,
        ),
        Align2::LEFT_CENTER,
        "PRODUCT",
        Type::Caption,
        t.text_tertiary,
    );
    for (k, tilt) in tilts.iter().enumerate().skip(first).take(fit) {
        let x =
            grid_x + (k - first) as f32 * (size::TILT_CELL_W + space::S1) + size::TILT_CELL_W / 2.0;
        kit.text(
            &painter,
            pos2(x, rect.min.y + pad + header_h / 2.0),
            Align2::CENTER_CENTER,
            format!("{:.1}°", tilt.deg),
            Type::CaptionTab,
            t.text_tertiary,
        );
    }
    let mut pick = None;
    let available = |m, deg| model.loaded_here().is_some_and(|l| l.has(m, deg));
    for (r, &m) in PRODUCTS.iter().enumerate() {
        let y = rect.min.y + pad + header_h + r as f32 * size::ROW_H;
        let cy = y + size::ROW_H / 2.0;
        let current_row = m == model.moment;
        kit.text(
            &painter,
            pos2(rect.min.x + pad + space::S2, cy),
            Align2::LEFT_CENTER,
            m.short_name(),
            Type::Body,
            if current_row {
                t.text_primary
            } else {
                t.text_secondary
            },
        );
        for (k, tilt) in tilts.iter().enumerate().skip(first).take(fit) {
            let x = grid_x + (k - first) as f32 * (size::TILT_CELL_W + space::S1);
            let cell = Rect::from_min_size(
                pos2(x, cy - size::TILT_CELL_H / 2.0),
                vec2(size::TILT_CELL_W, size::TILT_CELL_H),
            );
            let has = available(m, tilt.deg) && tilt.state == TiltState::In;
            let current = current_row && (tilt.deg - model.tilt_deg).abs() < 0.05;
            let focused = r == row && k == col;
            if !has {
                // Not in this volume: a quiet hollow marker, not a dead button.
                kit.empty_dot(&painter, cell.center());
                continue;
            }
            let resp = interact(
                &mut ui,
                cell,
                Id::new(("cell", r, k)),
                WidgetType::Button,
                &format!("{} {:.1} degrees", m.name(), tilt.deg),
                Some(current),
            );
            if current {
                painter.rect_filled(cell, CornerRadius::same(radius::SM), t.accent);
            } else {
                kit.state_fill(&painter, cell, radius::SM, &resp, focused);
            }
            // The tilt is in the column header; the cell is just a target.
            kit.cell_dot(
                &painter,
                cell.center(),
                if current {
                    t.accent_on
                } else if resp.hovered() || focused {
                    t.text_primary
                } else {
                    t.text_secondary
                },
            );
            if focused && !current {
                painter.rect_stroke(
                    cell,
                    CornerRadius::same(radius::SM),
                    egui::Stroke::new(size::FOCUS_RING / 2.0, t.accent),
                    egui::StrokeKind::Inside,
                );
            }
            if resp.clicked() {
                pick = Some((m, tilt.deg));
            }
        }
    }
    if enter
        && let Some(tilt) = tilts.get(col)
        && available(PRODUCTS[row], tilt.deg)
    {
        pick = Some((PRODUCTS[row], tilt.deg));
    }
    model.picker = Picker::Product { row, col };
    if let Some((m, deg)) = pick {
        model.moment = m;
        model.tilt_deg = deg;
        model.picker = Picker::None;
        return;
    }
    close_on_outside_click(&ui, model, rect, anchor);
}

/// Section titles for the site list.
fn state_name(code: &str) -> String {
    const NAMES: [(&str, &str); 56] = [
        ("AK", "Alaska"),
        ("AL", "Alabama"),
        ("AR", "Arkansas"),
        ("AZ", "Arizona"),
        ("CA", "California"),
        ("CO", "Colorado"),
        ("CT", "Connecticut"),
        ("DC", "District of Columbia"),
        ("DE", "Delaware"),
        ("FL", "Florida"),
        ("GA", "Georgia"),
        ("GU", "Guam"),
        ("HI", "Hawaii"),
        ("IA", "Iowa"),
        ("ID", "Idaho"),
        ("IL", "Illinois"),
        ("IN", "Indiana"),
        ("KS", "Kansas"),
        ("KY", "Kentucky"),
        ("LA", "Louisiana"),
        ("MA", "Massachusetts"),
        ("MD", "Maryland"),
        ("ME", "Maine"),
        ("MI", "Michigan"),
        ("MN", "Minnesota"),
        ("MO", "Missouri"),
        ("MS", "Mississippi"),
        ("MT", "Montana"),
        ("NC", "North Carolina"),
        ("ND", "North Dakota"),
        ("NE", "Nebraska"),
        ("NH", "New Hampshire"),
        ("NJ", "New Jersey"),
        ("NM", "New Mexico"),
        ("NV", "Nevada"),
        ("NY", "New York"),
        ("OH", "Ohio"),
        ("OK", "Oklahoma"),
        ("OR", "Oregon"),
        ("PA", "Pennsylvania"),
        ("PR", "Puerto Rico"),
        ("RI", "Rhode Island"),
        ("SC", "South Carolina"),
        ("SD", "South Dakota"),
        ("TN", "Tennessee"),
        ("TX", "Texas"),
        ("UT", "Utah"),
        ("VA", "Virginia"),
        ("VT", "Vermont"),
        ("WA", "Washington"),
        ("WI", "Wisconsin"),
        ("WV", "West Virginia"),
        ("WY", "Wyoming"),
        ("AS", "American Samoa"),
        ("VI", "U.S. Virgin Islands"),
        ("MP", "Northern Mariana Islands"),
    ];
    match code {
        "" => "Outside the US".to_string(),
        c => NAMES
            .iter()
            .find(|(k, _)| *k == c)
            .map_or_else(|| c.to_string(), |(_, n)| n.to_string()),
    }
}
