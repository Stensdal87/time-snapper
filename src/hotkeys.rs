//! Globale genvejstaster, der virker uanset hvilket program der har fokus.
//!
//! Windows: RegisterHotKey på en tråd med sin egen beskedløkke. Linux er ikke
//! understøttet endnu (Wayland kræver XDG Desktop Portal GlobalShortcuts).

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::shortcuts::{Action, KeyCombo};

pub struct GlobalHotkeys {
    rx: Receiver<Action>,
    /// Fejl pr. handling fra sidste registrering, f.eks. "optaget af et andet program".
    errors: Arc<Mutex<HashMap<Action, String>>>,
    inner: platform::Inner,
}

impl GlobalHotkeys {
    /// `None` hvis globale genveje ikke understøttes på dette system.
    pub fn start(ctx: &egui::Context, bindings: Vec<(Action, KeyCombo)>) -> Option<Self> {
        let (tx, rx) = mpsc::channel();
        let errors = Arc::new(Mutex::new(HashMap::new()));
        let inner = platform::Inner::start(ctx.clone(), tx, errors.clone(), bindings)?;
        Some(Self { rx, errors, inner })
    }

    /// Registrerer genvejene forfra (efter de er ændret i indstillingerne).
    pub fn set_bindings(&self, bindings: Vec<(Action, KeyCombo)>) {
        self.inner.set_bindings(bindings);
    }

    pub fn poll(&self) -> Vec<Action> {
        self.rx.try_iter().collect()
    }

    pub fn error(&self, action: Action) -> Option<String> {
        self.errors.lock().ok()?.get(&action).cloned()
    }
}

/// Windows' virtuelle tastkode for en egui-tast.
#[cfg_attr(not(windows), allow(dead_code))]
fn virtual_key(key: egui::Key) -> Option<u32> {
    use egui::Key;
    let name = key.name();
    Some(match key {
        Key::A | Key::B | Key::C | Key::D | Key::E | Key::F | Key::G | Key::H | Key::I | Key::J | Key::K | Key::L
        | Key::M | Key::N | Key::O | Key::P | Key::Q | Key::R | Key::S | Key::T | Key::U | Key::V | Key::W | Key::X
        | Key::Y | Key::Z => name.as_bytes()[0] as u32,
        Key::Num0 | Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6 | Key::Num7 | Key::Num8
        | Key::Num9 => name.as_bytes()[0] as u32,
        Key::Space => 0x20,
        Key::Enter => 0x0D,
        Key::Tab => 0x09,
        Key::Backspace => 0x08,
        Key::Escape => 0x1B,
        Key::Insert => 0x2D,
        Key::Delete => 0x2E,
        Key::Home => 0x24,
        Key::End => 0x23,
        Key::PageUp => 0x21,
        Key::PageDown => 0x22,
        Key::ArrowLeft => 0x25,
        Key::ArrowUp => 0x26,
        Key::ArrowRight => 0x27,
        Key::ArrowDown => 0x28,
        Key::Comma => 0xBC,
        Key::Minus => 0xBD,
        Key::Period => 0xBE,
        Key::Plus | Key::Equals => 0xBB,
        _ => match name.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
            Some(n @ 1..=24) => 0x70 + n - 1,
            _ => return None,
        },
    })
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::sync::mpsc::Sender;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey, UnregisterHotKey,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY,
    };

    /// Beder hotkey-tråden om at læse genvejene igen.
    const WM_RELOAD: u32 = WM_APP + 1;

    pub struct Inner {
        thread_id: u32,
        pending: Arc<Mutex<Vec<(Action, KeyCombo)>>>,
    }

    impl Inner {
        pub fn start(
            ctx: egui::Context,
            tx: Sender<Action>,
            errors: Arc<Mutex<HashMap<Action, String>>>,
            bindings: Vec<(Action, KeyCombo)>,
        ) -> Option<Self> {
            let pending = Arc::new(Mutex::new(bindings));
            let (id_tx, id_rx) = mpsc::channel();
            let thread_pending = pending.clone();
            std::thread::spawn(move || {
                // SAFETY: MSG er en almindelig C-struktur; nul er en gyldig startværdi.
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                // Opret trådens beskedkø, før tråd-id'et gives videre, så
                // PostThreadMessageW ikke går tabt.
                // SAFETY: gyldig peger til msg; null-hwnd betyder trådens egen kø.
                unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
                // SAFETY: ingen forudsætninger.
                let _ = id_tx.send(unsafe { GetCurrentThreadId() });

                let mut registered: Vec<Action> = Vec::new();
                let reload = |registered: &mut Vec<Action>| {
                    for i in 0..registered.len() {
                        // SAFETY: id'erne blev registreret af denne tråd.
                        unsafe { UnregisterHotKey(std::ptr::null_mut(), i as i32 + 1) };
                    }
                    registered.clear();
                    let mut new_errors = HashMap::new();
                    let bindings = thread_pending.lock().map(|b| b.clone()).unwrap_or_default();
                    for (action, combo) in bindings {
                        let Some(vk) = virtual_key(combo.key) else {
                            new_errors.insert(action, tr!("hotkey.unsupported_key"));
                            continue;
                        };
                        let mut mods = MOD_NOREPEAT;
                        if combo.ctrl {
                            mods |= MOD_CONTROL;
                        }
                        if combo.alt {
                            mods |= MOD_ALT;
                        }
                        if combo.shift {
                            mods |= MOD_SHIFT;
                        }
                        let id = registered.len() as i32 + 1;
                        // SAFETY: null-hwnd: WM_HOTKEY sendes til denne tråds kø.
                        if unsafe { RegisterHotKey(std::ptr::null_mut(), id, mods, vk) } != 0 {
                            registered.push(action);
                        } else {
                            new_errors.insert(action, tr!("hotkey.taken"));
                        }
                    }
                    if let Ok(mut errors) = errors.lock() {
                        *errors = new_errors;
                    }
                    ctx.request_repaint();
                };
                reload(&mut registered);

                // SAFETY: gyldig peger til msg; null-hwnd læser trådens egen kø.
                while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
                    match msg.message {
                        WM_HOTKEY => {
                            if let Some(&action) = registered.get(msg.wParam.wrapping_sub(1)) {
                                let _ = tx.send(action);
                                ctx.request_repaint();
                            }
                        }
                        WM_RELOAD => reload(&mut registered),
                        _ => {}
                    }
                }
            });
            let thread_id = id_rx.recv().ok()?;
            Some(Self { thread_id, pending })
        }

        pub fn set_bindings(&self, bindings: Vec<(Action, KeyCombo)>) {
            if let Ok(mut pending) = self.pending.lock() {
                *pending = bindings;
            }
            // SAFETY: tråd-id'et tilhører hotkey-tråden, som har en beskedkø.
            unsafe { PostThreadMessageW(self.thread_id, WM_RELOAD, 0, 0) };
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub struct Inner;

    impl Inner {
        pub fn start(
            _: egui::Context,
            _: mpsc::Sender<Action>,
            _: Arc<Mutex<HashMap<Action, String>>>,
            _: Vec<(Action, KeyCombo)>,
        ) -> Option<Self> {
            None
        }

        pub fn set_bindings(&self, _: Vec<(Action, KeyCombo)>) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_key_codes() {
        use egui::Key;
        assert_eq!(virtual_key(Key::A), Some(0x41));
        assert_eq!(virtual_key(Key::Z), Some(0x5A));
        assert_eq!(virtual_key(Key::Num0), Some(0x30));
        assert_eq!(virtual_key(Key::F1), Some(0x70));
        assert_eq!(virtual_key(Key::F12), Some(0x7B));
        assert_eq!(virtual_key(Key::F24), Some(0x87));
        assert_eq!(virtual_key(Key::F25), None);
        assert_eq!(virtual_key(Key::Space), Some(0x20));
    }
}
