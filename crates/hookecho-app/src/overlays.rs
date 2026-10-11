//! Centered overlays: the command palette (docs/DESIGN.md §3.8) and the
//! keyboard shortcuts sheet (§3.9).

use crate::commands::{self, Action, Command, Group, SHORTCUTS, ShortcutGroup};
use crate::design::components::{self, Kit, interact, scroll_indicator};
use crate::design::icons::Icon;
use crate::design::tokens::{Type, radius, size, space};
use crate::model::{Model, Picker};
use eframe::egui::{self, Align2, Id, Key, LayerId, Order, Rect, Ui, WidgetType, pos2, vec2};

fn icon_for(c: &Command) -> Icon {
    match (&c.action, c.group) {
        (
            Action::Palette(_)
            | Action::RestoreColors
            | Action::Sheet(crate::model::SheetTab::Colors),
            _,
        ) => Icon::Palette,
        (Action::Shortcuts, _) => Icon::Keyboard,
        (Action::Live | Action::PlayPause, _) => Icon::Play,
        (Action::ZoomIn | Action::ZoomOut, _) => Icon::Search,
        (_, Group::Sites) => Icon::Radar,
        (_, Group::Products) => Icon::Gauge,
        (_, Group::Layers) => Icon::Layers,
        (_, Group::Actions) => Icon::Zap,
        (_, Group::Settings) => Icon::Settings,
    }
}

/// Height of the result list: a header per group plus the rows.
fn list_height(results: &[Command]) -> f32 {
    let mut groups: Vec<Group> = results.iter().map(|c| c.group).collect();
    groups.dedup();
    groups.len() as f32 * size::ROW_H * 0.7 + results.len() as f32 * size::ROW_H
}

/// Opens the command palette (Ctrl+K, or the search button).
pub fn open_command(model: &mut Model) {
    model.picker = Picker::Command {
        query: String::new(),
        selected: 0,
        scroll: 0.0,
    };
}

/// A card centered horizontally, `space.6` from the top.
fn card(screen: Rect, w: f32, h: f32) -> Rect {
    let w = w.min(screen.width() - 2.0 * space::S3);
    let top = screen.min.y + space::S6;
    let h = h.min(screen.max.y - top - space::S3);
    Rect::from_min_size(pos2(screen.center().x - w / 2.0, top), vec2(w, h))
}

/// Closes the overlay when a click lands outside it and outside `anchor`
/// (the button that toggles it).
fn close_outside(ui: &Ui, model: &mut Model, rect: Rect, anchor: Rect) {
    if ui.input(|i| {
        i.pointer.any_click()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !rect.contains(p) && !anchor.contains(p))
    }) {
        model.picker = Picker::None;
    }
}

pub fn command_palette(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    view: [f64; 2],
    anchor: Rect,
) {
    let Picker::Command {
        mut query,
        mut selected,
        mut scroll,
    } = model.picker.clone()
    else {
        return;
    };
    let t = kit.theme.clone();
    // Sized to the results (from the query as it stood at the frame's
    // start; a keystroke resizes it on the next frame).
    let list_h = list_height(&commands::search(commands::all(model, &query), &query));
    let h =
        space::S3 + size::COMMAND_FIELD_H + space::S2 + list_h.max(2.0 * size::ROW_H) + space::S2;
    let rect = card(kit.screen, size::COMMAND_W, h.min(size::COMMAND_MAX_H));
    let layer = LayerId::new(Order::Foreground, Id::new("command-palette"));
    let painter = ui.ctx().layer_painter(layer);
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    kit.glass(&painter, rect, radius::LG, 3);
    let pad = space::S3;

    let field = Rect::from_min_size(
        rect.min + vec2(pad, pad),
        vec2(rect.width() - 2.0 * pad, size::COMMAND_FIELD_H),
    );
    let field_id = Id::new("command-search");
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
        "Type a radar, product, layer or setting",
    ) {
        selected = 0;
        scroll = 0.0;
    }

    let results = commands::search(commands::all(model, &query), &query);
    let list = Rect::from_min_max(
        pos2(rect.min.x + space::S2, field.max.y + space::S2),
        pos2(rect.max.x - space::S2, rect.max.y - space::S2),
    );
    // Rows: a header before each group's first command.
    let header_h = size::ROW_H * 0.7;
    let mut rows: Vec<(f32, Option<usize>, Option<Group>)> = Vec::new();
    let mut last = None;
    for (i, c) in results.iter().enumerate() {
        if last != Some(c.group) {
            rows.push((header_h, None, Some(c.group)));
            last = Some(c.group);
        }
        rows.push((size::ROW_H, Some(i), None));
    }
    let content_h: f32 = rows.iter().map(|r| r.0).sum();

    let (up, down, enter, wheel) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::Enter),
            i.smooth_scroll_delta.y,
        )
    });
    if !results.is_empty() {
        if down {
            selected = (selected + 1).min(results.len() - 1);
        }
        if up {
            selected = selected.saturating_sub(1);
        }
        selected = selected.min(results.len() - 1);
    }
    if ui.rect_contains_pointer(list) {
        scroll -= wheel;
    }
    if up || down {
        let mut top = 0.0;
        for r in &rows {
            if r.1 == Some(selected) {
                break;
            }
            top += r.0;
        }
        if top - header_h < scroll {
            scroll = (top - header_h).max(0.0);
        } else if top + size::ROW_H > scroll + list.height() {
            scroll = top + size::ROW_H - list.height();
        }
    }
    scroll = scroll.clamp(0.0, (content_h - list.height()).max(0.0));

    let clip = painter.with_clip_rect(list);
    let mut y = list.min.y - scroll;
    let mut run = enter.then_some(selected).filter(|_| !results.is_empty());
    for (h, cmd_i, group) in &rows {
        let row = Rect::from_min_size(pos2(list.min.x, y), vec2(list.width(), *h));
        y += h;
        if row.max.y < list.min.y || row.min.y > list.max.y {
            continue;
        }
        if let Some(g) = group {
            kit.text(
                &clip,
                pos2(row.min.x + space::S2, row.center().y + space::S1 / 2.0),
                Align2::LEFT_CENTER,
                g.label(),
                Type::Caption,
                t.text_tertiary,
            );
            continue;
        }
        let Some(i) = *cmd_i else { continue };
        let c = &results[i];
        let resp = interact(
            &mut ui,
            row.intersect(list),
            Id::new(("command", i, &c.label)),
            WidgetType::Button,
            &c.label,
            Some(i == selected),
        );
        if resp.hovered() && ui.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
            selected = i;
        }
        kit.row_fill(&clip, row, &resp, i == selected);
        let cy = row.center().y;
        let icon = components::icon_rect(
            pos2(row.min.x + space::S2 + size::ICON_SM / 2.0, cy),
            size::ICON_SM,
        );
        kit.icon(&clip, icon, icon_for(c), size::ICON_SM, t.text_secondary);
        let label = kit.text(
            &clip,
            pos2(icon.max.x + space::S3, cy),
            Align2::LEFT_CENTER,
            c.label.clone(),
            Type::Body,
            t.text_primary,
        );
        let mut right = row.max.x - space::S2;
        if let Some(k) = c.shortcut {
            let g = kit.galley(&clip, k, Type::Caption, t.text_secondary);
            let w = g.size().x + space::S2;
            let r = kit.keycap(&clip, pos2(right - w, cy - size::KEYCAP_H / 2.0), k);
            right = r.min.x - space::S2;
        }
        if !c.hint.is_empty() && right > label.max.x + space::S4 {
            // The hint gives way to the label in a narrow window.
            let hint = kit.galley(&clip, c.hint.clone(), Type::Caption, t.text_tertiary);
            if right - hint.size().x > label.max.x + space::S3 {
                kit.text(
                    &clip,
                    pos2(right, cy),
                    Align2::RIGHT_CENTER,
                    c.hint.clone(),
                    Type::Caption,
                    t.text_tertiary,
                );
            }
        }
        if resp.clicked() {
            run = Some(i);
        }
    }
    if results.is_empty() {
        kit.text(
            &painter,
            pos2(list.center().x, list.min.y + size::ROW_H),
            Align2::CENTER_CENTER,
            "Nothing matches",
            Type::Body,
            t.text_tertiary,
        );
    }
    scroll_indicator(kit, &painter, list, scroll, content_h);

    model.picker = Picker::Command {
        query,
        selected,
        scroll,
    };
    if let Some(i) = run {
        let now = ui.input(|i| i.time);
        commands::run(model, &results[i].action, view, now);
    } else {
        close_outside(&ui, model, rect, anchor);
    }
}

pub fn shortcuts_sheet(ui: &mut Ui, kit: &mut Kit, model: &mut Model, anchor: Rect) {
    let Picker::Shortcuts = model.picker else {
        return;
    };
    let t = kit.theme.clone();
    let pad = space::S4;
    let eyebrow = size::ROW_H * 0.7;
    // Two columns: groups fill the left until it's at least half the rows.
    let total: usize = SHORTCUTS.iter().map(|(_, r)| r.len()).sum();
    let mut split = 0;
    let mut acc = 0;
    for (i, (_, r)) in SHORTCUTS.iter().enumerate() {
        if acc * 2 >= total {
            break;
        }
        acc += r.len();
        split = i + 1;
    }
    let col_h = |groups: &[ShortcutGroup]| {
        groups
            .iter()
            .map(|(_, r)| eyebrow + r.len() as f32 * size::SHORTCUT_ROW_H + space::S3)
            .sum::<f32>()
    };
    let (left, right) = SHORTCUTS.split_at(split);
    let narrow = kit.screen.width() < size::SHORTCUTS_W + 2.0 * space::S3;
    let body_h = if narrow {
        col_h(SHORTCUTS)
    } else {
        col_h(left).max(col_h(right))
    };
    let title_h = size::ROW_H;
    let rect = card(kit.screen, size::SHORTCUTS_W, pad + title_h + body_h + pad);
    let layer = LayerId::new(Order::Foreground, Id::new("shortcuts"));
    let painter = ui.ctx().layer_painter(layer);
    let ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
    kit.glass(&painter, rect, radius::LG, 3);
    kit.text(
        &painter,
        pos2(rect.min.x + pad, rect.min.y + pad + title_h / 2.0),
        Align2::LEFT_CENTER,
        "Keyboard shortcuts",
        Type::Title,
        t.text_primary,
    );
    let clip = painter.with_clip_rect(rect.shrink(space::S1));
    let col_w = if narrow {
        rect.width() - 2.0 * pad
    } else {
        (rect.width() - 3.0 * pad) / 2.0
    };
    let columns: Vec<&[ShortcutGroup]> = if narrow {
        vec![SHORTCUTS]
    } else {
        vec![left, right]
    };
    for (ci, groups) in columns.iter().enumerate() {
        let x = rect.min.x + pad + ci as f32 * (col_w + pad);
        let mut y = rect.min.y + pad + title_h;
        for (name, keys) in groups.iter() {
            kit.text(
                &clip,
                pos2(x, y + eyebrow / 2.0),
                Align2::LEFT_CENTER,
                *name,
                Type::Caption,
                t.text_tertiary,
            );
            y += eyebrow;
            for (k, what) in keys.iter() {
                let cy = y + size::SHORTCUT_ROW_H / 2.0;
                kit.keycap(&clip, pos2(x, cy - size::KEYCAP_H / 2.0), k);
                kit.text(
                    &clip,
                    pos2(x + size::SHORTCUT_KEY_COL, cy),
                    Align2::LEFT_CENTER,
                    *what,
                    Type::Body,
                    t.text_primary,
                );
                y += size::SHORTCUT_ROW_H;
            }
            y += space::S3;
        }
    }
    close_outside(&ui, model, rect, anchor);
}
