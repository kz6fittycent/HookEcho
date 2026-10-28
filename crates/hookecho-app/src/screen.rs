//! Main screen (docs/DESIGN.md §3): map interaction, chrome and inspector.

use crate::design::components::{self, DotState, Kit, interact, measure, row};
use crate::design::icons::Icon;
use crate::design::tokens::{Type, motion, radius, size, space};
use crate::model::{self, Fmt, Inspect, Model, Picker, TiltState};
use crate::pickers;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, Id, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    WidgetType, pos2, vec2,
};
use hookecho_core::sites::{self, SiteKind};

/// Bands at the top and bottom where chrome lives.
fn chrome_bands(screen: Rect) -> [Rect; 3] {
    let m = margin(screen);
    let compact = screen.width() < size::COMPACT_BREAKPOINT;
    let bottom_h = m
        + size::TIMELINE_H
        + if compact {
            size::LEGEND_H + space::S2
        } else {
            0.0
        };
    [
        Rect::from_min_max(
            screen.min,
            pos2(screen.max.x, screen.min.y + m + size::CHIP_H + space::S2),
        ),
        Rect::from_min_max(
            pos2(screen.min.x, screen.max.y - bottom_h - space::S2),
            screen.max,
        ),
        // The legend card, bottom right.
        Rect::from_min_max(
            pos2(
                screen.max.x - m - size::LEGEND_W - space::S2,
                screen.max.y
                    - m
                    - size::LEGEND_H
                    - space::S2
                    - if compact {
                        size::TIMELINE_H + space::S2
                    } else {
                        0.0
                    },
            ),
            screen.max,
        ),
    ]
}

/// Site markers to draw: (site, screen position, as a clickable pill?).
/// All sites when zoomed out far enough to choose one or while the site
/// picker is open; otherwise just the selected site. Markers never sit under
/// the chrome.
fn markers(model: &Model, screen: Rect) -> Vec<(&'static sites::Site, Pos2, bool)> {
    let size = view_size(screen);
    let show_all = model.camera.km_per_px() > 1.2 || matches!(model.picker, Picker::Site { .. });
    let bands = chrome_bands(screen);
    sites::all()
        .iter()
        .filter(|s| s.kind == SiteKind::Nexrad && (show_all || s.id == model.site.id))
        .filter_map(|s| {
            let p = model.camera.latlon_to_screen(s.lat, s.lon, size);
            let p = pos2(p[0] as f32, p[1] as f32);
            let pill = show_all && s.id != model.site.id;
            (screen.contains(p) && !bands.iter().any(|b| b.contains(p))).then_some((s, p, pill))
        })
        .collect()
}

/// Rects the renderer keeps free of place labels: chrome and site markers.
pub fn label_exclusions(model: &Model, screen: Rect) -> Vec<[f32; 4]> {
    let mut v: Vec<[f32; 4]> = chrome_bands(screen)
        .iter()
        .map(|r| [r.min.x, r.min.y, r.max.x, r.max.y])
        .collect();
    for (_, p, pill) in markers(model, screen) {
        let [w, h, dx] = size::MARKER_LABEL_CLEAR;
        let (hw, hh, off) = if pill {
            (size::SITE_PILL[0] / 2.0, size::SITE_PILL[1] / 2.0, 0.0)
        } else {
            (w, h, dx)
        };
        v.push([p.x - hw + off, p.y - hh, p.x + hw + off, p.y + hh]);
    }
    if model.sheet.is_some() {
        let r = crate::sheet::sheet_rect(screen, margin(screen));
        v.push([r.min.x, r.min.y, r.max.x, r.max.y]);
    }
    v
}

fn margin(screen: Rect) -> f32 {
    if screen.width() < size::COMPACT_BREAKPOINT {
        space::S3
    } else {
        space::S4
    }
}

fn view_size(screen: Rect) -> [f64; 2] {
    [screen.width() as f64, screen.height() as f64]
}

/// Map input: drag to pan (with fling), scroll/pinch to zoom smoothly around
/// the pointer, keyboard zoom. Returns true while motion is still settling.
pub fn map_input(ui: &mut Ui, model: &mut Model, screen: Rect) -> bool {
    let resp = ui.interact(screen, Id::new("map"), Sense::click_and_drag());
    let size = view_size(screen);
    let (dt, scroll, zoom, pointer, velocity) = ui.input(|i| {
        (
            i.stable_dt.min(0.1),
            i.smooth_scroll_delta,
            i.zoom_delta(),
            i.pointer.hover_pos(),
            i.pointer.velocity(),
        )
    });
    // Over the map itself: not over a floating layer, and not over the
    // chrome painted on the map's own layer (last frame's glass panels).
    let over_map = pointer.is_some_and(|p| {
        ui.ctx()
            .layer_id_at(p)
            .is_none_or(|l| l.order == egui::Order::Background)
            && !model
                .chrome
                .iter()
                .any(|r| p.x >= r[0] && p.x <= r[2] && p.y >= r[1] && p.y <= r[3])
    });
    if resp.dragged() {
        let d = resp.drag_delta();
        model.camera.pan(d.x as f64, d.y as f64);
        model.motion.fling = [0.0; 2];
        model.motion.zoom_target = model.camera.scale;
    }
    if resp.drag_stopped() && !model.reduced_motion {
        model.motion.fling = [velocity.x as f64, velocity.y as f64];
    }
    if over_map && matches!(model.picker, Picker::None) {
        let factor = (scroll.y as f64 / 240.0).exp() * zoom as f64;
        if (factor - 1.0).abs() > 1e-4 {
            let anchor =
                pointer.map_or([size[0] / 2.0, size[1] / 2.0], |p| [p.x as f64, p.y as f64]);
            model.motion.zoom_target = (model.motion.zoom_target * factor).clamp(
                hookecho_core::geo::Camera::MIN_SCALE,
                hookecho_core::geo::Camera::MAX_SCALE,
            );
            model.motion.zoom_anchor = anchor;
        }
    }
    // Double-click zooms in on the point (DESIGN §3, parity: map interaction).
    if resp.double_clicked()
        && let Some(p) = pointer
    {
        model.motion.zoom_target =
            (model.camera.scale * 2.0).min(hookecho_core::geo::Camera::MAX_SCALE);
        model.motion.zoom_anchor = [p.x as f64, p.y as f64];
    }
    let mut animating = false;
    // Zoom easing toward the target (critically damped first-order approach).
    let ratio = model.motion.zoom_target / model.camera.scale;
    if (ratio - 1.0).abs() > 1e-3 {
        let k = if model.reduced_motion {
            1.0
        } else {
            1.0 - (-(dt as f64) / motion::ZOOM_TAU as f64).exp()
        };
        model
            .camera
            .zoom_at(ratio.powf(k), model.motion.zoom_anchor, size);
        animating = true;
    } else {
        model.motion.zoom_target = model.camera.scale;
    }
    // Fling decay.
    let f = model.motion.fling;
    if f[0].abs() + f[1].abs() > 5.0 {
        model.camera.pan(f[0] * dt as f64, f[1] * dt as f64);
        let decay = 0.5f64.powf(dt as f64 / motion::FLING_HALF_LIFE as f64);
        model.motion.fling = [f[0] * decay, f[1] * decay];
        animating = true;
    } else {
        model.motion.fling = [0.0; 2];
    }
    // Inspector follows the pointer at rest; hidden while dragging.
    if !matches!(model.inspect, Inspect::Pinned(_)) {
        model.inspect = match pointer {
            Some(p) if over_map && !resp.dragged() && matches!(model.picker, Picker::None) => {
                Inspect::Hover(
                    model
                        .camera
                        .screen_to_latlon([p.x as f64, p.y as f64], size),
                )
            }
            _ => Inspect::Off,
        };
    }
    animating
}

/// A key press whose own modifiers pass `f` (the event's modifiers, not
/// the frame's held state, so a quick chord isn't missed).
fn pressed_with(i: &egui::InputState, key: Key, f: impl Fn(egui::Modifiers) -> bool) -> bool {
    i.events.iter().any(|e| {
        matches!(e, egui::Event::Key { key: k, pressed: true, modifiers, .. } if *k == key && f(*modifiers))
    })
}

/// Keyboard shortcuts (shown in tooltips and the `?` sheet).
pub fn keys(ui: &Ui, model: &mut Model, screen: Rect) {
    let size = view_size(screen);
    let typing = matches!(model.picker, Picker::Site { .. } | Picker::Command { .. });
    ui.input(|i| {
        if i.key_pressed(Key::Escape) {
            // One layer at a time: a picker first, then the sheet.
            if matches!(model.picker, Picker::None) {
                model.sheet = None;
            }
            model.picker = Picker::None;
            if matches!(model.inspect, Inspect::Pinned(_)) {
                model.inspect = Inspect::Off;
            }
        }
        if pressed_with(i, Key::K, |m| m.command) {
            if matches!(model.picker, Picker::Command { .. }) {
                model.picker = Picker::None;
            } else {
                crate::overlays::open_command(model);
            }
            return;
        }
        if typing {
            return;
        }
        if i.key_pressed(Key::Questionmark) || pressed_with(i, Key::Slash, |m| m.shift) {
            model.picker = match model.picker {
                Picker::Shortcuts => Picker::None,
                _ => Picker::Shortcuts,
            };
            return;
        }
        if i.key_pressed(Key::S) {
            model.picker = Picker::Site {
                query: String::new(),
                selected: 0,
                scroll: 0.0,
            };
        }
        if i.key_pressed(Key::P) {
            model.picker = Picker::Product;
        }
        if i.key_pressed(Key::L) {
            model.toggle_sheet();
        }
        if matches!(model.picker, Picker::None) {
            if i.key_pressed(Key::Space) {
                model.toggle_play(i.time);
            }
            if i.key_pressed(Key::Comma) {
                model.looping.playing = false;
                if i.modifiers.shift {
                    model.show_pos(0)
                } else {
                    model.step_frame(-1)
                }
            }
            if i.key_pressed(Key::Period) {
                model.looping.playing = false;
                if i.modifiers.shift {
                    model.show_pos(usize::MAX)
                } else {
                    model.step_frame(1)
                }
            }
            if i.key_pressed(Key::Home) {
                model.looping.playing = false;
                model.show_pos(0);
            }
            if i.key_pressed(Key::End) {
                model.looping.playing = false;
                model.show_pos(usize::MAX);
            }
            if i.key_pressed(Key::OpenBracket) {
                model.step_tilt(-1);
            }
            if i.key_pressed(Key::CloseBracket) {
                model.step_tilt(1);
            }
            if i.key_pressed(Key::R) {
                model.moment = hookecho_core::Moment::Reflectivity;
            }
            if i.key_pressed(Key::V) {
                model.moment = hookecho_core::Moment::Velocity;
            }
            if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
                model.motion.zoom_target =
                    (model.motion.zoom_target * 1.6).min(hookecho_core::geo::Camera::MAX_SCALE);
                model.motion.zoom_anchor = [size[0] / 2.0, size[1] / 2.0];
            }
            if i.key_pressed(Key::Minus) {
                model.motion.zoom_target =
                    (model.motion.zoom_target / 1.6).max(hookecho_core::geo::Camera::MIN_SCALE);
                model.motion.zoom_anchor = [size[0] / 2.0, size[1] / 2.0];
            }
            if i.key_pressed(Key::I) {
                model.inspect = match model.inspect {
                    Inspect::Hover(p) => Inspect::Pinned(p),
                    _ => Inspect::Off,
                };
            }
        }
    });
}

/// Paints the screen. `scene`: the rendered map texture.
pub fn paint(ui: &mut Ui, kit: &mut Kit, model: &mut Model, scene: egui::TextureId) {
    let screen = kit.screen;
    let painter = ui.painter().clone();
    painter.image(
        scene,
        screen,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    home_marker(kit, model, &painter);
    site_markers(ui, kit, model, &painter);
    let m = margin(screen);
    let site_rect = site_chip(
        ui,
        kit,
        model,
        &painter,
        pos2(screen.min.x + m, screen.min.y + m),
    );
    let product_rect = product_chip(
        ui,
        kit,
        model,
        &painter,
        pos2(site_rect.max.x + space::S2, screen.min.y + m),
    );
    let compact = screen.width() < size::COMPACT_BREAKPOINT;
    let legend_rect = legend(ui, kit, model, &painter, screen, m, compact);
    timeline(ui, kit, model, &painter, screen, m, compact, legend_rect);
    if let Inspect::Hover(at) | Inspect::Pinned(at) = model.inspect {
        inspector(kit, model, &painter, at);
    }
    let search_rect = top_right(ui, kit, model, &painter, screen, m);
    crate::sheet::side_sheet(ui, kit, model, crate::sheet::sheet_rect(screen, m));
    match model.picker {
        Picker::Site { .. } => pickers::site_picker(ui, kit, model, site_rect),
        Picker::Product => pickers::product_picker(ui, kit, model, product_rect),
        Picker::Palette => pickers::palette_picker(ui, kit, model, legend_rect),
        Picker::Command { .. } => {
            crate::overlays::command_palette(ui, kit, model, view_size(screen), search_rect)
        }
        Picker::Shortcuts => crate::overlays::shortcuts_sheet(ui, kit, model, search_rect),
        Picker::None => {}
    }
    if let Some((text, _)) = &model.notice {
        notice(kit, &painter, screen, m, text);
    } else if matches!(model.picker, Picker::None) && model.no_precipitation() {
        let text = format!("Little or no rain or snow near {} right now", model.site.id);
        notice(kit, &painter, screen, m, &text);
    }
}

/// Top-right cluster (DESIGN.md §3.2): command palette and side sheet
/// buttons on one glass chip. Returns the search button's rect (the
/// palette's anchor).
fn top_right(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &egui::Painter,
    screen: Rect,
    m: f32,
) -> Rect {
    let h = size::CHIP_H;
    let b = size::ICON_BUTTON;
    let inset = (h - b) / 2.0;
    let w = 2.0 * b + space::S1 + 2.0 * inset;
    let rect = Rect::from_min_size(pos2(screen.max.x - m - w, screen.min.y + m), vec2(w, h));
    kit.glass(painter, rect, radius::MD, 1);
    let search = Rect::from_min_size(rect.min + vec2(inset, inset), vec2(b, b));
    let resp = components::icon_button(
        ui,
        kit,
        painter,
        search,
        Id::new("command-button"),
        Icon::Search,
        "Search everything",
        Some("Ctrl K"),
        matches!(model.picker, Picker::Command { .. }),
    );
    if resp.clicked() {
        if matches!(model.picker, Picker::Command { .. }) {
            model.picker = Picker::None;
        } else {
            crate::overlays::open_command(model);
        }
    }
    let button = Rect::from_min_size(search.min + vec2(b + space::S1, 0.0), vec2(b, b));
    let resp = components::icon_button(
        ui,
        kit,
        painter,
        button,
        Id::new("sheet-button"),
        Icon::Layers,
        "Layers, colors and settings",
        Some("L"),
        model.sheet.is_some(),
    );
    if resp.clicked() {
        model.toggle_sheet();
    }
    search
}

/// A short message centered above the bottom chrome (palette imports).
fn notice(kit: &Kit, painter: &egui::Painter, screen: Rect, m: f32, text: &str) {
    let g = kit.galley(
        painter,
        text.to_string(),
        Type::Label,
        kit.theme.text_primary,
    );
    let w = (g.size().x + 2.0 * space::S4).min(screen.width() - 2.0 * m);
    let bottom = screen.max.y - m - size::TIMELINE_H - space::S3;
    let rect = Rect::from_min_max(
        pos2(screen.center().x - w / 2.0, bottom - size::NOTICE_H),
        pos2(screen.center().x + w / 2.0, bottom),
    );
    kit.glass(painter, rect, radius::MD, 2);
    kit.text(
        painter,
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        Type::Label,
        kit.theme.text_primary,
    );
}

/// The home point (set from the command palette), as a pin whose tip sits
/// on the spot. Hidden under the chrome like the site markers.
fn home_marker(kit: &mut Kit, model: &Model, painter: &egui::Painter) {
    let Some((lat, lon)) = model.config.home else {
        return;
    };
    let p = model
        .camera
        .latlon_to_screen(lat, lon, view_size(kit.screen));
    let tip = pos2(p[0] as f32, p[1] as f32);
    let pin = Rect::from_center_size(
        tip - egui::Vec2::Y * (size::ICON_MD / 2.0),
        vec2(size::ICON_MD, size::ICON_MD),
    );
    if !kit.screen.contains(tip) || chrome_bands(kit.screen).iter().any(|b| b.intersects(pin)) {
        return;
    }
    kit.icon(
        painter,
        pin,
        Icon::MapPin,
        size::ICON_MD,
        kit.theme.map_label,
    );
}

/// Radar site markers: always for the selected site; all sites when zoomed
/// out far enough to choose one, or while the site picker is open.
fn site_markers(ui: &mut Ui, kit: &mut Kit, model: &mut Model, painter: &egui::Painter) {
    let t = kit.theme;
    let size = view_size(kit.screen);
    let mut clicked = None;
    for (s, p, pill) in markers(model, kit.screen) {
        let current = s.id == model.site.id;
        if pill {
            let r = Rect::from_center_size(p, size::SITE_PILL.into());
            let resp = interact(
                ui,
                r,
                Id::new(("site-marker", &s.id)),
                WidgetType::Button,
                &format!("{} {}", s.id, s.name),
                None,
            );
            let g = kit.galley(painter, s.id.clone(), Type::Caption, t.text_secondary);
            let pill = Rect::from_center_size(p, g.size() + vec2(space::S3, space::S1 + 2.0));
            painter.rect(
                pill,
                CornerRadius::same(radius::SM),
                t.glass_fill,
                Stroke::new(components::hairline(painter.ctx()), t.glass_stroke),
                StrokeKind::Inside,
            );
            kit.state_fill(painter, pill, radius::SM, &resp, false);
            painter.galley(
                pill.center() - g.size() / 2.0,
                g,
                if resp.hovered() {
                    t.text_primary
                } else {
                    t.text_secondary
                },
            );
            if resp.clicked() {
                clicked = Some(s);
            }
        } else {
            kit.ring_marker(painter, p, if current { t.accent } else { t.map_label });
            kit.text(
                painter,
                p + vec2(size::MARKER_R + space::S1, 0.0),
                Align2::LEFT_CENTER,
                s.id.clone(),
                Type::Caption,
                t.map_label,
            );
        }
    }
    if let Some(s) = clicked {
        model.select_site(s, size);
        model.picker = Picker::None;
    }
}

fn site_chip(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &egui::Painter,
    at: Pos2,
) -> Rect {
    let t = kit.theme.clone();
    let pad = space::S3 + 2.0;
    let line1 = vec![
        (model.site.id.clone(), Type::Label, t.text_primary, 0.0),
        (
            model.site.name.clone(),
            Type::Body,
            t.text_secondary,
            size::CHIP_TEXT_GAP,
        ),
    ];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let (status, kind) = model.status_line(now_ms);
    let dot = match kind {
        model::StatusKind::Fresh => t.status_fresh,
        model::StatusKind::Aging => t.status_aging,
        model::StatusKind::Stale => t.status_stale,
        model::StatusKind::Archive => t.accent,
        model::StatusKind::Muted => t.text_tertiary,
    };
    let line2 = vec![(
        status,
        Type::Caption,
        t.text_secondary,
        size::STATUS_DOT + size::CHIP_TEXT_GAP,
    )];
    let text_x = pad + size::ICON_SM + space::S2;
    let w = text_x
        + measure(kit, painter, &line1).max(measure(kit, painter, &line2))
        + space::S2
        + size::ICON_SM
        + pad;
    let rect = Rect::from_min_size(at, vec2(w, size::CHIP_H));
    let resp = interact(
        ui,
        rect,
        Id::new("site-chip"),
        WidgetType::Button,
        &format!("Radar: {} {}", model.site.id, model.site.name),
        None,
    );
    kit.glass(painter, rect, radius::MD, 1);
    kit.state_fill(
        painter,
        rect,
        radius::MD,
        &resp,
        matches!(model.picker, Picker::Site { .. }),
    );
    let (y1, y2) = (
        rect.min.y + size::CHIP_LINE1_Y,
        rect.min.y + size::CHIP_LINE2_Y,
    );
    kit.icon(
        painter,
        components::icon_rect(
            pos2(rect.min.x + pad + size::ICON_SM / 2.0, y1),
            size::ICON_SM,
        ),
        Icon::Radar,
        size::ICON_SM,
        t.text_secondary,
    );
    row(kit, painter, rect.min.x + text_x, y1, &line1);
    kit.status_dot(
        painter,
        pos2(rect.min.x + text_x + size::STATUS_DOT / 2.0, y2),
        dot,
    );
    row(kit, painter, rect.min.x + text_x, y2, &line2);
    kit.icon(
        painter,
        components::icon_rect(
            pos2(rect.max.x - pad - size::ICON_SM / 2.0, y1),
            size::ICON_SM,
        ),
        Icon::ChevronDown,
        size::ICON_SM,
        t.text_secondary,
    );
    kit.focus_ring(painter, rect, radius::MD, &resp);
    let tip = match model.loaded_here() {
        Some(l) => format!("Change radar · scan pattern VCP {}", l.volume.vcp),
        None => "Change radar".to_string(),
    };
    kit.tooltip(ui, &resp, &tip, Some("S"));
    if resp.clicked() {
        model.picker = match model.picker {
            Picker::Site { .. } => Picker::None,
            _ => Picker::Site {
                query: String::new(),
                selected: 0,
                scroll: 0.0,
            },
        };
    }
    rect
}

fn product_chip(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &egui::Painter,
    at: Pos2,
) -> Rect {
    let t = kit.theme.clone();
    let pad = space::S3 + 2.0;
    let tilts = model.loaded_here().map(|l| l.tilts()).unwrap_or_default();
    // Plain words on the chip: what the product shows (DESIGN §3.1).
    let source = model::product_label(model.moment).1;
    let line1 = vec![
        (
            model::product_label(model.moment).0.to_string(),
            Type::Label,
            t.text_primary,
            0.0,
        ),
        (
            format!("{:.1}°", model.tilt_deg),
            Type::LabelTab,
            t.text_secondary,
            size::CHIP_TEXT_GAP,
        ),
    ];
    let line2 = vec![(source.to_string(), Type::Caption, t.text_tertiary, 0.0)];
    let dots_w = tilts.len() as f32 * size::SWEEP_DOT_STEP;
    let w = pad
        + measure(kit, painter, &line1).max(measure(kit, painter, &line2) + space::S3 + dots_w)
        + space::S2
        + size::ICON_SM
        + pad;
    let rect = Rect::from_min_size(at, vec2(w, size::CHIP_H));
    let resp = interact(
        ui,
        rect,
        Id::new("product-chip"),
        WidgetType::Button,
        &format!(
            "Product: {} {:.1} degrees",
            model.moment.name(),
            model.tilt_deg
        ),
        None,
    );
    kit.glass(painter, rect, radius::MD, 1);
    kit.state_fill(
        painter,
        rect,
        radius::MD,
        &resp,
        matches!(model.picker, Picker::Product),
    );
    let (y1, y2) = (
        rect.min.y + size::CHIP_LINE1_Y,
        rect.min.y + size::CHIP_LINE2_Y,
    );
    row(kit, painter, rect.min.x + pad, y1, &line1);
    let end = row(kit, painter, rect.min.x + pad, y2, &line2);
    // Sweep strip: one dot per tilt in the VCP (DESIGN §3.1).
    let mut x = end + space::S3;
    for tilt in &tilts {
        let current = (tilt.deg - model.tilt_deg).abs() < 0.05;
        let state = match tilt.state {
            TiltState::In => DotState::In,
            TiltState::Arriving => DotState::Arriving,
            TiltState::Pending => DotState::Pending,
        };
        kit.sweep_dot(painter, pos2(x + size::SWEEP_DOT_R, y2), state, current);
        x += size::SWEEP_DOT_STEP;
    }
    kit.icon(
        painter,
        components::icon_rect(
            pos2(rect.max.x - pad - size::ICON_SM / 2.0, y1),
            size::ICON_SM,
        ),
        Icon::ChevronDown,
        size::ICON_SM,
        t.text_secondary,
    );
    kit.focus_ring(painter, rect, radius::MD, &resp);
    kit.tooltip(ui, &resp, "Change product or tilt", Some("P"));
    if resp.clicked() {
        model.picker = match model.picker {
            Picker::Product => Picker::None,
            _ => Picker::Product,
        };
    }
    rect
}

#[allow(clippy::too_many_arguments)]
fn legend(
    ui: &mut Ui,
    kit: &Kit,
    model: &mut Model,
    painter: &egui::Painter,
    screen: Rect,
    m: f32,
    compact: bool,
) -> Rect {
    let t = kit.theme;
    let bottom = if compact {
        screen.max.y - m - size::TIMELINE_H - space::S2
    } else {
        screen.max.y - m
    };
    let rect = Rect::from_min_max(
        pos2(screen.max.x - m - size::LEGEND_W, bottom - size::LEGEND_H),
        pos2(screen.max.x - m, bottom),
    );
    // The legend opens the palette picker.
    let resp = interact(
        ui,
        rect,
        Id::new("legend"),
        WidgetType::Button,
        "Color palette",
        None,
    );
    kit.glass(painter, rect, radius::MD, 1);
    kit.state_fill(
        painter,
        rect,
        radius::MD,
        &resp,
        matches!(model.picker, Picker::Palette),
    );
    kit.focus_ring(painter, rect, radius::MD, &resp);
    kit.tooltip(ui, &resp, "Change colors", None);
    if resp.clicked() {
        model.picker = match model.picker {
            Picker::Palette => Picker::None,
            _ => Picker::Palette,
        };
    }
    let pad = space::S3;
    let fmt = Fmt::of(&model.config);
    let (unit, factor) = fmt.legend_unit(model.moment);
    let palette = model.palettes.active(&model.config, model.moment).clone();
    let (lo, hi) = (palette.min(), palette.max());
    kit.text(
        painter,
        pos2(rect.min.x + pad, rect.min.y + size::LEGEND_TITLE_Y),
        Align2::LEFT_CENTER,
        model.moment.short_name(),
        Type::Caption,
        t.text_secondary,
    );
    let bar = Rect::from_min_max(
        pos2(rect.min.x + pad, rect.min.y + size::LEGEND_BAR_Y),
        pos2(
            rect.max.x - pad,
            rect.min.y + size::LEGEND_BAR_Y + size::LEGEND_BAR_H,
        ),
    );
    // Hovering the legend reads out the value under the pointer (DESIGN §3.4).
    let hover_x = resp.hover_pos().map(|p| p.x.clamp(bar.min.x, bar.max.x));
    let header = match hover_x {
        Some(x) => {
            let v = lo + (hi - lo) * (x - bar.min.x) / bar.width();
            let (text, u) = fmt.value(model.moment, v);
            (format!("{text} {u}").trim().to_string(), t.text_primary)
        }
        None => (unit.to_string(), t.text_tertiary),
    };
    kit.text(
        painter,
        pos2(rect.max.x - pad, rect.min.y + size::LEGEND_TITLE_Y),
        Align2::RIGHT_CENTER,
        header.0,
        Type::CaptionTab,
        header.1,
    );
    kit.color_bar(painter, bar, |f| palette.sample(lo + (hi - lo) * f));
    if let Some(x) = hover_x {
        kit.bar_marker(painter, bar, x);
    }
    // About five evenly spaced, round-numbered ticks in display units.
    let (dlo, dhi) = (lo * factor, hi * factor);
    let step = nice_step((dhi - dlo) / size::LEGEND_TICKS);
    let mut v = (dlo / step).ceil() * step;
    while v <= dhi + 1e-3 {
        let x = bar.min.x + (v - dlo) / (dhi - dlo) * bar.width();
        let label = if step < 1.0 {
            format!("{v:.1}")
        } else {
            format!("{v:.0}")
        };
        let r = kit
            .galley(painter, label.clone(), Type::CaptionTab, t.text_tertiary)
            .size()
            .x
            / 2.0;
        if x - r >= rect.min.x + 2.0 && x + r <= rect.max.x - 2.0 {
            kit.text(
                painter,
                pos2(x, bar.max.y + size::LEGEND_TICK_GAP),
                Align2::CENTER_CENTER,
                label,
                Type::CaptionTab,
                t.text_tertiary,
            );
        }
        v += step;
    }
    rect
}

fn nice_step(raw: f32) -> f32 {
    let mag = 10f32.powf(raw.log10().floor());
    let n = raw / mag;
    let s = if n < 1.5 {
        1.0
    } else if n < 3.5 {
        2.0
    } else if n < 7.5 {
        5.0
    } else {
        10.0
    };
    s * mag
}

/// Phase 1 stands in for the timeline with the scan-time chip; the loop
/// controls arrive with real-time data in Phase 2.
/// The bottom timeline (DESIGN.md §3.3): play/pause, a scrubber with one
/// tick per scan spaced by real time, the frame's time, speed, and Live.
/// With nothing to loop (archived or sample data), a compact time chip.
#[allow(clippy::too_many_arguments)]
fn timeline(
    ui: &mut Ui,
    kit: &mut Kit,
    model: &mut Model,
    painter: &egui::Painter,
    screen: Rect,
    m: f32,
    compact: bool,
    legend: Rect,
) {
    let t = kit.theme.clone();
    let Some(shown) = model.shown() else { return };
    let archived = shown.archived;
    let sweep_end = model
        .current_sweep()
        .and_then(|i| shown.volume.sweeps[i].time_range_ms())
        .map(|r| r.1);
    let label = if archived {
        let Some(ms) = shown.time_ms() else { return };
        model::archive_time(ms)
    } else {
        let Some(ms) = sweep_end.or(shown.newest_ms()) else {
            return;
        };
        model::clock_time(ms, model.config.utc)
    };
    let positions = model.timeline();
    let h = size::TIMELINE_H;
    let y = screen.max.y - m - h;
    let place = |w: f32| {
        let mut x = screen.center().x - w / 2.0;
        if !compact && x + w > legend.min.x - space::S2 {
            x = (legend.min.x - space::S2 - w).max(screen.min.x + m);
        }
        Rect::from_min_size(
            pos2(x, y),
            vec2(
                w.min(legend.min.x - space::S2 - x)
                    .max(w.min(screen.width() - 2.0 * m)),
                h,
            ),
        )
    };
    if positions.len() < 2 {
        // Nothing to animate: just say when this scan is from.
        let g = kit.galley(painter, label, Type::LabelTab, t.accent);
        let w = space::S3 + size::ICON_SM + space::S2 + g.size().x + space::S4;
        let rect = place(w).shrink2(egui::Vec2::Y * space::S1);
        kit.glass(painter, rect, radius::MD, 1);
        let icon = components::icon_rect(
            pos2(
                rect.min.x + space::S3 + size::ICON_SM / 2.0,
                rect.center().y,
            ),
            size::ICON_SM,
        );
        kit.icon(painter, icon, Icon::Clock, size::ICON_SM, t.accent);
        painter.galley(
            pos2(icon.max.x + space::S2, rect.center().y - g.size().y / 2.0),
            g,
            t.accent,
        );
        return;
    }
    let rect = if compact {
        Rect::from_min_size(pos2(screen.min.x + m, y), vec2(screen.width() - 2.0 * m, h))
    } else {
        place(size::TIMELINE_MAX_W.min(screen.width() - 2.0 * m))
    };
    kit.glass(painter, rect, radius::MD, 1);
    let b = size::ICON_BUTTON;
    let pad = (h - b) / 2.0;
    let cy = rect.center().y;
    let now = ui.input(|i| i.time);

    // Play / pause.
    let play_rect = Rect::from_min_size(rect.min + vec2(pad, pad), vec2(b, b));
    let playing = model.looping.playing;
    let label_play = if playing { "Pause loop" } else { "Play loop" };
    let resp = components::icon_button(
        ui,
        kit,
        painter,
        play_rect,
        Id::new("play"),
        if playing { Icon::Pause } else { Icon::Play },
        label_play,
        Some("Space"),
        playing,
    );
    if resp.clicked() {
        model.toggle_play(now);
    }

    // Right group: Live, speed, time.
    let live_pos = positions.len() - 1;
    let at_live = model.timeline_pos() == live_pos;
    let live = Rect::from_min_size(
        pos2(
            rect.max.x - pad - size::LIVE_BUTTON[0],
            cy - size::LIVE_BUTTON[1] / 2.0,
        ),
        size::LIVE_BUTTON.into(),
    );
    let live_resp = interact(
        ui,
        live,
        Id::new("live"),
        WidgetType::Button,
        "Jump to the latest scan",
        Some(at_live),
    );
    if at_live {
        painter.rect_filled(live, CornerRadius::same(radius::SM), t.selected);
    } else {
        kit.state_fill(painter, live, radius::SM, &live_resp, false);
    }
    kit.status_dot(
        painter,
        pos2(live.min.x + size::LIVE_DOT_X, cy),
        if at_live { t.accent } else { t.text_tertiary },
    );
    // Accent text only on plain glass (contrast test): primary on the fill.
    kit.text(
        painter,
        pos2(live.min.x + size::LIVE_TEXT_X, cy),
        Align2::LEFT_CENTER,
        "Live",
        Type::Caption,
        if at_live {
            t.text_primary
        } else {
            t.text_secondary
        },
    );
    kit.focus_ring(painter, live, radius::SM, &live_resp);
    kit.tooltip(ui, &live_resp, "Latest scan", Some("End"));
    if live_resp.clicked() {
        model.looping.playing = false;
        model.show_pos(live_pos);
    }
    let speed = Rect::from_min_size(
        pos2(
            live.min.x - space::S2 - size::SPEED_BUTTON[0],
            cy - size::SPEED_BUTTON[1] / 2.0,
        ),
        size::SPEED_BUTTON.into(),
    );
    let speed_resp = interact(
        ui,
        speed,
        Id::new("speed"),
        WidgetType::Button,
        "Loop speed",
        None,
    );
    kit.state_fill(painter, speed, radius::SM, &speed_resp, false);
    let sp = model.config.loop_speed;
    let sp_label = if sp < 0.75 {
        "½×"
    } else if sp < 1.5 {
        "1×"
    } else {
        "2×"
    };
    kit.text(
        painter,
        speed.center(),
        Align2::CENTER_CENTER,
        sp_label,
        Type::CaptionTab,
        t.text_secondary,
    );
    kit.focus_ring(painter, speed, radius::SM, &speed_resp);
    kit.tooltip(ui, &speed_resp, "Loop speed", None);
    if speed_resp.clicked() {
        model.config.loop_speed = if sp < 0.75 {
            1.0
        } else if sp < 1.5 {
            2.0
        } else {
            0.5
        };
    }
    let time = kit.text(
        painter,
        pos2(speed.min.x - space::S3, cy),
        Align2::RIGHT_CENTER,
        label,
        Type::LabelTab,
        t.text_primary,
    );

    // Scrubber.
    let track = Rect::from_min_max(
        pos2(play_rect.max.x + space::S3, cy - size::TRACK_H / 2.0),
        pos2(time.min.x - space::S5, cy + size::TRACK_H / 2.0),
    );
    let hit = track.expand2(vec2(size::TRACK_INSET, h / 2.0 - size::TRACK_H));
    let scrub = ui.interact(hit, Id::new("scrubber"), Sense::click_and_drag());
    scrub.widget_info(|| egui::WidgetInfo::labeled(WidgetType::Slider, true, "Loop position"));
    let (t0, t1) = (positions[0].0 as f64, positions[live_pos].0 as f64);
    let span = (t1 - t0).max(1.0);
    let x_of = |ms: i64| {
        track.min.x
            + size::TRACK_INSET
            + ((ms as f64 - t0) / span) as f32 * (track.width() - 2.0 * size::TRACK_INSET)
    };
    if (scrub.clicked() || scrub.dragged())
        && let Some(p) = scrub.interact_pointer_pos()
    {
        let nearest = (0..positions.len()).min_by(|&a, &b| {
            (x_of(positions[a].0) - p.x)
                .abs()
                .total_cmp(&(x_of(positions[b].0) - p.x).abs())
        });
        if let Some(n) = nearest {
            model.looping.playing = false;
            model.show_pos(n);
        }
    }
    let pos = model.timeline_pos();
    painter.rect_filled(track, CornerRadius::same(radius::HAIR), t.track);
    let cur_x = x_of(positions[pos.min(live_pos)].0);
    painter.rect_filled(
        Rect::from_min_max(track.min, pos2(cur_x, track.max.y)),
        CornerRadius::same(radius::HAIR),
        t.accent.gamma_multiply(0.55),
    );
    for (i, &(ms, _)) in positions.iter().enumerate() {
        let tick = Rect::from_center_size(pos2(x_of(ms), cy), size::TICK.into());
        painter.rect_filled(
            tick,
            CornerRadius::same(radius::HAIR),
            if i <= pos {
                t.text_secondary
            } else {
                t.text_tertiary
            },
        );
    }
    painter.rect_filled(
        Rect::from_center_size(pos2(cur_x, cy), size::PLAYHEAD.into()),
        CornerRadius::same(radius::XS),
        t.accent,
    );
    kit.focus_ring(painter, track, radius::SM, &scrub);
}

fn inspector(kit: &mut Kit, model: &Model, painter: &egui::Painter, at: (f64, f64)) {
    let t = kit.theme.clone();
    let Some(l) = model.loaded_here() else { return };
    let Some(sweep) = model.current_sweep() else {
        return;
    };
    let size = view_size(kit.screen);
    let p = model.camera.latlon_to_screen(at.0, at.1, size);
    let p = pos2(p[0] as f32, p[1] as f32);
    let mut probe = l.probe(sweep, model.moment, at);
    // Same rule as the map: no dual-pol value where the mask hides it.
    if model.config.mask_dualpol
        && model::is_dualpol(model.moment)
        && l.probe(sweep, hookecho_core::Moment::Reflectivity, at)
            .value
            .is_none_or(|v| v < model::DUALPOL_MIN_DBZ)
    {
        (probe.raw, probe.value) = (None, None);
    }
    let fmt = Fmt::of(&model.config);
    let (value, unit, note) = match (probe.raw, probe.value) {
        (Some(1), _) => ("RF".to_string(), "", Some("Range folded")),
        (_, Some(v)) => {
            let (s, u) = fmt.value(model.moment, v);
            let note = match model.moment {
                hookecho_core::Moment::Velocity if v >= 0.0 => Some("Away from radar"),
                hookecho_core::Moment::Velocity => Some("Toward radar"),
                _ => None,
            };
            (s, u, note)
        }
        _ => ("—".to_string(), "", Some("No echo")),
    };
    kit.reticle(painter, p);
    let rows = [
        ("Beam height".to_string(), fmt.height(probe.height_km)),
        (
            format!("From {}", l.site.id),
            format!(
                "{}  {:03.0}°",
                fmt.distance(probe.ground_km),
                probe.bearing_deg
            ),
        ),
    ];
    let pad = space::S3;
    let w = size::INSPECTOR_W;
    let h = pad
        + size::INSPECTOR_VALUE_H
        + if note.is_some() {
            size::INSPECTOR_NOTE_H
        } else {
            0.0
        }
        + space::S2
        + rows.len() as f32 * size::INSPECTOR_ROW_H
        + pad
        - space::S1;
    let screen = kit.screen;
    let off = size::INSPECTOR_OFFSET;
    let mut min = p + egui::Vec2::splat(off);
    if min.x + w > screen.max.x - space::S4 {
        min.x = p.x - off - w;
    }
    if min.y + h > screen.max.y - space::S4 - size::TIMELINE_H {
        min.y = p.y - off - h;
    }
    let rect = Rect::from_min_size(min, vec2(w, h));
    kit.glass(painter, rect, radius::MD, 2);
    let mut y = rect.min.y + pad + 14.0;
    let end = row(
        kit,
        painter,
        rect.min.x + pad,
        y,
        &[(value, Type::Value, t.text_primary, 0.0)],
    );
    kit.text(
        painter,
        pos2(end + 5.0, y + 3.0),
        Align2::LEFT_CENTER,
        unit,
        Type::Body,
        t.text_secondary,
    );
    // Arrow toward the radar.
    let a = ((probe.bearing_deg + 180.0) % 360.0).to_radians() as f32;
    let c = pos2(rect.max.x - pad - 8.0, y);
    let dir = vec2(a.sin(), -a.cos());
    let perp = vec2(-dir.y, dir.x);
    painter.add(egui::Shape::convex_polygon(
        vec![
            c + dir * 7.0,
            c - dir * 5.0 + perp * 5.0,
            c - dir * 2.5,
            c - dir * 5.0 - perp * 5.0,
        ],
        t.text_secondary,
        Stroke::NONE,
    ));
    y += 18.0;
    if let Some(n) = note {
        kit.text(
            painter,
            pos2(rect.min.x + pad, y + 4.0),
            Align2::LEFT_CENTER,
            n,
            Type::Caption,
            t.text_secondary,
        );
        y += 16.0;
    }
    y += space::S2;
    painter.line_segment(
        [
            pos2(rect.min.x + pad, y - 4.0),
            pos2(rect.max.x - pad, y - 4.0),
        ],
        Stroke::new(components::hairline(painter.ctx()), t.glass_stroke),
    );
    for (k, v) in rows {
        y += 10.0;
        kit.text(
            painter,
            pos2(rect.min.x + pad, y),
            Align2::LEFT_CENTER,
            k,
            Type::Caption,
            t.text_tertiary,
        );
        kit.text(
            painter,
            pos2(rect.max.x - pad, y),
            Align2::RIGHT_CENTER,
            v,
            Type::CaptionTab,
            t.text_primary,
        );
        y += 10.0;
    }
}
