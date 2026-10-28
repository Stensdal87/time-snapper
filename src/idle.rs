//! Registrerer når brugeren har været væk fra computeren.
//!
//! Baggrundstråde sender `IdleEvent`s til brugerfladen:
//! - Linux (Wayland): protokollen ext-idle-notify, som ser al input på skrivebordet.
//! - Windows: GetLastInputInfo, der tjekkes hvert 5. sekund.
//! - Alle platforme: et ur der opdager, når computeren har været i dvale.
//!
//! En "væk"-periode starter først efter `IDLE_AFTER` uden input. Brugerfladen
//! afgør selv, om perioden er lang nok til at spørge om pause.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Local};
use eframe::egui;

/// Så længe uden input før vi regner brugeren for væk.
const IDLE_AFTER: StdDuration = StdDuration::from_secs(60);

/// Springer uret mere end dette frem mellem to tjek, har computeren sovet.
const SLEEP_GAP: Duration = Duration::seconds(60);

pub enum IdleEvent {
    /// Ingen input siden `since`.
    Away { since: DateTime<Local> },
    /// Input igen ved `at`.
    Back { at: DateTime<Local> },
    /// Computeren har været i dvale fra `from` til `to`.
    Slept { from: DateTime<Local>, to: DateTime<Local> },
}

pub struct IdleWatcher {
    rx: Receiver<IdleEvent>,
}

impl IdleWatcher {
    /// Starter baggrundstrådene. `ctx` bruges til at vække brugerfladen.
    pub fn start(ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        spawn_sleep_detector(tx.clone(), ctx.clone());
        platform::spawn(tx, ctx.clone());
        Self { rx }
    }

    /// Hændelser modtaget siden sidst.
    pub fn poll(&self) -> Vec<IdleEvent> {
        self.rx.try_iter().collect()
    }
}

fn send(tx: &Sender<IdleEvent>, ctx: &egui::Context, event: IdleEvent) -> bool {
    let ok = tx.send(event).is_ok();
    ctx.request_repaint();
    ok
}

/// Opdager dvale/standby: hvis uret er sprunget meget mere frem end tråden har
/// sovet, har computeren sovet i mellemtiden.
fn spawn_sleep_detector(tx: Sender<IdleEvent>, ctx: egui::Context) {
    std::thread::spawn(move || {
        // Kort interval: det sidste tjek før dvale er dér, tiden stoppes.
        let tick = StdDuration::from_secs(5);
        let mut last = Local::now();
        loop {
            std::thread::sleep(tick);
            let now = Local::now();
            if now - last > SLEEP_GAP && !send(&tx, &ctx, IdleEvent::Slept { from: last, to: now }) {
                return;
            }
            last = now;
        }
    });
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use wayland_client::protocol::{wl_registry, wl_seat};
    use wayland_client::{Connection, Dispatch, QueueHandle};
    use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{self, ExtIdleNotificationV1};
    use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;

    pub fn spawn(tx: Sender<IdleEvent>, ctx: egui::Context) {
        std::thread::spawn(move || {
            // Uden Wayland (f.eks. en ren X11-session) er der ingen registrering;
            // dvale opdages stadig af spawn_sleep_detector.
            let _ = run(tx, ctx);
        });
    }

    struct State {
        seat: Option<wl_seat::WlSeat>,
        notifier: Option<(ExtIdleNotifierV1, u32)>,
        tx: Sender<IdleEvent>,
        ctx: egui::Context,
    }

    fn run(tx: Sender<IdleEvent>, ctx: egui::Context) -> Result<(), Box<dyn std::error::Error>> {
        // Programmet kører selv via X11, men kan stadig tale med Wayland direkte.
        let conn = Connection::connect_to_env()?;
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut state = State { seat: None, notifier: None, tx, ctx };
        queue.roundtrip(&mut state)?;

        let (Some(seat), Some((notifier, version))) = (state.seat.clone(), state.notifier.clone()) else {
            return Err("ext-idle-notify understøttes ikke".into());
        };
        let timeout = IDLE_AFTER.as_millis() as u32;
        // Version 2 ser kun på input og ignorerer f.eks. videoafspilning, der
        // holder skærmen tændt; det er det vi vil, når vi spørger "var du væk?".
        let _notification = if version >= 2 {
            notifier.get_input_idle_notification(timeout, &seat, &qh, ())
        } else {
            notifier.get_idle_notification(timeout, &seat, &qh, ())
        };
        loop {
            queue.blocking_dispatch(&mut state)?;
        }
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for State {
        fn event(
            state: &mut Self,
            registry: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global { name, interface, version } = event {
                match interface.as_str() {
                    "wl_seat" if state.seat.is_none() => {
                        state.seat = Some(registry.bind(name, 1, qh, ()));
                    }
                    "ext_idle_notifier_v1" => {
                        let version = version.min(2);
                        state.notifier = Some((registry.bind(name, version, qh, ()), version));
                    }
                    _ => {}
                }
            }
        }
    }

    impl Dispatch<wl_seat::WlSeat, ()> for State {
        fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    }

    impl Dispatch<ExtIdleNotifierV1, ()> for State {
        fn event(
            _: &mut Self,
            _: &ExtIdleNotifierV1,
            _: <ExtIdleNotifierV1 as wayland_client::Proxy>::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ExtIdleNotificationV1, ()> for State {
        fn event(
            state: &mut Self,
            _: &ExtIdleNotificationV1,
            event: ext_idle_notification_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let now = Local::now();
            let event = match event {
                ext_idle_notification_v1::Event::Idled => {
                    IdleEvent::Away { since: now - Duration::from_std(IDLE_AFTER).unwrap_or_default() }
                }
                ext_idle_notification_v1::Event::Resumed => IdleEvent::Back { at: now },
                _ => return,
            };
            send(&state.tx, &state.ctx, event);
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    /// Sekunder siden sidste tastetryk eller musebevægelse.
    fn idle_seconds() -> Option<i64> {
        let mut info = LASTINPUTINFO { cbSize: size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        // SAFETY: info er en gyldig LASTINPUTINFO med cbSize sat, som API'et kræver.
        if unsafe { GetLastInputInfo(&mut info) } == 0 {
            return None;
        }
        // SAFETY: GetTickCount har ingen forudsætninger.
        let now = unsafe { GetTickCount() };
        Some(i64::from(now.wrapping_sub(info.dwTime) / 1000))
    }

    pub fn spawn(tx: Sender<IdleEvent>, ctx: egui::Context) {
        std::thread::spawn(move || {
            let limit = IDLE_AFTER.as_secs() as i64;
            let mut away = false;
            loop {
                std::thread::sleep(StdDuration::from_secs(5));
                let Some(idle) = idle_seconds() else { continue };
                let now = Local::now();
                let event = if !away && idle >= limit {
                    away = true;
                    IdleEvent::Away { since: now - Duration::seconds(idle) }
                } else if away && idle < limit {
                    away = false;
                    IdleEvent::Back { at: now - Duration::seconds(idle) }
                } else {
                    continue;
                };
                if !send(&tx, &ctx, event) {
                    return;
                }
            }
        });
    }
}

#[cfg(not(any(target_os = "linux", windows)))]
mod platform {
    use super::*;

    pub fn spawn(_: Sender<IdleEvent>, _: egui::Context) {}
}
