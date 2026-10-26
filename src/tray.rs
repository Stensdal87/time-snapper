//! Ikon i systembakken med en menu til at styre tiden uden at vise vinduet.
//!
//! Brugerfladen beskriver menuen med en `TrayState`; klik i menuen kommer
//! tilbage som `TrayAction`s. Linux bruger StatusNotifierItem over D-Bus (ksni),
//! Windows bruger tray-icon.

use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui;

use crate::data::Id;

pub enum TrayAction {
    /// Venstreklik på ikonet: vis eller skjul vinduet.
    Toggle,
    Start(Id),
    Call,
    Pause,
    Stop,
    Quit,
}

/// Et projekt i menuen "Skift projekt".
#[derive(Clone, PartialEq)]
pub struct TrayProject {
    pub id: Id,
    pub name: String,
    pub active: bool,
}

/// Alt hvad menuen viser. Menuen bygges kun om, når den ændrer sig.
#[derive(Clone, PartialEq, Default)]
pub struct TrayState {
    /// Første linje i menuen og tooltip, f.eks. "▶ Kunde / Projekt · 0:45:12".
    /// Ændres hvert sekund, mens tiden kører.
    pub status: String,
    pub running: bool,
    /// Teksten på telefon-punktet ("📞 Telefon", "📞 Afslut opkald …").
    pub call_label: String,
    /// Teksten på pause-punktet ("☕ Pause", "▶ Fortsæt" …).
    pub pause_label: String,
    pub window_visible: bool,
    /// Kunder med deres projekter.
    pub customers: Vec<(String, Vec<TrayProject>)>,
}

pub struct Tray {
    rx: Receiver<TrayAction>,
    last: Option<TrayState>,
    inner: platform::Inner,
}

impl Tray {
    /// Opretter ikonet. `None` hvis systemet ikke har en systembakke.
    pub fn start(ctx: &egui::Context) -> Option<Self> {
        let (tx, rx) = mpsc::channel();
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).ok()?;
        let sender = ActionSender { tx, ctx: ctx.clone() };
        let inner = platform::Inner::start(sender, icon)?;
        Some(Self { rx, last: None, inner })
    }

    pub fn update(&mut self, state: TrayState) {
        if self.last.as_ref() != Some(&state) {
            self.inner.update(&state);
            self.last = Some(state);
        }
    }

    pub fn poll(&self) -> Vec<TrayAction> {
        self.rx.try_iter().collect()
    }
}

/// Sender handlinger til brugerfladen og vækker den, også når vinduet er skjult.
#[derive(Clone)]
struct ActionSender {
    tx: Sender<TrayAction>,
    ctx: egui::Context,
}

impl ActionSender {
    fn send(&self, action: TrayAction) {
        let _ = self.tx.send(action);
        self.ctx.request_repaint();
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use ksni::blocking::{Handle, TrayMethods};
    use ksni::menu::{MenuItem, StandardItem, SubMenu};

    struct KsniTray {
        state: TrayState,
        sender: ActionSender,
        icon: ksni::Icon,
    }

    pub struct Inner {
        handle: Handle<KsniTray>,
    }

    impl Inner {
        pub fn start(sender: ActionSender, icon: egui::IconData) -> Option<Self> {
            // ksni vil have ARGB i netværksbyte-orden; egui giver RGBA.
            let data = icon.rgba.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, a]| [a, r, g, b]).collect();
            let icon = ksni::Icon { width: icon.width as i32, height: icon.height as i32, data };
            let tray = KsniTray { state: TrayState::default(), sender, icon };
            // Fejler hvis skrivebordet ikke har en systembakke (StatusNotifierWatcher).
            let handle = tray.spawn().ok()?;
            Some(Self { handle })
        }

        pub fn update(&self, state: &TrayState) {
            let state = state.clone();
            self.handle.update(|tray| tray.state = state);
        }
    }

    fn item(label: impl Into<String>, enabled: bool, action: fn(&KsniTray)) -> MenuItem<KsniTray> {
        StandardItem {
            label: label.into(),
            enabled,
            activate: Box::new(move |tray: &mut KsniTray| action(tray)),
            ..Default::default()
        }
        .into()
    }

    impl ksni::Tray for KsniTray {
        fn id(&self) -> String {
            "time-snapper".into()
        }

        fn title(&self) -> String {
            "Time Snapper".into()
        }

        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            vec![self.icon.clone()]
        }

        fn tool_tip(&self) -> ksni::ToolTip {
            ksni::ToolTip { title: "Time Snapper".into(), description: self.state.status.clone(), ..Default::default() }
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            self.sender.send(TrayAction::Toggle);
        }

        fn menu(&self) -> Vec<MenuItem<Self>> {
            let s = &self.state;
            let projects: Vec<MenuItem<Self>> = s
                .customers
                .iter()
                .map(|(customer, projects)| {
                    SubMenu {
                        label: customer.clone(),
                        submenu: projects
                            .iter()
                            .map(|p| {
                                let id = p.id;
                                StandardItem {
                                    label: if p.active { format!("▶ {}", p.name) } else { p.name.clone() },
                                    enabled: !p.active,
                                    activate: Box::new(move |tray: &mut Self| tray.sender.send(TrayAction::Start(id))),
                                    ..Default::default()
                                }
                                .into()
                            })
                            .collect(),
                        ..Default::default()
                    }
                    .into()
                })
                .collect();

            vec![
                item(s.status.clone(), false, |_| {}),
                MenuItem::Separator,
                SubMenu { label: "Skift projekt".into(), submenu: projects, ..Default::default() }.into(),
                item(s.call_label.clone(), true, |t| t.sender.send(TrayAction::Call)),
                item(s.pause_label.clone(), true, |t| t.sender.send(TrayAction::Pause)),
                item("⏹ Stop", s.running, |t| t.sender.send(TrayAction::Stop)),
                MenuItem::Separator,
                item(
                    if s.window_visible { "Skjul vinduet" } else { "Vis vinduet" },
                    true,
                    |t| t.sender.send(TrayAction::Toggle),
                ),
                item("Afslut Time Snapper", true, |t| t.sender.send(TrayAction::Quit)),
            ]
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

    pub struct Inner {
        icon: TrayIcon,
        /// Statuslinjen, så dens tekst kan opdateres uden at bygge menuen om.
        status_item: MenuItem,
        /// Menuen som den sidst blev bygget, uden statusteksten.
        built: TrayState,
    }

    impl Inner {
        pub fn start(sender: ActionSender, icon: egui::IconData) -> Option<Self> {
            let clicks = sender.clone();
            TrayIconEvent::set_event_handler(Some(move |event| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    clicks.send(TrayAction::Toggle);
                }
            }));
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let id = event.id.0.as_str();
                let action = match id {
                    "call" => TrayAction::Call,
                    "pause" => TrayAction::Pause,
                    "stop" => TrayAction::Stop,
                    "toggle" => TrayAction::Toggle,
                    "quit" => TrayAction::Quit,
                    _ => match id.strip_prefix("start:").and_then(|n| n.parse().ok()) {
                        Some(project_id) => TrayAction::Start(project_id),
                        None => return,
                    },
                };
                sender.send(action);
            }));

            let icon = tray_icon::Icon::from_rgba(icon.rgba, icon.width, icon.height).ok()?;
            let built = TrayState::default();
            let (menu, status_item) = build_menu(&built);
            let icon = TrayIconBuilder::new()
                .with_icon(icon)
                .with_tooltip("Time Snapper")
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .build()
                .ok()?;
            Some(Self { icon, status_item, built })
        }

        pub fn update(&mut self, state: &TrayState) {
            // Statusteksten ændres hvert sekund. At skifte hele menuen ud kan
            // lukke en åben menu, så den bygges kun om, når andet end teksten ændres.
            let structure = TrayState { status: String::new(), ..state.clone() };
            if structure != self.built {
                let (menu, status_item) = build_menu(state);
                self.icon.set_menu(Some(Box::new(menu)));
                self.status_item = status_item;
                self.built = structure;
            } else {
                self.status_item.set_text(&state.status);
            }
            let _ = self.icon.set_tooltip(Some(format!("Time Snapper\n{}", state.status)));
        }
    }

    fn build_menu(s: &TrayState) -> (Menu, MenuItem) {
        let menu = Menu::new();
        let switch = Submenu::new("Skift projekt", true);
        for (customer, projects) in &s.customers {
            let sub = Submenu::new(customer, true);
            for p in projects {
                let label = if p.active { format!("▶ {}", p.name) } else { p.name.clone() };
                let _ = sub.append(&MenuItem::with_id(format!("start:{}", p.id), label, !p.active, None));
            }
            let _ = switch.append(&sub);
        }
        let toggle = if s.window_visible { "Skjul vinduet" } else { "Vis vinduet" };
        let status_item = MenuItem::with_id("status", &s.status, false, None);
        let _ = menu.append(&status_item);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&switch);
        let _ = menu.append(&MenuItem::with_id("call", &s.call_label, true, None));
        let _ = menu.append(&MenuItem::with_id("pause", &s.pause_label, true, None));
        let _ = menu.append(&MenuItem::with_id("stop", "⏹ Stop", s.running, None));
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&MenuItem::with_id("toggle", toggle, true, None));
        let _ = menu.append(&MenuItem::with_id("quit", "Afslut Time Snapper", true, None));
        (menu, status_item)
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use super::*;

    pub struct Inner;

    impl Inner {
        pub fn start(_: ActionSender, _: egui::IconData) -> Option<Self> {
            None
        }

        pub fn update(&mut self, _: &TrayState) {}
    }
}
