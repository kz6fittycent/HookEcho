//! Site, product and palette pickers (docs/DESIGN.md §3.7): glass popovers anchored
//! to their chips, keyboard-driven, built from design components only.

use crate::design::components::{
    self, Kit, interact, popover_above, popover_rect, scroll_indicator,
};
use crate::design::icons::Icon;
use crate::design::tokens::{Type, radius, size, space};
use crate::model::{self, Model, PRODUCTS, Picker, TiltState};
use eframe::egui::{
    self, Align2, CornerRadius, Id, Key, LayerId, Order, Rect, Ui, WidgetType, pos2, vec2,
};
use hookecho_core::Moment;

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
    // With nothing typed, the list runs nearest-first from the middle of the
    // map, each row with its distance; a search groups by state as before.
    let near = model.map_center();
    let by_distance = query.trim().is_empty();
    let fmt = model::Fmt::of(&model.config);
    let items = model::site_list(&query, &model.config.favorites, Some(near));
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
        } else if by_distance {
            "Closest to the map center".to_string()
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
                    if by_distance {
                        let km = hookecho_core::geo::range_bearing(near, (s.lat, s.lon)).0;
                        format!("{} · {}", fmt.distance(km), s.state)
                    } else {
                        s.state.clone()
                    },
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

/// Product picker: a list of products, each with what it shows, then the
/// scan angles as buttons. Clicking either applies it and closes the picker;
/// arrow keys change product (up/down) and angle (left/right) live, and
/// Enter or Escape closes.
pub fn product_picker(ui: &mut Ui, kit: &mut Kit, model: &mut Model, anchor: Rect) {
    let Picker::Product = model.picker else {
        return;
    };
    let t = kit.theme.clone();
    let tilts = model.loaded_here().map(|l| l.tilts()).unwrap_or_default();
    let pad = space::S3;
    let w = size::PICKER_W.min(kit.screen.width() - space::S6);
    let chip = vec2(size::TILT_CHIP_W, size::TILT_CELL_H);
    let per_row = (((w - 2.0 * pad + space::S1) / (chip.x + space::S1)).floor() as usize).max(1);
    let chip_rows = tilts.len().div_ceil(per_row).max(1);
    let eyebrow = size::ROW_H * 0.6;
    let h = pad
        + eyebrow
        + PRODUCTS.len() as f32 * size::PRODUCT_ROW_H
        + space::S3
        + eyebrow
        + size::HINT_H
        + chip_rows as f32 * (chip.y + space::S1)
        + pad;
    let rect = popover_rect(kit.screen, anchor, w, h);
    let layer = LayerId::new(Order::Foreground, Id::new("product-picker"));
    let painter = ui.ctx().layer_painter(layer);
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    kit.glass(&painter, rect, radius::LG, 2);
    // (product, angle) pairs present in the volume, computed once so the
    // picker can change the model while drawing.
    let present: Vec<(Moment, f32)> = model
        .loaded_here()
        .map(|l| {
            PRODUCTS
                .iter()
                .flat_map(|&m| {
                    tilts
                        .iter()
                        .filter(move |x| l.has(m, x.deg))
                        .map(move |x| (m, x.deg))
                })
                .collect()
        })
        .unwrap_or_default();
    let available = |m: Moment, deg: f32| {
        present
            .iter()
            .any(|&(pm, d)| pm == m && (d - deg).abs() < 0.05)
    };

    // Keyboard: arrows apply live, Enter closes.
    let (up, down, left, right, enter) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::ArrowLeft),
            i.key_pressed(Key::ArrowRight),
            i.key_pressed(Key::Enter),
        )
    });
    let cur = PRODUCTS
        .iter()
        .position(|&p| p == model.moment)
        .unwrap_or(0);
    if up || down {
        let next = if down {
            (cur + 1).min(PRODUCTS.len() - 1)
        } else {
            cur.saturating_sub(1)
        };
        model.moment = PRODUCTS[next];
    }
    if left || right {
        let usable: Vec<f32> = tilts
            .iter()
            .filter(|x| x.state != TiltState::Pending && available(model.moment, x.deg))
            .map(|x| x.deg)
            .collect();
        if let Some(i) = usable
            .iter()
            .position(|&d| (d - model.tilt_deg).abs() < 0.05)
        {
            let j = if right {
                (i + 1).min(usable.len() - 1)
            } else {
                i.saturating_sub(1)
            };
            model.tilt_deg = usable[j];
        }
    }

    let mut y = rect.min.y + pad;
    kit.text(
        &painter,
        pos2(rect.min.x + pad + space::S2, y + eyebrow / 2.0),
        Align2::LEFT_CENTER,
        "PRODUCT",
        Type::Caption,
        t.text_tertiary,
    );
    y += eyebrow;
    let mut close = enter;
    for &m in PRODUCTS.iter() {
        let row = Rect::from_min_size(
            pos2(rect.min.x + space::S2, y),
            vec2(rect.width() - 2.0 * space::S2, size::PRODUCT_ROW_H),
        );
        y += size::PRODUCT_ROW_H;
        let (name, what) = model::product_label(m);
        let selected = m == model.moment;
        let resp = interact(
            &mut ui,
            row,
            Id::new(("product", m.code())),
            WidgetType::Button,
            &format!("{name}: {what}"),
            Some(selected),
        );
        kit.row_fill(&painter, row, &resp, selected);
        if selected {
            kit.selection_bar(&painter, row);
        }
        let x = row.min.x + space::S3;
        kit.text(
            &painter,
            pos2(x, row.center().y - size::PRODUCT_LINE_GAP),
            Align2::LEFT_CENTER,
            name,
            Type::Body,
            t.text_primary,
        );
        kit.text(
            &painter,
            pos2(x, row.center().y + size::PRODUCT_LINE_GAP),
            Align2::LEFT_CENTER,
            what,
            Type::Caption,
            t.text_tertiary,
        );
        if resp.clicked() {
            model.moment = m;
            close = true;
        }
    }
    // Keep the angle valid for the product (velocity isn't scanned at every angle).
    if !available(model.moment, model.tilt_deg)
        && let Some(d) = tilts
            .iter()
            .find(|x| available(model.moment, x.deg))
            .map(|x| x.deg)
    {
        model.tilt_deg = d;
    }

    y += space::S3;
    kit.divider(
        &painter,
        rect.min.x + pad,
        rect.max.x - pad,
        y - space::S3 / 2.0,
    );
    kit.text(
        &painter,
        pos2(rect.min.x + pad + space::S2, y + eyebrow / 2.0),
        Align2::LEFT_CENTER,
        "SCAN ANGLE",
        Type::Caption,
        t.text_tertiary,
    );
    y += eyebrow;
    kit.text(
        &painter,
        pos2(rect.min.x + pad + space::S2, y + size::HINT_H / 2.0),
        Align2::LEFT_CENTER,
        "Lower angles see closer to the ground",
        Type::Caption,
        t.text_tertiary,
    );
    y += size::HINT_H;
    for (k, tilt) in tilts.iter().enumerate() {
        let (r, c) = (k / per_row, k % per_row);
        let cell = Rect::from_min_size(
            pos2(
                rect.min.x + pad + c as f32 * (chip.x + space::S1),
                y + r as f32 * (chip.y + space::S1),
            ),
            chip,
        );
        let label = format!("{:.1}°", tilt.deg);
        let usable = tilt.state != TiltState::Pending && available(model.moment, tilt.deg);
        if !usable {
            // Not scanned yet in this volume, or this product isn't measured
            // at this angle: shown, but clearly not a button.
            let why = if tilt.state == TiltState::Pending {
                "not scanned yet"
            } else {
                "not measured at this angle"
            };
            kit.text(
                &painter,
                cell.center(),
                Align2::CENTER_CENTER,
                label,
                Type::CaptionTab,
                t.text_tertiary.gamma_multiply(0.55),
            );
            let resp = ui.interact(cell, Id::new(("tilt-off", k)), egui::Sense::hover());
            kit.tooltip(&ui, &resp, &format!("{:.1}°: {why}", tilt.deg), None);
            continue;
        }
        let current = (tilt.deg - model.tilt_deg).abs() < 0.05;
        let lowest = k == 0;
        let tip = format!(
            "{:.1}°{}",
            tilt.deg,
            if lowest {
                " · lowest scan, closest to the ground"
            } else {
                ""
            }
        );
        let resp = interact(
            &mut ui,
            cell,
            Id::new(("tilt", k)),
            WidgetType::Button,
            &tip,
            Some(current),
        );
        if current {
            painter.rect_filled(cell, CornerRadius::same(radius::SM), t.accent);
        } else {
            kit.state_fill(&painter, cell, radius::SM, &resp, false);
            painter.rect_stroke(
                cell,
                CornerRadius::same(radius::SM),
                egui::Stroke::new(components::hairline(painter.ctx()), t.glass_stroke),
                egui::StrokeKind::Inside,
            );
        }
        kit.text(
            &painter,
            cell.center(),
            Align2::CENTER_CENTER,
            label,
            Type::CaptionTab,
            if current { t.accent_on } else { t.text_primary },
        );
        if tilt.state == TiltState::Arriving {
            kit.sweep_dot(
                &painter,
                pos2(
                    cell.max.x - size::SWEEP_DOT_R * 2.0,
                    cell.min.y + size::SWEEP_DOT_R * 2.0,
                ),
                components::DotState::Arriving,
                false,
            );
        }
        kit.focus_ring(&painter, cell, radius::SM, &resp);
        kit.tooltip(&ui, &resp, &tip, Some("[ ]"));
        if resp.clicked() {
            model.tilt_deg = tilt.deg;
            close = true;
        }
    }
    if close {
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

/// Color palettes for the product on screen, above the legend: built-ins,
/// then imported tables, each with a preview bar. Choosing one applies it.
pub fn palette_picker(ui: &mut Ui, kit: &mut Kit, model: &mut Model, anchor: Rect) {
    let Picker::Palette = model.picker else {
        return;
    };
    let t = kit.theme.clone();
    let m = model.moment;
    let choices = model.palettes.choices(m).to_vec();
    let active = model.palettes.active(&model.config, m).name.clone();
    let pad = space::S3;
    let eyebrow = size::ROW_H * 0.6;
    let w = size::LEGEND_W
        .max(size::PICKER_W * 0.75)
        .min(kit.screen.width() - space::S6);
    let h = pad
        + eyebrow
        + choices.len() as f32 * size::PRODUCT_ROW_H
        + space::S3
        + 2.0 * size::HINT_H
        + pad;
    let rect = popover_above(kit.screen, anchor, w, h);
    let layer = LayerId::new(Order::Foreground, Id::new("palette-picker"));
    let painter = ui.ctx().layer_painter(layer);
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    kit.glass(&painter, rect, radius::LG, 2);

    let (up, down, enter) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::Enter),
        )
    });
    let cur = choices
        .iter()
        .position(|c| c.palette.name == active)
        .unwrap_or(0);
    if (up || down) && !choices.is_empty() {
        let next = if down {
            (cur + 1).min(choices.len() - 1)
        } else {
            cur.saturating_sub(1)
        };
        let name = choices[next].palette.name.clone();
        model.palettes.select(&mut model.config, m, &name);
    }

    let mut y = rect.min.y + pad;
    kit.text(
        &painter,
        pos2(rect.min.x + pad + space::S2, y + eyebrow / 2.0),
        Align2::LEFT_CENTER,
        format!("{} COLORS", m.short_name().to_uppercase()),
        Type::Caption,
        t.text_tertiary,
    );
    y += eyebrow;
    let mut close = enter;
    if palette_rows(
        &mut ui,
        kit,
        &painter,
        model,
        rect.min.x + space::S2,
        rect.max.x - space::S2,
        &mut y,
    ) {
        close = true;
    }
    y += space::S3;
    kit.divider(
        &painter,
        rect.min.x + pad,
        rect.max.x - pad,
        y - space::S3 / 2.0,
    );
    for line in [
        "To add your own, drop a GRLevelX .pal file",
        "anywhere on the window",
    ] {
        kit.text(
            &painter,
            pos2(rect.min.x + pad + space::S2, y + size::HINT_H / 2.0),
            Align2::LEFT_CENTER,
            line,
            Type::Caption,
            t.text_tertiary,
        );
        y += size::HINT_H;
    }
    if close {
        model.picker = Picker::None;
    } else {
        close_on_outside_click(&ui, model, rect, anchor);
    }
}

/// Palette rows for the product on screen, from `*y` down (shared by the
/// legend's picker and the side sheet's Colors tab). Returns true when one
/// was clicked.
pub fn palette_rows(
    ui: &mut Ui,
    kit: &mut Kit,
    painter: &egui::Painter,
    model: &mut Model,
    x0: f32,
    x1: f32,
    y: &mut f32,
) -> bool {
    let t = kit.theme.clone();
    let m = model.moment;
    let choices = model.palettes.choices(m).to_vec();
    let active = model.palettes.active(&model.config, m).name.clone();
    let mut chosen = false;
    let mut remove = None;
    for c in &choices {
        let row = Rect::from_min_size(pos2(x0, *y), vec2(x1 - x0, size::PRODUCT_ROW_H));
        *y += size::PRODUCT_ROW_H;
        let p = &c.palette;
        let selected = p.name == active;
        let resp = interact(
            ui,
            row,
            Id::new(("palette", m.code(), &p.name)),
            WidgetType::Button,
            &p.name,
            Some(selected),
        );
        kit.row_fill(painter, row, &resp, selected);
        if selected {
            kit.selection_bar(painter, row);
        }
        let (tx0, tx1) = (row.min.x + space::S3, row.max.x - space::S3);
        let line1 = row.center().y - size::PRODUCT_LINE_GAP;
        kit.text(
            painter,
            pos2(tx0, line1),
            Align2::LEFT_CENTER,
            &p.name,
            Type::Body,
            t.text_primary,
        );
        if c.imported {
            // A remove button, then the "Imported" tag left of it.
            let x = Rect::from_center_size(
                pos2(tx1 - size::ICON_SM / 2.0, line1),
                vec2(size::ICON_MD, size::ICON_MD),
            );
            let label = format!("Remove {}", p.name);
            let rm = components::icon_button(
                ui,
                kit,
                painter,
                x,
                Id::new(("palette-remove", m.code(), &p.name)),
                Icon::X,
                &label,
                None,
                false,
            );
            if rm.clicked() {
                remove = Some(p.name.clone());
            }
            kit.text(
                painter,
                pos2(x.min.x - space::S2, line1),
                Align2::RIGHT_CENTER,
                "Imported",
                Type::Caption,
                t.text_tertiary,
            );
        }
        let bar = Rect::from_min_size(
            pos2(
                tx0,
                row.center().y + size::PRODUCT_LINE_GAP - size::PALETTE_PREVIEW_H / 2.0,
            ),
            vec2(tx1 - tx0, size::PALETTE_PREVIEW_H),
        );
        let (lo, hi) = (p.min(), p.max());
        kit.color_bar(painter, bar, |f| p.sample(lo + (hi - lo) * f));
        if resp.clicked() {
            model.palettes.select(&mut model.config, m, &p.name);
            chosen = true;
        }
    }
    if let Some(name) = remove {
        let r = model.palettes.remove(&mut model.config, m, &name);
        if let Err(e) = r {
            eprintln!("hookecho: removing palette: {e}");
        }
    }
    chosen
}
