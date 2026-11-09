//! The menu bar (docs/DESIGN.md §3.2): one glass strip across the top with
//! the menus on the left, the radar and product chips beside them, and
//! Search at the right; and the menus it opens. The menus' contents come
//! from `menus.rs`; picking a row runs the same action the search palette
//! would.

use crate::commands::{self, Action};
use crate::design::components::{self, Kit, interact};
use crate::design::icons::Icon;
use crate::design::tokens::{Type, radius, size, space};
use crate::menus::{self, Item, Mark, MenuId};
use crate::model::{MenuState, Model, Picker};
use eframe::egui::{
    self, Align2, Id, Key, LayerId, Order, Rect, Stroke, Ui, WidgetType, pos2, vec2,
};

/// What the bar laid out, for the pickers and menus that hang off it.
pub struct Bar {
    pub rect: Rect,
    pub site: Rect,
    pub product: Rect,
    pub search: Rect,
    /// Each menu title shown, and its rect.
    pub titles: Vec<(MenuId, Rect)>,
}

/// The bar's rect: the window's width less the margin, a chip tall.
pub fn bar_rect(screen: Rect, m: f32) -> Rect {
    Rect::from_min_max(
        pos2(screen.min.x + m, screen.min.y + m),
        pos2(screen.max.x - m, screen.min.y + m + size::CHIP_H),
    )
}

fn title_width(kit: &Kit, painter: &egui::Painter, id: MenuId) -> f32 {
    kit.galley(painter, id.title(), Type::Label, kit.theme.text_primary)
        .size()
        .x
        + 2.0 * space::S3
}

fn vline(kit: &Kit, painter: &egui::Painter, x: f32, rect: Rect) {
    painter.line_segment(
        [
            pos2(x, rect.min.y + space::S3),
            pos2(x, rect.max.y - space::S3),
        ],
        Stroke::new(components::hairline(painter.ctx()), kit.theme.glass_stroke),
    );
}

/// Draws the bar. Clicking a title opens its menu; while one is open,
/// pointing at another title switches to it.
pub fn draw(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &egui::Painter,
    screen: Rect,
    m: f32,
) -> Bar {
    let rect = bar_rect(screen, m);
    kit.glass(painter, rect, radius::MD, 1);
    let search_w = components::labeled_button_width(kit, painter, "Search");
    let site_w = crate::screen::site_chip_width(kit, painter, model);
    let full: f32 = MenuId::BAR
        .iter()
        .map(|&id| title_width(kit, painter, id))
        .sum();
    // Short of room, first the product chip drops its description (it
    // moves to the tooltip), then the titles fold into one "Menu".
    let fixed = 6.0 * space::S2 + site_w + search_w;
    let fits = |brief| {
        fixed + full + crate::screen::product_chip_width(kit, painter, model, brief) <= rect.width()
    };
    let (brief, compact) = if fits(false) {
        (false, false)
    } else {
        (true, !fits(true))
    };
    let ids: Vec<MenuId> = if compact {
        vec![MenuId::All]
    } else {
        MenuId::BAR.to_vec()
    };
    let open = match &model.picker {
        Picker::Menu(s) => Some(s.menu),
        _ => None,
    };
    let mut x = rect.min.x + space::S2;
    let mut titles = Vec::new();
    for id in ids {
        let r = if id == MenuId::All {
            let w = components::labeled_button_width(kit, painter, id.title());
            Rect::from_min_size(
                pos2(x, rect.center().y - size::ICON_BUTTON / 2.0),
                vec2(w, size::ICON_BUTTON),
            )
        } else {
            Rect::from_min_size(
                pos2(x, rect.center().y - size::MENU_TITLE_H / 2.0),
                vec2(title_width(kit, painter, id), size::MENU_TITLE_H),
            )
        };
        x = r.max.x;
        let is_open = open == Some(id);
        let resp = if id == MenuId::All {
            components::labeled_button(
                ui,
                kit,
                painter,
                r,
                Id::new(("menu-title", id.title())),
                Icon::Menu,
                id.title(),
                "Every menu",
                Some("F10"),
                is_open,
            )
        } else {
            let resp = interact(
                ui,
                r,
                Id::new(("menu-title", id.title())),
                WidgetType::Button,
                id.title(),
                Some(is_open),
            );
            kit.state_fill(painter, r, radius::SM, &resp, is_open);
            let color = if is_open || resp.hovered() {
                kit.theme.text_primary
            } else {
                kit.theme.text_secondary
            };
            kit.text(
                painter,
                r.center(),
                Align2::CENTER_CENTER,
                id.title(),
                Type::Label,
                color,
            );
            kit.focus_ring(painter, r, radius::SM, &resp);
            resp
        };
        if resp.clicked() {
            model.toggle_menu(id);
        } else if open.is_some_and(|o| o != id)
            && resp.hovered()
            && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO)
        {
            model.picker = Picker::Menu(MenuState::new(id));
        }
        titles.push((id, r));
    }
    x += space::S2;
    vline(kit, painter, x, rect);
    x += space::S2;
    let site = crate::screen::site_chip(ui, kit, model, painter, pos2(x, rect.min.y));
    x = site.max.x + space::S1;
    vline(kit, painter, x, rect);
    x += space::S1;
    let product = crate::screen::product_chip(ui, kit, model, painter, pos2(x, rect.min.y), brief);
    let search = Rect::from_min_size(
        pos2(
            rect.max.x - space::S2 - search_w,
            rect.center().y - size::ICON_BUTTON / 2.0,
        ),
        vec2(search_w, size::ICON_BUTTON),
    );
    let searching = matches!(model.picker, Picker::Command { .. });
    let resp = components::labeled_button(
        ui,
        kit,
        painter,
        search,
        Id::new("command-button"),
        Icon::Search,
        "Search",
        "Search radars, products, layers and settings",
        Some("Ctrl K"),
        searching,
    );
    if resp.clicked() {
        if searching {
            model.picker = Picker::None;
        } else {
            crate::overlays::open_command(model);
        }
    }
    Bar {
        rect,
        site,
        product,
        search,
        titles,
    }
}

fn row_h(item: &Item) -> f32 {
    match item {
        Item::Run { .. } | Item::Sub { .. } => size::MENU_ROW_H,
        Item::Heading(_) => size::MENU_HEADING_H,
        Item::Info(_) => size::MENU_INFO_H,
        Item::Separator => size::MENU_SEPARATOR_H,
    }
}

fn list_width(kit: &Kit, painter: &egui::Painter, items: &[Item]) -> f32 {
    let t = &kit.theme;
    let w = |s: &str, ty: Type| kit.galley(painter, s, ty, t.text_primary).size().x;
    let widest = items
        .iter()
        .map(|it| match it {
            Item::Run {
                label, shortcut, ..
            } => {
                size::MENU_MARK_COL
                    + w(label, Type::Body)
                    + shortcut.map_or(0.0, |k| space::S5 + w(k, Type::Caption) + space::S2 + 2.0)
                    + space::S3
            }
            Item::Sub { label, value, .. } => {
                size::MENU_MARK_COL
                    + w(label, Type::Body)
                    + space::S5
                    + if value.is_empty() {
                        0.0
                    } else {
                        w(value, Type::Caption) + space::S2
                    }
                    + size::ICON_SM
                    + space::S2
            }
            Item::Heading(s) | Item::Info(s) => {
                size::MENU_MARK_COL + w(s, Type::Caption) + space::S3
            }
            Item::Separator => 0.0,
        })
        .fold(0.0, f32::max);
    // Room for the list's own inset.
    (widest + 2.0 * space::S1).max(size::MENU_MIN_W)
}

fn next_interactive(items: &[Item], from: Option<usize>, dir: i32) -> Option<usize> {
    let n = items.len() as i32;
    if n == 0 {
        return None;
    }
    let mut i = match from {
        Some(i) => i as i32,
        None if dir > 0 => -1,
        None => n,
    };
    for _ in 0..n {
        i = (i + dir).rem_euclid(n);
        if items[i as usize].interactive() {
            return Some(i as usize);
        }
    }
    None
}

/// Opens the menu at the title left or right of `cur` (arrow keys).
fn neighbor(titles: &[(MenuId, Rect)], cur: MenuId, dir: i32) -> Option<MenuId> {
    let i = titles.iter().position(|(id, _)| *id == cur)? as i32;
    let n = titles.len() as i32;
    (n > 1).then(|| titles[(i + dir).rem_euclid(n) as usize].0)
}

/// Draws the open menu and its open submenus, and handles the pointer and
/// keys in them. `view`: the map's size (for actions that move it).
pub fn dropdown(ui: &mut Ui, kit: &mut Kit, model: &mut Model, bar: &Bar, view: [f64; 2]) {
    let Picker::Menu(mut st) = model.picker.clone() else {
        return;
    };
    // A narrow window shows only "Menu": open the menu asked for (by key or
    // from search) inside it.
    if !bar.titles.iter().any(|(id, _)| *id == st.menu) {
        if bar.titles.iter().any(|(id, _)| *id == MenuId::All)
            && let Some(i) = MenuId::BAR.iter().position(|x| *x == st.menu)
        {
            st = MenuState {
                menu: MenuId::All,
                path: vec![i],
                active: None,
                scroll: Vec::new(),
            };
        } else {
            model.picker = Picker::None;
            return;
        }
    }
    let anchor = bar
        .titles
        .iter()
        .find(|(id, _)| *id == st.menu)
        .map(|(_, r)| *r)
        .expect("checked above");
    let items = menus::build(model, st.menu);
    let mut levels = vec![items.clone()];
    for k in 0..st.path.len() {
        match menus::level(&items, &st.path[..=k]) {
            Some(l) => levels.push(l),
            None => {
                st.path.truncate(k);
                st.active = None;
                break;
            }
        }
    }
    st.scroll.resize(levels.len(), 0.0);

    // Keys act on the innermost list.
    let deepest = levels.len() - 1;
    let (up, down, left, right, enter) = ui.input(|i| {
        (
            i.key_pressed(Key::ArrowUp),
            i.key_pressed(Key::ArrowDown),
            i.key_pressed(Key::ArrowLeft),
            i.key_pressed(Key::ArrowRight),
            i.key_pressed(Key::Enter),
        )
    });
    let mut run: Option<(Action, bool)> = None;
    let open_sub = |st: &mut MenuState, i: usize| {
        st.path.push(i);
        st.active = menus::level(&items, &st.path).and_then(|l| next_interactive(&l, None, 1));
    };
    if down || up {
        st.active = next_interactive(&levels[deepest], st.active, if down { 1 } else { -1 });
    }
    if right || enter {
        match st
            .active
            .and_then(|a| levels[deepest].get(a).map(|it| (a, it)))
        {
            Some((a, Item::Sub { .. })) => open_sub(&mut st, a),
            Some((_, Item::Run { action, mark, .. })) if enter => {
                run = Some((action.clone(), matches!(mark, Mark::Check(_))));
            }
            _ if right && st.path.is_empty() => {
                if let Some(n) = neighbor(&bar.titles, st.menu, 1) {
                    st = MenuState::new(n);
                    model.picker = Picker::Menu(st);
                    return;
                }
            }
            _ => {}
        }
    }
    if left {
        if let Some(p) = st.path.pop() {
            st.active = Some(p);
        } else if let Some(n) = neighbor(&bar.titles, st.menu, -1) {
            model.picker = Picker::Menu(MenuState::new(n));
            return;
        }
    }
    // The path may have changed above; redo the levels to draw.
    let mut levels = vec![items.clone()];
    for k in 0..st.path.len() {
        if let Some(l) = menus::level(&items, &st.path[..=k]) {
            levels.push(l);
        }
    }
    st.scroll.resize(levels.len(), 0.0);

    let layer = LayerId::new(Order::Foreground, Id::new("menu"));
    let painter = ui.ctx().layer_painter(layer);
    let mut ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(kit.screen));
    let t = kit.theme.clone();
    let screen = kit.screen;
    let moved = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
    let wheel = ui.input(|i| i.smooth_scroll_delta.y);
    let mut rects: Vec<Rect> = Vec::new();
    // The row each submenu hangs from.
    let mut parent_row = anchor;
    let mut new_path: Option<Vec<usize>> = None;
    let mut new_active: Option<Option<usize>> = None;
    for (li, items) in levels.iter().enumerate() {
        let w = list_width(kit, &painter, items).min(screen.width() - 2.0 * space::S2);
        let content_h: f32 = items.iter().map(row_h).sum();
        let (x, y) = if li == 0 {
            (anchor.min.x, bar.rect.max.y + space::S1)
        } else {
            let pr = rects[li - 1];
            let x = if pr.max.x + space::S1 + w <= screen.max.x - space::S2 {
                pr.max.x + space::S1
            } else {
                pr.min.x - space::S1 - w
            };
            (x, parent_row.min.y - space::S1)
        };
        let x = x.clamp(screen.min.x + space::S2, screen.max.x - space::S2 - w);
        let max_h = screen.height() - 2.0 * space::S2;
        let h = (content_h + 2.0 * space::S1).min(max_h);
        let y = y
            .min(screen.max.y - space::S2 - h)
            .max(screen.min.y + space::S2);
        let rect = Rect::from_min_size(pos2(x, y), vec2(w, h));
        rects.push(rect);
        kit.glass(&painter, rect, radius::MD, 2);
        let list = rect.shrink(space::S1);
        let scroll = &mut st.scroll[li];
        if components::pointer_over(&ui, list) {
            *scroll -= wheel;
        }
        *scroll = scroll.clamp(0.0, (content_h - list.height()).max(0.0));
        let clip = painter.with_clip_rect(list);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list);
        let mut yy = list.min.y - *scroll;
        let highlighted = if li < st.path.len() {
            Some(st.path[li])
        } else {
            st.active
        };
        for (i, item) in items.iter().enumerate() {
            let row = Rect::from_min_size(pos2(list.min.x, yy), vec2(list.width(), row_h(item)));
            yy = row.max.y;
            if li < st.path.len() && st.path[li] == i {
                parent_row = row;
            }
            if row.max.y < list.min.y || row.min.y > list.max.y {
                continue;
            }
            let text_x = row.min.x + size::MENU_MARK_COL;
            let cy = row.center().y;
            match item {
                Item::Separator => {
                    kit.divider(&clip, row.min.x + space::S2, row.max.x - space::S2, cy)
                }
                Item::Heading(s) => {
                    kit.text(
                        &clip,
                        pos2(text_x, cy + space::S1 / 2.0),
                        Align2::LEFT_CENTER,
                        s.as_str(),
                        Type::Caption,
                        t.text_tertiary,
                    );
                }
                Item::Info(s) => {
                    kit.text(
                        &clip,
                        pos2(text_x, cy),
                        Align2::LEFT_CENTER,
                        s.as_str(),
                        Type::Caption,
                        t.text_secondary,
                    );
                }
                Item::Run {
                    label,
                    shortcut,
                    mark,
                    enabled,
                    action,
                } => {
                    let (kind, selected) = match mark {
                        Mark::Check(on) => (WidgetType::Checkbox, Some(*on)),
                        Mark::Radio(on) => (WidgetType::RadioButton, Some(*on)),
                        Mark::None => (WidgetType::Button, None),
                    };
                    let resp = interact(
                        &mut child,
                        row.intersect(list),
                        Id::new(("menu-item", li, item.label())),
                        kind,
                        label,
                        selected,
                    );
                    if *enabled {
                        kit.row_fill(&clip, row, &resp, highlighted == Some(i));
                    }
                    match mark {
                        Mark::Check(true) => {
                            let r = components::icon_rect(
                                pos2(row.min.x + size::MENU_MARK_COL / 2.0, cy),
                                size::ICON_SM,
                            );
                            kit.icon(&clip, r, Icon::Check, size::ICON_SM, t.accent);
                        }
                        Mark::Radio(true) => {
                            clip.circle_filled(
                                pos2(row.min.x + size::MENU_MARK_COL / 2.0, cy),
                                size::MENU_RADIO_R,
                                t.accent,
                            );
                        }
                        _ => {}
                    }
                    kit.text(
                        &clip,
                        pos2(text_x, cy),
                        Align2::LEFT_CENTER,
                        label.as_str(),
                        Type::Body,
                        if *enabled {
                            t.text_primary
                        } else {
                            t.text_tertiary
                        },
                    );
                    if let Some(k) = shortcut {
                        let g = kit.galley(&clip, *k, Type::Caption, t.text_secondary);
                        let kw = (g.size().x + space::S2 + 2.0).max(size::KEYCAP_H);
                        kit.keycap(
                            &clip,
                            pos2(row.max.x - space::S2 - kw, cy - size::KEYCAP_H / 2.0),
                            k,
                        );
                    }
                    if *enabled {
                        if resp.hovered() && moved {
                            new_path = Some(st.path[..li].to_vec());
                            new_active = Some(Some(i));
                        }
                        if resp.clicked() {
                            run = Some((action.clone(), matches!(mark, Mark::Check(_))));
                        }
                    }
                }
                Item::Sub { label, value, .. } => {
                    let resp = interact(
                        &mut child,
                        row.intersect(list),
                        Id::new(("menu-item", li, item.label())),
                        WidgetType::Button,
                        label,
                        Some(highlighted == Some(i) && li < st.path.len()),
                    );
                    kit.row_fill(&clip, row, &resp, highlighted == Some(i));
                    kit.text(
                        &clip,
                        pos2(text_x, cy),
                        Align2::LEFT_CENTER,
                        label.as_str(),
                        Type::Body,
                        t.text_primary,
                    );
                    let chevron = components::icon_rect(
                        pos2(row.max.x - space::S2 - size::ICON_SM / 2.0, cy),
                        size::ICON_SM,
                    );
                    kit.icon(
                        &clip,
                        chevron,
                        Icon::ChevronRight,
                        size::ICON_SM,
                        t.text_secondary,
                    );
                    if !value.is_empty() {
                        kit.text(
                            &clip,
                            pos2(chevron.min.x - space::S1, cy),
                            Align2::RIGHT_CENTER,
                            value.as_str(),
                            Type::Caption,
                            t.text_tertiary,
                        );
                    }
                    let opened = li < st.path.len() && st.path[li] == i;
                    if (resp.hovered() && moved && !opened) || (resp.clicked() && !opened) {
                        let mut p = st.path[..li].to_vec();
                        p.push(i);
                        new_path = Some(p);
                        new_active = Some(None);
                    }
                }
            }
        }
        components::scroll_indicator(kit, &painter, list, *scroll, content_h);
    }
    if let Some(p) = new_path {
        st.path = p;
    }
    if let Some(a) = new_active {
        st.active = a;
    }
    if let Some((action, keep_open)) = run {
        let now = ui.input(|i| i.time);
        commands::run(model, &action, view, now);
        // Switches stay open so several can be flipped; anything that
        // opened something else (a picker, a card) wins.
        if keep_open && matches!(model.picker, Picker::None) {
            model.picker = Picker::Menu(st);
        }
        ui.ctx().request_repaint();
        return;
    }
    model.picker = Picker::Menu(st);
    // A click anywhere else closes the menu (a title's own click toggles).
    if ui.input(|i| {
        i.pointer.any_click()
            && i.pointer.interact_pos().is_some_and(|p| {
                !rects.iter().any(|r| r.contains(p))
                    && !bar.titles.iter().any(|(_, r)| r.contains(p))
            })
    }) {
        model.picker = Picker::None;
    }
}
