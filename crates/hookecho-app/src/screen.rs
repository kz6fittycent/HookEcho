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
use hookecho_render::palette::Palette;

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
    let over_map = pointer.is_some_and(|p| {
        ui.ctx()
            .layer_id_at(p)
            .is_none_or(|l| l.order == egui::Order::Background)
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

/// Keyboard shortcuts (shown in tooltips).
pub fn keys(ui: &Ui, model: &mut Model, screen: Rect) {
    let size = view_size(screen);
    let typing = matches!(model.picker, Picker::Site { .. });
    ui.input(|i| {
        if i.key_pressed(Key::Escape) {
            model.picker = Picker::None;
            if matches!(model.inspect, Inspect::Pinned(_)) {
                model.inspect = Inspect::Off;
            }
        }
        if typing {
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
            model.picker = Picker::Product { row: 0, col: 0 };
        }
        if matches!(model.picker, Picker::None) {
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
    let legend_rect = legend(kit, model, &painter, screen, m, compact);
    scan_time(kit, model, &painter, screen, m, compact, legend_rect);
    if let Inspect::Hover(at) | Inspect::Pinned(at) = model.inspect {
        inspector(kit, model, &painter, at);
    }
    match model.picker {
        Picker::Site { .. } => pickers::site_picker(ui, kit, model, site_rect),
        Picker::Product { .. } => pickers::product_picker(ui, kit, model, product_rect),
        Picker::None => {}
    }
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
    let (status, dot) = match model.loaded_here() {
        Some(l) if l.archived => {
            let mode = if hookecho_core::level2::vcp_is_clear_air(l.volume.vcp) {
                "Clear air"
            } else {
                "Precipitation"
            };
            (
                format!("Archived scan · VCP {} · {mode}", l.volume.vcp),
                t.accent,
            )
        }
        Some(_) => ("Live".to_string(), t.status_fresh),
        None => ("No data loaded".to_string(), t.text_tertiary),
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
    kit.tooltip(ui, &resp, "Change radar", Some("S"));
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
    let (tilts, source) = match model.loaded_here() {
        Some(l) => {
            let sweep = model.current_sweep().map(|i| &l.volume.sweeps[i]);
            let superres = sweep
                .and_then(|s| s.radials.first().map(|r| r.azimuth_spacing_deg < 0.75))
                .unwrap_or(false);
            (
                l.tilts(),
                if superres {
                    "Level II · Super-res"
                } else {
                    "Level II"
                },
            )
        }
        None => (Vec::new(), "Level II"),
    };
    let line1 = vec![
        (
            model.moment.short_name().to_string(),
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
        matches!(model.picker, Picker::Product { .. }),
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
            Picker::Product { .. } => Picker::None,
            _ => Picker::Product { row: 0, col: 0 },
        };
    }
    rect
}

fn legend(
    kit: &Kit,
    model: &Model,
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
    kit.glass(painter, rect, radius::MD, 1);
    let pad = space::S3;
    let fmt = Fmt(model.config.units);
    let (unit, factor) = fmt.legend_unit(model.moment);
    let palette = Palette::default_for(model.moment);
    let (lo, hi) = (palette.min(), palette.max());
    kit.text(
        painter,
        pos2(rect.min.x + pad, rect.min.y + size::LEGEND_TITLE_Y),
        Align2::LEFT_CENTER,
        model.moment.short_name(),
        Type::Caption,
        t.text_secondary,
    );
    kit.text(
        painter,
        pos2(rect.max.x - pad, rect.min.y + size::LEGEND_TITLE_Y),
        Align2::RIGHT_CENTER,
        unit,
        Type::Caption,
        t.text_tertiary,
    );
    let bar = Rect::from_min_max(
        pos2(rect.min.x + pad, rect.min.y + size::LEGEND_BAR_Y),
        pos2(
            rect.max.x - pad,
            rect.min.y + size::LEGEND_BAR_Y + size::LEGEND_BAR_H,
        ),
    );
    kit.color_bar(painter, bar, |f| palette.sample(lo + (hi - lo) * f));
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
fn scan_time(
    kit: &mut Kit,
    model: &Model,
    painter: &egui::Painter,
    screen: Rect,
    m: f32,
    compact: bool,
    legend: Rect,
) {
    let t = kit.theme.clone();
    let Some(l) = model.loaded_here() else { return };
    let Some(ms) = l.time_ms() else { return };
    let text = model::archive_time(ms);
    let g = kit.galley(painter, text.clone(), Type::LabelTab, t.accent);
    let w = space::S3 + size::ICON_SM + space::S2 + g.size().x + space::S4;
    let h = size::TIMELINE_H - space::S2;
    let mut x = screen.center().x - w / 2.0;
    if !compact && x + w > legend.min.x - space::S2 {
        x = legend.min.x - space::S2 - w;
    }
    let rect = Rect::from_min_size(pos2(x, screen.max.y - m - h), vec2(w, h));
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
    let probe = l.probe(sweep, model.moment, at);
    let fmt = Fmt(model.config.units);
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
