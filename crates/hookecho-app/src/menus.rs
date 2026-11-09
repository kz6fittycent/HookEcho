//! The menu bar's menus (docs/DESIGN.md §3.2): what each one holds for the
//! current state. Every row runs a [`commands::Action`], the same actions
//! the search palette runs, so a menu and a search always agree. No egui
//! here; `menubar.rs` draws these.

use crate::commands::{Action, Layer};
use crate::dock::Panel;
use crate::model::{self, Model, PRODUCTS};
use hookecho_core::Moment;
use hookecho_core::config::{Speed, ThemePref, Units};

/// The menus, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuId {
    Radar,
    Product,
    Layers,
    Loop,
    Panels,
    View,
    Settings,
    Help,
    /// Narrow windows: one "Menu" title holding every menu as a submenu.
    All,
}

impl MenuId {
    /// The bar's menus in order (`All` stands in for them when narrow).
    pub const BAR: [MenuId; 8] = [
        MenuId::Radar,
        MenuId::Product,
        MenuId::Layers,
        MenuId::Loop,
        MenuId::Panels,
        MenuId::View,
        MenuId::Settings,
        MenuId::Help,
    ];

    pub fn title(self) -> &'static str {
        match self {
            MenuId::Radar => "Radar",
            MenuId::Product => "Product",
            MenuId::Layers => "Layers",
            MenuId::Loop => "Loop",
            MenuId::Panels => "Panels",
            MenuId::View => "View",
            MenuId::Settings => "Settings",
            MenuId::Help => "Help",
            MenuId::All => "Menu",
        }
    }
}

/// The mark in a row's left gutter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    None,
    /// An on/off switch; the menu stays open so several can be flipped.
    Check(bool),
    /// One of a set; picking one closes the menu.
    Radio(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Run {
        label: String,
        /// The keycap shown at the right.
        shortcut: Option<&'static str>,
        mark: Mark,
        enabled: bool,
        action: Action,
    },
    /// Opens a submenu; `value` (the current choice) shows at the right.
    Sub {
        label: String,
        value: String,
        items: Vec<Item>,
    },
    /// A small caption over a group of rows.
    Heading(String),
    /// A line of text that does nothing (About, hints).
    Info(String),
    Separator,
}

impl Item {
    /// Whether the row reacts to the pointer and the arrow keys.
    pub fn interactive(&self) -> bool {
        match self {
            Item::Run { enabled, .. } => *enabled,
            Item::Sub { .. } => true,
            _ => false,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Item::Run { label, .. } | Item::Sub { label, .. } => label,
            Item::Heading(s) | Item::Info(s) => s,
            Item::Separator => "",
        }
    }
}

fn run(label: impl Into<String>, shortcut: Option<&'static str>, action: Action) -> Item {
    Item::Run {
        label: label.into(),
        shortcut,
        mark: Mark::None,
        enabled: true,
        action,
    }
}

fn check(label: impl Into<String>, on: bool, action: Action) -> Item {
    Item::Run {
        label: label.into(),
        shortcut: None,
        mark: Mark::Check(on),
        enabled: true,
        action,
    }
}

fn radio(label: impl Into<String>, on: bool, action: Action) -> Item {
    Item::Run {
        label: label.into(),
        shortcut: None,
        mark: Mark::Radio(on),
        enabled: true,
        action,
    }
}

fn sub(label: impl Into<String>, value: impl Into<String>, items: Vec<Item>) -> Item {
    Item::Sub {
        label: label.into(),
        value: value.into(),
        items,
    }
}

/// The items of `menu` for the current state.
pub fn build(model: &Model, menu: MenuId) -> Vec<Item> {
    match menu {
        MenuId::Radar => radar(model),
        MenuId::Product => product(model),
        MenuId::Layers => layers(model),
        MenuId::Loop => looping(model),
        MenuId::Panels => panels(model),
        MenuId::View => view(model),
        MenuId::Settings => settings(model),
        MenuId::Help => help(),
        MenuId::All => MenuId::BAR
            .iter()
            .map(|&m| sub(m.title(), "", build(model, m)))
            .collect(),
    }
}

/// The items along `path` (indices of opened submenus): `path = []` is the
/// menu itself. `None` if the path no longer leads to a submenu (the state
/// changed under it).
pub fn level(items: &[Item], path: &[usize]) -> Option<Vec<Item>> {
    let mut cur = items.to_vec();
    for &i in path {
        match cur.get(i) {
            Some(Item::Sub { items, .. }) => cur = items.clone(),
            _ => return None,
        }
    }
    Some(cur)
}

fn radar(model: &Model) -> Vec<Item> {
    let c = &model.config;
    let mut v = vec![
        run("Change radar…", Some("S"), Action::SitePicker),
        run(
            if c.home.is_some() {
                "Nearest radar to home"
            } else {
                "Nearest radar to the map center"
            },
            None,
            Action::NearestRadar,
        ),
        run("Use my location", None, Action::UseMyLocation),
        run("Set home on the map…", None, Action::SetHome),
    ];
    let favorites: Vec<Item> = c
        .favorites
        .iter()
        .filter_map(|id| hookecho_core::sites::by_id(id))
        .map(|s| {
            radio(
                format!("{} {}", s.id, s.name),
                s.id == model.site.id,
                Action::Site(s.id.as_str()),
            )
        })
        .collect();
    v.push(Item::Separator);
    if favorites.is_empty() {
        v.push(Item::Info(
            "Star a radar in the radar list to keep it here".into(),
        ));
    } else {
        let n = favorites.len();
        v.push(sub("Favorites", n.to_string(), favorites));
    }
    v.push(Item::Separator);
    v.push(check(
        "Airport radars (TDWR)",
        c.tdwr,
        Action::Layer(Layer::Tdwr),
    ));
    v.push(Item::Run {
        label: "Airport radar long range".into(),
        shortcut: None,
        mark: Mark::Check(c.tdwr_long_range),
        enabled: c.tdwr,
        action: Action::Layer(Layer::TdwrLongRange),
    });
    v
}

fn product(model: &Model) -> Vec<Item> {
    let mut v: Vec<Item> = PRODUCTS
        .iter()
        .map(|&m| Item::Run {
            label: model::product_label(m).0.to_string(),
            shortcut: match m {
                Moment::Reflectivity => Some("R"),
                Moment::Velocity => Some("V"),
                _ => None,
            },
            mark: Mark::Radio(model.moment == m),
            enabled: true,
            action: Action::Product(m),
        })
        .collect();
    v.extend([
        Item::Separator,
        run("Higher scan angle", Some("]"), Action::TiltUp),
        run("Lower scan angle", Some("["), Action::TiltDown),
        run("Product and angle…", Some("P"), Action::ProductPicker),
        Item::Separator,
        check(
            "Hide dual-pol noise",
            model.config.mask_dualpol,
            Action::MaskDualPol(!model.config.mask_dualpol),
        ),
    ]);
    v
}

fn layers(model: &Model) -> Vec<Item> {
    use hookecho_core::alerts::Event;
    use hookecho_core::goes::Band;
    use hookecho_core::spc::Product;
    let c = &model.config;
    let map = c.map;
    let hidden = |e: Event| c.hidden_warnings.iter().any(|k| k == e.key());
    let shown = Event::ALL.iter().filter(|e| !hidden(**e)).count();
    let types: Vec<Item> = Event::ALL
        .iter()
        .map(|&e| {
            check(
                e.name().trim_end_matches(" Warning"),
                !hidden(e),
                Action::WarningType(e.key()),
            )
        })
        .collect();
    let outlook = c.outlook.as_deref().and_then(Product::from_key);
    let mut outlooks = vec![radio("Off", outlook.is_none(), Action::Outlook(None))];
    for day in 1..=3u8 {
        outlooks.push(Item::Heading(format!("DAY {day}")));
        for p in Product::ALL.into_iter().filter(|p| p.day() == day) {
            outlooks.push(radio(
                format!("Day {day}: {}", p.name().to_lowercase()),
                outlook == Some(p),
                Action::Outlook(Some(p.key())),
            ));
        }
    }
    let band = c.satellite.as_deref().and_then(Band::from_key);
    let mut bands = vec![radio("Off", band.is_none(), Action::Satellite(None))];
    bands.extend(
        Band::ALL
            .into_iter()
            .map(|b| radio(b.name(), band == Some(b), Action::Satellite(Some(b)))),
    );
    bands.extend([Item::Separator, Item::Info(crate::satellite::status(model))]);
    // A feed that's on but failing says so where it's switched.
    let l = &model.layers;
    let trouble = |on: bool, t: &Option<String>, what: &str| {
        (on && t.is_some()).then(|| Item::Info(format!("Can't reach {what} right now; retrying")))
    };
    let mut v = vec![
        Item::Heading("WARNINGS AND OUTLOOKS".into()),
        check("NWS warnings", c.warnings, Action::Layer(Layer::Warnings)),
    ];
    v.extend(trouble(c.warnings, &l.alerts_trouble, "the NWS"));
    v.extend([
        sub(
            "Warning types",
            if shown == Event::ALL.len() {
                "All".to_string()
            } else {
                format!("{shown} of {}", Event::ALL.len())
            },
            types,
        ),
        check("SPC watches", c.watches, Action::Layer(Layer::Watches)),
        check(
            "Mesoscale discussions",
            c.discussions,
            Action::Layer(Layer::Discussions),
        ),
    ]);
    v.extend(trouble(
        c.watches || c.discussions || outlook.is_some(),
        &l.spc_trouble,
        "SPC",
    ));
    v.extend([
        sub(
            "SPC outlook",
            outlook.map_or("Off".to_string(), |p| {
                format!("Day {}: {}", p.day(), p.name().to_lowercase())
            }),
            outlooks,
        ),
        Item::Heading("OBSERVATIONS".into()),
        sub(
            "Satellite (GOES-East)",
            band.map_or("Off", |b| b.name()),
            bands,
        ),
        check(
            "Station plots",
            c.station_plots,
            Action::Layer(Layer::StationPlots),
        ),
    ]);
    v.extend(trouble(
        c.station_plots || c.station_panel,
        &l.obs_trouble,
        "aviationweather.gov",
    ));
    v.extend([
        check("Aurora forecast", c.aurora, Action::Layer(Layer::Aurora)),
        Item::Heading("MAP".into()),
        check("County lines", map.counties, Action::Layer(Layer::Counties)),
        check("Highways", map.roads, Action::Layer(Layer::Roads)),
        check("Place names", map.labels, Action::Layer(Layer::Labels)),
    ]);
    if let Some(t) = trouble(c.aurora, &l.space_trouble, "NOAA SWPC")
        && let Some(i) = v.iter().position(|i| i.label() == "Aurora forecast")
    {
        v.insert(i + 1, t);
    }
    v
}

fn looping(model: &Model) -> Vec<Item> {
    let c = &model.config;
    let frames = [10, 30, 50]
        .into_iter()
        .map(|n| {
            radio(
                format!("{n} frames"),
                c.loop_frames == n,
                Action::LoopFrames(n),
            )
        })
        .collect();
    let speeds = [(0.5, "½×"), (1.0, "1×"), (2.0, "2×")]
        .into_iter()
        .map(|(s, name)| radio(name, (c.loop_speed - s).abs() < 0.01, Action::LoopSpeed(s)))
        .collect();
    let speed_name = [(0.5, "½×"), (1.0, "1×"), (2.0, "2×")]
        .into_iter()
        .find(|(s, _)| (c.loop_speed - s).abs() < 0.01)
        .map_or(String::new(), |(_, n)| n.to_string());
    vec![
        run(
            if model.looping.playing {
                "Pause loop"
            } else {
                "Play loop"
            },
            Some("Space"),
            Action::PlayPause,
        ),
        run("Go to live", Some("End"), Action::Live),
        run("Previous frame", Some(","), Action::StepFrame(-1)),
        run("Next frame", Some("."), Action::StepFrame(1)),
        run("Oldest frame", Some("Home"), Action::FirstFrame),
        Item::Separator,
        sub("Length", format!("{} frames", c.loop_frames), frames),
        sub("Speed", speed_name, speeds),
    ]
}

fn panels(model: &Model) -> Vec<Item> {
    let c = &model.config;
    let mut v = vec![
        check(
            "Station panel",
            c.station_panel,
            Action::Layer(Layer::StationPanel),
        ),
        run("Space weather", None, Action::SpaceWeather),
        Item::Heading("KEEP IN THE LEFT DOCK".into()),
    ];
    v.extend(Panel::ALL.into_iter().map(|p| {
        check(
            p.title(),
            crate::dock::wants_dock(model, p),
            Action::ToggleDock(p),
        )
    }));
    v
}

fn view(model: &Model) -> Vec<Item> {
    let c = &model.config;
    let themes = [
        (ThemePref::Dark, "Dark"),
        (ThemePref::Light, "Light"),
        (ThemePref::System, "Same as the system"),
    ];
    let theme_name = themes
        .iter()
        .find(|(t, _)| *t == c.theme)
        .map_or("", |(_, n)| n);
    let scales = [
        (0.85, "Small"),
        (1.0, "Medium"),
        (1.15, "Large"),
        (1.3, "Extra large"),
    ];
    let scale_name = scales
        .iter()
        .find(|(s, _)| (c.label_scale - s).abs() < 0.01)
        .map_or("", |(_, n)| n);
    let active = model.palettes.active(c, model.moment).name.clone();
    let mut colors: Vec<Item> = model
        .palettes
        .choices(model.moment)
        .iter()
        .map(|ch| {
            let name = ch.palette.name.clone();
            radio(name.clone(), name == active, Action::Palette(name))
        })
        .collect();
    colors.extend([
        Item::Separator,
        Item::Info("Drop a GRLevelX .pal file on the window to add one".into()),
        run("Restore default colors", None, Action::RestoreColors),
    ]);
    vec![
        sub(
            "Theme",
            theme_name,
            themes
                .iter()
                .map(|(t, n)| radio(*n, c.theme == *t, Action::Theme(*t)))
                .collect(),
        ),
        sub(
            "Map text",
            scale_name,
            scales
                .iter()
                .map(|(s, n)| radio(*n, (c.label_scale - s).abs() < 0.01, Action::LabelScale(*s)))
                .collect(),
        ),
        sub(
            format!("Colors for {}", model.moment.short_name()),
            active,
            colors,
        ),
        Item::Separator,
        run("Zoom in", Some("+"), Action::ZoomIn),
        run("Zoom out", Some("−"), Action::ZoomOut),
        Item::Separator,
        check(
            "Reduce motion",
            c.reduced_motion,
            Action::ReducedMotion(!c.reduced_motion),
        ),
    ]
}

fn settings(model: &Model) -> Vec<Item> {
    let c = &model.config;
    let (speed_now, _) = c.speed.resolve(c.units);
    let speeds = [
        (Speed::Knots, "Knots (kt)"),
        (Speed::Mph, "Miles per hour (mph)"),
        (Speed::Kmh, "Kilometers per hour (km/h)"),
        (Speed::Ms, "Meters per second (m/s)"),
    ];
    let speed_name = speeds
        .iter()
        .find(|(s, _)| s.resolve(c.units).0 == speed_now)
        .map_or("", |(_, n)| n);
    vec![
        sub(
            "Distance",
            match c.units {
                Units::Imperial => "Miles and feet",
                Units::Metric => "Kilometers and meters",
            },
            vec![
                radio(
                    "Miles and feet",
                    c.units == Units::Imperial,
                    Action::Units(Units::Imperial),
                ),
                radio(
                    "Kilometers and meters",
                    c.units == Units::Metric,
                    Action::Units(Units::Metric),
                ),
            ],
        ),
        sub(
            "Wind speed",
            speed_name,
            speeds
                .iter()
                .map(|(s, n)| radio(*n, s.resolve(c.units).0 == speed_now, Action::Speed(*s)))
                .collect(),
        ),
        sub(
            "Clock",
            if c.utc { "UTC" } else { "Local time" },
            vec![
                radio("Local time", !c.utc, Action::Utc(false)),
                radio("UTC", c.utc, Action::Utc(true)),
            ],
        ),
        Item::Separator,
        Item::Run {
            label: match model.cache_bytes {
                Some(n) => format!("Clear radar cache ({})", human_bytes(n)),
                None => "Clear radar cache".to_string(),
            },
            shortcut: None,
            mark: Mark::None,
            enabled: model.cache_bytes != Some(0),
            action: Action::ClearCache,
        },
        Item::Info("Live radar comes from NOAA via Unidata's open data".into()),
        Item::Info("on AWS. Nothing about you is sent.".into()),
    ]
}

fn help() -> Vec<Item> {
    vec![
        run("Keyboard shortcuts", Some("?"), Action::Shortcuts),
        run("Search everything", Some("Ctrl K"), Action::Search),
        Item::Separator,
        sub(
            "About Hook Echo",
            "",
            vec![
                Item::Info(format!("Hook Echo {}", env!("CARGO_PKG_VERSION"))),
                Item::Info("Free software under the GPL, version 3 or later".into()),
                Item::Info("Radar: NOAA NEXRAD and TDWR".into()),
                Item::Info("Maps: US Census Bureau and Natural Earth".into()),
            ],
        ),
    ]
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
    use super::*;
    use crate::commands::{self, Group};
    use hookecho_core::config::Config;

    fn model() -> Model {
        let site = hookecho_core::sites::by_id("KTLX").unwrap();
        Model::new(Config::default(), site, [1280.0, 800.0])
    }

    fn actions(items: &[Item], out: &mut Vec<Action>) {
        for it in items {
            match it {
                Item::Run { action, .. } => out.push(action.clone()),
                Item::Sub { items, .. } => actions(items, out),
                _ => {}
            }
        }
    }

    fn every_action(model: &Model) -> Vec<Action> {
        let mut v = Vec::new();
        for m in MenuId::BAR {
            actions(&build(model, m), &mut v);
        }
        v
    }

    /// Owner (2026-10-29): everything must be findable by browsing. Every
    /// command the search offers (other than one radar by name) is in a
    /// menu.
    #[test]
    fn every_search_command_is_in_a_menu() {
        let m = model();
        let menu = every_action(&m);
        for c in commands::all_in(&m, "", None) {
            if (c.group == Group::Sites && matches!(c.action, Action::Site(_)))
                || matches!(c.action, Action::OpenMenu(_))
            {
                continue;
            }
            // Toggles in the palette carry the new value; the menus carry
            // the same action.
            assert!(
                menu.contains(&c.action),
                "{:?} ({}) is in no menu",
                c.action,
                c.label
            );
        }
    }

    #[test]
    fn the_narrow_menu_holds_every_menu() {
        let m = model();
        let all = build(&m, MenuId::All);
        assert_eq!(all.len(), MenuId::BAR.len());
        let mut v = Vec::new();
        actions(&all, &mut v);
        assert_eq!(v, every_action(&m));
    }

    #[test]
    fn marks_follow_the_state() {
        let mut m = model();
        m.config.watches = false;
        m.config.satellite = Some(hookecho_core::goes::Band::Infrared.key().into());
        let items = build(&m, MenuId::Layers);
        let find = |label: &str| items.iter().find(|i| i.label() == label).cloned();
        assert!(matches!(
            find("SPC watches"),
            Some(Item::Run {
                mark: Mark::Check(false),
                ..
            })
        ));
        let Some(Item::Sub { value, items, .. }) = find("Satellite (GOES-East)") else {
            panic!("no satellite submenu")
        };
        assert_eq!(value, hookecho_core::goes::Band::Infrared.name());
        assert!(items.iter().any(|i| matches!(
            i,
            Item::Run {
                mark: Mark::Radio(true),
                action: Action::Satellite(Some(hookecho_core::goes::Band::Infrared)),
                ..
            }
        )));
    }

    #[test]
    fn levels_follow_a_path() {
        let m = model();
        let items = build(&m, MenuId::All);
        let layers = MenuId::BAR
            .iter()
            .position(|x| *x == MenuId::Layers)
            .unwrap();
        let l1 = level(&items, &[layers]).unwrap();
        let types = l1
            .iter()
            .position(|i| i.label() == "Warning types")
            .unwrap();
        let l2 = level(&items, &[layers, types]).unwrap();
        assert_eq!(l2.len(), hookecho_core::alerts::Event::ALL.len());
        assert!(
            level(&items, &[layers, 0]).is_none(),
            "a heading isn't a submenu"
        );
    }

    #[test]
    fn human_bytes() {
        assert_eq!(super::human_bytes(0), "0.0 MB");
        assert_eq!(super::human_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(super::human_bytes(50 * 1024 * 1024), "50 MB");
        assert_eq!(super::human_bytes(3 * 1024 * 1024 * 1024 / 2), "1.5 GB");
    }
}
