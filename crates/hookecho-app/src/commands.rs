//! The command palette's commands (docs/DESIGN.md §3.8): every site,
//! product, layer, action and setting, reachable by typing or by browsing a
//! group (the tabs under the search field). Built from the model each time
//! the palette opens or the query changes; no egui here.

use crate::model::{self, Model, PRODUCTS, Picker, SheetTab};
use hookecho_core::Moment;
use hookecho_core::config::{Speed, ThemePref, Units};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Sites,
    Products,
    Layers,
    Actions,
    Settings,
}

impl Group {
    /// The tabs under the search field, after "All".
    pub const ALL: [Group; 5] = [
        Group::Sites,
        Group::Products,
        Group::Layers,
        Group::Actions,
        Group::Settings,
    ];

    /// The tab's name.
    pub fn tab(self) -> &'static str {
        match self {
            Group::Sites => "Radars",
            Group::Products => "Products",
            Group::Layers => "Layers",
            Group::Actions => "Actions",
            Group::Settings => "Settings",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Group::Sites => "RADARS",
            Group::Products => "PRODUCTS",
            Group::Layers => "LAYERS",
            Group::Actions => "ACTIONS",
            Group::Settings => "SETTINGS",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Layer {
    Counties,
    Roads,
    Labels,
    Warnings,
    Watches,
    Discussions,
    StationPlots,
    StationPanel,
    Aurora,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Site(&'static str),
    Product(Moment),
    TiltUp,
    TiltDown,
    Layer(Layer),
    PlayPause,
    Live,
    SitePicker,
    NearestRadar,
    SetHome,
    UseMyLocation,
    ProductPicker,
    Sheet(SheetTab),
    Shortcuts,
    ZoomIn,
    ZoomOut,
    Theme(ThemePref),
    Units(Units),
    Speed(Speed),
    Utc(bool),
    LoopFrames(usize),
    LoopSpeed(f32),
    ReducedMotion(bool),
    Palette(String),
    RestoreColors,
    MaskDualPol(bool),
    LabelScale(f32),
    SpaceWeather,
    /// Show a GOES band, or none.
    Satellite(Option<hookecho_core::goes::Band>),
    /// Show an SPC outlook day (its categorical outlook), or none.
    Outlook(Option<u8>),
    /// Dock the inspector, or float it again.
    DockInspector,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub group: Group,
    pub label: String,
    pub hint: String,
    pub shortcut: Option<&'static str>,
    /// Extra words that match but aren't shown ("dark mode", "km").
    pub keywords: &'static str,
    pub action: Action,
}

fn cmd(
    group: Group,
    label: impl Into<String>,
    hint: impl Into<String>,
    shortcut: Option<&'static str>,
    keywords: &'static str,
    action: Action,
) -> Command {
    Command {
        group,
        label: label.into(),
        hint: hint.into(),
        shortcut,
        keywords,
        action,
    }
}

/// A titled group of (keycap, action) pairs.
pub type ShortcutGroup = (&'static str, &'static [(&'static str, &'static str)]);

/// Keyboard shortcuts, grouped, for the `?` sheet.
pub const SHORTCUTS: &[ShortcutGroup] = &[
    (
        "GENERAL",
        &[
            ("Ctrl K", "Menu (search everything)"),
            ("S", "Change radar"),
            ("P", "Change product or angle"),
            ("L", "Layers, colors and settings"),
            ("?", "Keyboard shortcuts"),
            ("Esc", "Close"),
        ],
    ),
    (
        "PRODUCTS",
        &[
            ("R", "Reflectivity"),
            ("V", "Velocity"),
            ("[", "Lower scan angle"),
            ("]", "Higher scan angle"),
        ],
    ),
    (
        "LOOP",
        &[
            ("Space", "Play or pause"),
            (",", "Previous frame"),
            (".", "Next frame"),
            ("Home", "Oldest frame"),
            ("End", "Live"),
        ],
    ),
    (
        "MAP",
        &[
            ("+", "Zoom in"),
            ("−", "Zoom out"),
            ("I", "Pin the inspector"),
        ],
    ),
];

/// The hint on a layer that's on: what a click will do.
const ON: &str = "On · click to turn off";

/// Every command for the current state, or one group's (`None`: all). In
/// "All", sites come only from a query (200 rows would bury everything
/// else), except favorites; browsing Radars lists every radar, nearest the
/// map center first.
pub fn all_in(model: &Model, query: &str, group: Option<Group>) -> Vec<Command> {
    let browse_sites = group == Some(Group::Sites);
    let mut v = everything(model, query, browse_sites);
    if let Some(g) = group {
        v.retain(|c| c.group == g);
    }
    v
}

fn everything(model: &Model, query: &str, browse_sites: bool) -> Vec<Command> {
    let mut v = Vec::new();
    let sites = model::site_list(
        query,
        &model.config.favorites,
        Some(model.map_center()),
        model.config.tdwr,
    );
    let limit = if browse_sites { usize::MAX } else { 8 };
    for (s, fav) in sites
        .into_iter()
        .filter(|(_, fav)| *fav || browse_sites || !query.trim().is_empty())
        .take(limit)
    {
        let hint = if fav {
            format!("{} · favorite", s.state)
        } else {
            s.state.clone()
        };
        v.push(cmd(
            Group::Sites,
            format!("{} {}", s.id, s.name),
            hint,
            None,
            "",
            Action::Site(s.id.as_str()),
        ));
    }
    // Always on top, so a first-time user sees how to start near home.
    v.push(cmd(
        Group::Sites,
        "Nearest radar",
        if model.config.home.is_some() {
            "Closest to home"
        } else {
            "Closest to the middle of the map"
        },
        None,
        "local closest my area",
        Action::NearestRadar,
    ));
    v.push(cmd(
        Group::Sites,
        "Use my location",
        "Set home where this computer is, then go to the nearest radar",
        None,
        "location gps geoclue where am i near me",
        Action::UseMyLocation,
    ));
    v.push(cmd(
        Group::Sites,
        "Set home",
        "Click the map to choose the spot",
        None,
        "location house my area",
        Action::SetHome,
    ));
    for m in PRODUCTS {
        let (name, what) = model::product_label(m);
        let key = match m {
            Moment::Reflectivity => Some("R"),
            Moment::Velocity => Some("V"),
            _ => None,
        };
        v.push(cmd(
            Group::Products,
            name,
            what,
            key,
            m.code(),
            Action::Product(m),
        ));
    }
    v.push(cmd(
        Group::Products,
        "Higher scan angle",
        "Next tilt up",
        Some("]"),
        "tilt elevation",
        Action::TiltUp,
    ));
    v.push(cmd(
        Group::Products,
        "Lower scan angle",
        "Next tilt down",
        Some("["),
        "tilt elevation",
        Action::TiltDown,
    ));
    let c = &model.config;
    let sat = c
        .satellite
        .as_deref()
        .and_then(hookecho_core::goes::Band::from_key);
    // A row that's on turns its layer off.
    for band in hookecho_core::goes::Band::ALL {
        let on = sat == Some(band);
        v.push(cmd(
            Group::Layers,
            format!("Satellite: {}", band.name().to_lowercase()),
            if on {
                ON
            } else {
                "Off · GOES-East under the radar"
            },
            None,
            "goes satellite clouds imagery hide",
            Action::Satellite((!on).then_some(band)),
        ));
    }
    let outlook_day = c
        .outlook
        .as_deref()
        .and_then(hookecho_core::spc::Product::from_key)
        .map(|p| p.day());
    for day in 1..=3u8 {
        let on = outlook_day == Some(day);
        v.push(cmd(
            Group::Layers,
            format!("SPC outlook, day {day}"),
            if on {
                ON
            } else {
                "Off · severe weather risk areas"
            },
            None,
            "convective outlook risk categorical tornado hail wind hide",
            Action::Outlook((!on).then_some(day)),
        ));
    }
    let map = model.config.map;
    let state = |on: bool| if on { ON } else { "Off" };
    for (layer, name, on, kw) in [
        (
            Layer::Warnings,
            "NWS warnings",
            c.warnings,
            "tornado severe flood alerts",
        ),
        (
            Layer::Watches,
            "SPC watches",
            c.watches,
            "tornado severe thunderstorm",
        ),
        (
            Layer::Discussions,
            "Mesoscale discussions",
            c.discussions,
            "spc md mcd",
        ),
        (
            Layer::StationPlots,
            "Station plots",
            c.station_plots,
            "surface observations metar temperature dewpoint wind",
        ),
        (
            Layer::StationPanel,
            "Station panel",
            c.station_panel,
            "command center conditions pressure visibility",
        ),
        (
            Layer::Aurora,
            "Aurora forecast",
            c.aurora,
            "space weather northern lights ovation",
        ),
        (Layer::Counties, "County lines", map.counties, "borders"),
        (Layer::Roads, "Highways", map.roads, "roads interstates"),
        (
            Layer::Labels,
            "Place names",
            map.labels,
            "cities towns labels",
        ),
    ] {
        v.push(cmd(
            Group::Layers,
            name,
            state(on),
            None,
            kw,
            Action::Layer(layer),
        ));
    }
    v.push(cmd(
        Group::Layers,
        "Hide dual-pol noise",
        if model.config.mask_dualpol {
            "On"
        } else {
            "Off"
        },
        None,
        "mask cc zdr phi clutter",
        Action::MaskDualPol(!model.config.mask_dualpol),
    ));
    let playing = model.looping.playing;
    v.extend([
        cmd(
            Group::Actions,
            if playing { "Pause loop" } else { "Play loop" },
            "Animate recent scans",
            Some("Space"),
            "animation",
            Action::PlayPause,
        ),
        cmd(
            Group::Actions,
            "Go to live",
            "Newest scan, loop stopped",
            Some("End"),
            "latest now",
            Action::Live,
        ),
        cmd(
            Group::Actions,
            "Change radar",
            "Pick a site from the list",
            Some("S"),
            "site",
            Action::SitePicker,
        ),
        cmd(
            Group::Actions,
            "Change product or angle",
            "Product and scan angle",
            Some("P"),
            "tilt",
            Action::ProductPicker,
        ),
        cmd(
            Group::Actions,
            "Layers",
            "Side sheet",
            Some("L"),
            "map",
            Action::Sheet(SheetTab::Layers),
        ),
        cmd(
            Group::Actions,
            "Colors",
            "Side sheet",
            None,
            "palette pal",
            Action::Sheet(SheetTab::Colors),
        ),
        cmd(
            Group::Actions,
            "Settings",
            "Side sheet",
            None,
            "preferences options",
            Action::Sheet(SheetTab::Settings),
        ),
        cmd(
            Group::Actions,
            "Space weather",
            "Kp, solar wind, flares, SWPC alerts",
            None,
            "aurora geomagnetic storm sun solar kp swpc",
            Action::SpaceWeather,
        ),
        cmd(
            Group::Actions,
            if crate::dock::wants_dock(model, crate::dock::Panel::Inspector) {
                "Float the inspector"
            } else {
                "Dock the inspector"
            },
            "Read values in the left dock instead of at the pointer",
            None,
            "inspector value dock panel pin",
            Action::DockInspector,
        ),
        cmd(
            Group::Actions,
            "Keyboard shortcuts",
            "Every key at a glance",
            Some("?"),
            "keys help",
            Action::Shortcuts,
        ),
        cmd(Group::Actions, "Zoom in", "", Some("+"), "", Action::ZoomIn),
        cmd(
            Group::Actions,
            "Zoom out",
            "",
            Some("−"),
            "",
            Action::ZoomOut,
        ),
    ]);
    let c = &model.config;
    let current = |on: bool| if on { "Current" } else { "" };
    for (t, name, kw) in [
        (ThemePref::Dark, "Dark", "night mode"),
        (ThemePref::Light, "Light", "day mode"),
        (ThemePref::System, "System", "follow automatic"),
    ] {
        v.push(cmd(
            Group::Settings,
            format!("Theme: {name}"),
            current(c.theme == t),
            None,
            kw,
            Action::Theme(t),
        ));
    }
    v.push(cmd(
        Group::Settings,
        "Distance: miles and feet",
        current(c.units == Units::Imperial),
        None,
        "units imperial",
        Action::Units(Units::Imperial),
    ));
    v.push(cmd(
        Group::Settings,
        "Distance: kilometers and meters",
        current(c.units == Units::Metric),
        None,
        "units metric km",
        Action::Units(Units::Metric),
    ));
    let (unit_now, _) = c.speed.resolve(c.units);
    for (sp, label, kw) in [
        (Speed::Knots, "kt", "knots"),
        (Speed::Mph, "mph", "miles per hour"),
        (Speed::Kmh, "km/h", "kilometers per hour kph"),
        (Speed::Ms, "m/s", "meters per second"),
    ] {
        v.push(cmd(
            Group::Settings,
            format!("Wind speed: {label}"),
            current(unit_now == sp.resolve(c.units).0),
            None,
            kw,
            Action::Speed(sp),
        ));
    }
    v.push(cmd(
        Group::Settings,
        "Clock: local time",
        current(!c.utc),
        None,
        "time zone",
        Action::Utc(false),
    ));
    v.push(cmd(
        Group::Settings,
        "Clock: UTC",
        current(c.utc),
        None,
        "time zone zulu gmt",
        Action::Utc(true),
    ));
    for n in [10, 30, 50] {
        v.push(cmd(
            Group::Settings,
            format!("Loop: {n} frames"),
            current(c.loop_frames == n),
            None,
            "length animation",
            Action::LoopFrames(n),
        ));
    }
    for (s, name) in [(0.5, "½×"), (1.0, "1×"), (2.0, "2×")] {
        v.push(cmd(
            Group::Settings,
            format!("Loop speed: {name}"),
            current((c.loop_speed - s).abs() < 0.01),
            None,
            "animation fast slow",
            Action::LoopSpeed(s),
        ));
    }
    v.push(cmd(
        Group::Settings,
        if c.reduced_motion {
            "Reduce motion: turn off"
        } else {
            "Reduce motion: turn on"
        },
        "",
        None,
        "animation accessibility",
        Action::ReducedMotion(!c.reduced_motion),
    ));
    let active = model.palettes.active(c, model.moment).name.clone();
    for choice in model.palettes.choices(model.moment) {
        let name = &choice.palette.name;
        v.push(cmd(
            Group::Settings,
            format!("Colors: {name}"),
            if *name == active {
                format!("Current for {}", model.moment.short_name())
            } else {
                format!("For {}", model.moment.short_name())
            },
            None,
            "palette color table",
            Action::Palette(name.clone()),
        ));
    }
    for (s, name) in [
        (0.85, "small"),
        (1.0, "medium"),
        (1.15, "large"),
        (1.3, "extra large"),
    ] {
        v.push(cmd(
            Group::Settings,
            format!("Map text: {name}"),
            current((c.label_scale - s).abs() < 0.01),
            None,
            "font size labels place names",
            Action::LabelScale(s),
        ));
    }
    v.push(cmd(
        Group::Settings,
        "Restore default colors",
        "Every product",
        None,
        "palette reset",
        Action::RestoreColors,
    ));
    v
}

/// Fuzzy score of `query` against `text`: every query character must
/// appear in order; word starts and runs score higher. None = no match.
pub fn fuzzy(query: &str, text: &str) -> Option<i32> {
    let q: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if q.is_empty() {
        return Some(0);
    }
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut qi, mut score, mut last) = (0, 0, None::<usize>);
    for (i, &c) in t.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            score += 1;
            if i == 0 || !t[i - 1].is_alphanumeric() {
                score += 6;
            }
            if last == Some(i.wrapping_sub(1)) {
                score += 4;
            }
            last = Some(i);
            qi += 1;
        }
    }
    (qi == q.len()).then(|| score - last.map_or(0, |l| l as i32 / 8))
}

/// Matching commands, grouped (DESIGN.md §3.8). Groups are ordered by
/// their best match, commands by score within a group, and matches much
/// weaker than the best are dropped. An empty query keeps `Group` order.
pub fn search(commands: Vec<Command>, query: &str) -> Vec<Command> {
    let mut scored: Vec<(i32, Command)> = commands
        .into_iter()
        .filter_map(|c| {
            let s = fuzzy(query, &c.label)
                .or_else(|| fuzzy(query, &format!("{} {}", c.label, c.keywords)).map(|s| s - 8))
                .or_else(|| fuzzy(query, &c.hint).map(|s| s - 12))
                // The site search also matches city and state names.
                .or((c.group == Group::Sites).then_some(0))?;
            Some((s, c))
        })
        .collect();
    if let Some(best) = scored.iter().map(|x| x.0).max()
        && best > 0
    {
        scored.retain(|x| x.0 * 5 >= best * 2);
    }
    let group_best = |g: Group, v: &[(i32, Command)]| {
        v.iter()
            .filter(|x| x.1.group == g)
            .map(|x| x.0)
            .max()
            .unwrap_or(0)
    };
    let bests: Vec<(Group, i32)> = [
        Group::Sites,
        Group::Products,
        Group::Layers,
        Group::Actions,
        Group::Settings,
    ]
    .iter()
    .map(|&g| (g, group_best(g, &scored)))
    .collect();
    let rank = |g: Group| bests.iter().find(|x| x.0 == g).map_or(0, |x| x.1);
    // Stable: ties keep group order, then listed order.
    scored.sort_by(|a, b| {
        rank(b.1.group)
            .cmp(&rank(a.1.group))
            .then(a.1.group.cmp(&b.1.group))
            .then(b.0.cmp(&a.0))
    });
    scored.into_iter().map(|(_, c)| c).collect()
}

/// Runs a command. `view` is the map's size in points (site changes and
/// zoom anchor on it); `now` is the UI clock (loop timing).
pub fn run(model: &mut Model, action: &Action, view: [f64; 2], now: f64) {
    model.picker = Picker::None;
    let c = &mut model.config;
    match action {
        Action::Site(id) => {
            if let Some(s) = hookecho_core::sites::by_id(id) {
                model.select_site(s, view);
            }
        }
        Action::Product(m) => model.moment = *m,
        Action::TiltUp => model.step_tilt(1),
        Action::TiltDown => model.step_tilt(-1),
        Action::Layer(l) => {
            let f = match l {
                Layer::Counties => &mut c.map.counties,
                Layer::Roads => &mut c.map.roads,
                Layer::Labels => &mut c.map.labels,
                Layer::Warnings => &mut c.warnings,
                Layer::Watches => &mut c.watches,
                Layer::Discussions => &mut c.discussions,
                Layer::StationPlots => &mut c.station_plots,
                Layer::StationPanel => &mut c.station_panel,
                Layer::Aurora => &mut c.aurora,
            };
            *f = !*f;
        }
        Action::PlayPause => model.toggle_play(now),
        Action::Live => {
            model.looping.playing = false;
            model.show_pos(usize::MAX);
        }
        Action::SitePicker => {
            model.picker = Picker::Site {
                query: String::new(),
                selected: 0,
                scroll: 0.0,
            }
        }
        Action::NearestRadar => model.go_to_nearest(view),
        Action::SetHome => model.placing_home = true,
        Action::UseMyLocation => model.locate_requested = true,
        Action::Satellite(b) => c.satellite = b.map(|b| b.key().to_string()),
        Action::Outlook(day) => {
            c.outlook = day.and_then(|d| {
                hookecho_core::spc::Product::ALL
                    .into_iter()
                    .find(|p| p.day() == d)
                    .map(|p| p.key())
            })
        }
        Action::ProductPicker => model.picker = Picker::Product,
        Action::Sheet(tab) => model.sheet = Some(model::Sheet::new(*tab)),
        Action::Shortcuts => model.picker = Picker::Shortcuts,
        Action::SpaceWeather => {
            let p = crate::dock::Panel::Space;
            if crate::dock::wants_dock(model, p) {
                model.config.dock_collapsed.retain(|k| k != p.key());
            } else {
                model.picker = Picker::SpaceWeather { scroll: 0.0 };
            }
        }
        Action::DockInspector => {
            let p = crate::dock::Panel::Inspector;
            if crate::dock::wants_dock(model, p) {
                crate::dock::undock(model, p);
            } else {
                crate::dock::dock(model, p);
            }
        }
        Action::ZoomIn | Action::ZoomOut => {
            let k = if *action == Action::ZoomIn {
                1.6
            } else {
                1.0 / 1.6
            };
            let g = hookecho_core::geo::Camera::MIN_SCALE..=hookecho_core::geo::Camera::MAX_SCALE;
            model.motion.zoom_target = (model.motion.zoom_target * k).clamp(*g.start(), *g.end());
            model.motion.zoom_anchor = [view[0] / 2.0, view[1] / 2.0];
        }
        Action::Theme(t) => c.theme = *t,
        Action::Units(u) => c.units = *u,
        Action::Speed(sp) => c.speed = *sp,
        Action::Utc(on) => c.utc = *on,
        Action::LoopFrames(n) => c.loop_frames = *n,
        Action::LoopSpeed(s) => c.loop_speed = *s,
        Action::ReducedMotion(on) => {
            c.reduced_motion = *on;
            model.reduced_motion = *on;
        }
        Action::Palette(name) => {
            let m = model.moment;
            model.palettes.select(&mut model.config, m, name);
        }
        Action::RestoreColors => model.palettes.restore_defaults(&mut model.config),
        Action::MaskDualPol(on) => model.config.mask_dualpol = *on,
        Action::LabelScale(s) => model.config.label_scale = *s,
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

    /// The location commands lead the empty palette, where a first-time
    /// user will see them, and a search for "home" finds them.
    #[test]
    fn location_commands_first() {
        let m = model();
        let empty = search(all_in(&m, "", None), "");
        assert_eq!(empty[0].action, Action::NearestRadar);
        assert_eq!(empty[1].action, Action::UseMyLocation);
        assert_eq!(empty[2].action, Action::SetHome);
        let found = search(all_in(&m, "home", None), "home");
        assert_eq!(found[0].action, Action::SetHome);
        let found = search(all_in(&m, "my location", None), "my location");
        assert_eq!(found[0].action, Action::UseMyLocation);
    }

    /// Each tab browses its group with nothing typed; Radars lists every
    /// radar, and the satellite and outlook layers are there to find.
    #[test]
    fn groups_browse_without_typing() {
        let mut m = model();
        let radars = search(all_in(&m, "", Some(Group::Sites)), "");
        let sites = radars
            .iter()
            .filter(|c| matches!(c.action, Action::Site(_)))
            .count();
        assert!(sites > 150, "{sites}");
        assert!(search(all_in(&m, "", None), "").len() < radars.len());
        let layers = search(all_in(&m, "", Some(Group::Layers)), "");
        assert!(layers.iter().all(|c| c.group == Group::Layers));
        let ir = layers
            .iter()
            .find(|c| c.label == "Satellite: infrared")
            .unwrap();
        run(&mut m, &ir.action.clone(), [1280.0, 800.0], 0.0);
        assert_eq!(m.config.satellite.as_deref(), Some("infrared"));
        // The same row, now on, turns it off.
        let again = search(all_in(&m, "", Some(Group::Layers)), "");
        let ir = again
            .iter()
            .find(|c| c.label == "Satellite: infrared")
            .unwrap();
        assert_eq!(ir.hint, ON);
        run(&mut m, &ir.action.clone(), [1280.0, 800.0], 0.0);
        assert_eq!(m.config.satellite, None);
        let day2 = search(all_in(&m, "outlook", None), "outlook");
        let d2 = day2
            .iter()
            .find(|c| c.label == "SPC outlook, day 2")
            .unwrap();
        run(&mut m, &d2.action.clone(), [1280.0, 800.0], 0.0);
        assert_eq!(m.config.outlook.as_deref(), Some("day2-cat"));
    }

    #[test]
    fn fuzzy_prefers_word_starts() {
        assert!(fuzzy("vel", "Velocity").is_some());
        assert!(fuzzy("xyz", "Velocity").is_none());
        assert!(
            fuzzy("cc", "Correlation coefficient (CC)").unwrap()
                > fuzzy("cc", "Clock: local time").unwrap_or(0)
        );
        assert!(fuzzy("thd", "Theme: Dark").is_some());
    }

    #[test]
    fn search_finds_by_label_keyword_and_site() {
        let m = model();
        let first = |q: &str| {
            search(all_in(&m, q, None), q)
                .into_iter()
                .next()
                .map(|c| c.action)
        };
        assert_eq!(first("velocity"), Some(Action::Product(Moment::Velocity)));
        assert_eq!(first("dark mode"), Some(Action::Theme(ThemePref::Dark)));
        assert_eq!(first("utc"), Some(Action::Utc(true)));
        assert_eq!(first("knots"), Some(Action::Speed(Speed::Knots)));
        assert_eq!(first("mph"), Some(Action::Speed(Speed::Mph)));
        assert_eq!(first("kfws"), Some(Action::Site("KFWS")));
        assert_eq!(first("roads"), Some(Action::Layer(Layer::Roads)));
        // An empty query lists commands but no (non-favorite) sites.
        let empty = search(all_in(&m, "", None), "");
        assert!(empty.iter().all(|c| !matches!(c.action, Action::Site(_))));
        assert!(empty.len() > 20);
        // Grouped in order when nothing is typed.
        let groups: Vec<Group> = empty.iter().map(|c| c.group).collect();
        assert!(groups.windows(2).all(|w| w[0] <= w[1]));
        // Each group appears once, as a block.
        let typed = search(all_in(&m, "loop", None), "loop");
        let mut seen: Vec<Group> = Vec::new();
        for c in &typed {
            if seen.last() != Some(&c.group) {
                assert!(!seen.contains(&c.group), "{:?} split", c.group);
                seen.push(c.group);
            }
        }
    }

    #[test]
    fn run_applies() {
        let mut m = model();
        run(
            &mut m,
            &Action::Product(Moment::Velocity),
            [1280.0, 800.0],
            0.0,
        );
        assert_eq!(m.moment, Moment::Velocity);
        run(&mut m, &Action::Layer(Layer::Labels), [1280.0, 800.0], 0.0);
        assert!(!m.config.map.labels);
        run(&mut m, &Action::Site("KFWS"), [1280.0, 800.0], 0.0);
        assert_eq!(m.site.id, "KFWS");
        run(
            &mut m,
            &Action::Theme(ThemePref::Light),
            [1280.0, 800.0],
            0.0,
        );
        assert_eq!(m.config.theme, ThemePref::Light);
        run(&mut m, &Action::Shortcuts, [1280.0, 800.0], 0.0);
        assert_eq!(m.picker, Picker::Shortcuts);
    }
}
