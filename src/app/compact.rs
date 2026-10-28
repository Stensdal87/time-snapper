//! Den sammenfoldede visning: en lille bjælke med tid og ikonknapper, som en
//! mini-afspiller, plus en projektvælger der folder sig ud fra bjælken.
//!
//! Vinduet skifter størrelse efter hvad der skal vises (`WindowLayout`). Står
//! vinduet i den nederste halvdel af skærmen, vokser det opad, så vælgeren og
//! dialoger ikke havner uden for skærmen.

use super::*;
use crate::shortcuts::Action;

/// Hvad vinduet viser og dermed hvor stort det er.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum WindowLayout {
    /// Det fulde program.
    Normal,
    /// Kun bjælken.
    Bar,
    /// Bjælken med projektvælgeren under.
    Picker,
}

/// Projektvælgeren, der åbnes med 🔀 i bjælken (eller Ctrl+K).
#[derive(Default)]
pub(super) struct ProjectPicker {
    search: String,
    new_customer: String,
    new_project: String,
    focused: bool,
    /// Flyt den kørende tidstagning i stedet for at starte en ny (issue #10).
    moving: bool,
}

impl ProjectPicker {
    pub(super) fn moving() -> Self {
        Self { moving: true, ..Default::default() }
    }
}

/// En flytning af vinduet, som rettes til, hvis vinduesmanageren lægger
/// titellinjen oveni (se `restore_window_position`).
pub(super) struct PendingMove {
    target: Pos2,
    request: Pos2,
    at: Instant,
    tries: u8,
}

const BAR_SIZE: Vec2 = Vec2::new(400.0, 38.0);
const PICKER_HEIGHT: f32 = 420.0;
const NORMAL_SIZE: Vec2 = Vec2::new(320.0, 520.0);
const NORMAL_MIN_SIZE: Vec2 = Vec2::new(260.0, 300.0);

impl TimeSnapperApp {
    /// Hvilken visning der skal bruges lige nu. Dialoger kræver plads, så
    /// vinduet folder sig midlertidigt helt ud, mens de er åbne.
    pub(super) fn wanted_layout(&self) -> WindowLayout {
        if !self.data.compact {
            return WindowLayout::Normal;
        }
        if self.dialog_open() || self.settings_open {
            WindowLayout::Normal
        } else if self.picker.is_some() {
            WindowLayout::Picker
        } else {
            WindowLayout::Bar
        }
    }

    /// Giver vinduet den størrelse visningen kræver. Vinduet vokser opad, hvis
    /// det står i den nederste halvdel af skærmen, og holdes inden for skærmen.
    pub(super) fn apply_layout(&mut self, ctx: &egui::Context, frame: &eframe::Frame, layout: WindowLayout) {
        let Some(inner) = ctx.input(|i| i.viewport().inner_rect) else { return };
        if self.layout == WindowLayout::Normal && layout != WindowLayout::Normal {
            self.data.normal_size = Some(inner.size().into());
        }
        let (size, min_size) = match layout {
            WindowLayout::Normal => (self.data.normal_size.map_or(NORMAL_SIZE, Vec2::from), NORMAL_MIN_SIZE),
            WindowLayout::Bar => (BAR_SIZE, Vec2::new(240.0, BAR_SIZE.y)),
            WindowLayout::Picker => (Vec2::new(BAR_SIZE.x, PICKER_HEIGHT), Vec2::new(240.0, 200.0)),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(min_size));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));

        let mut top_left = inner.min;
        // Første gang ved opstart står vinduet allerede, hvor det blev gemt; dets
        // midlertidige størrelse (minimum 300 px høj) må ikke flytte det.
        if self.layout_applied
            && let Some(screen) = current_screen(ctx, frame)
        {
            let in_lower_half = inner.center().y > screen.center().y;
            let anchor_bottom = match (self.layout, layout) {
                // Bjælken folder sig ud: vælg kant efter hvor bjælken står, og husk den.
                (WindowLayout::Bar, _) => *self.bar_anchor_bottom.insert(in_lower_half),
                // Tilbage til bjælken: samme kant som da den foldede sig ud.
                (_, WindowLayout::Bar) => self.bar_anchor_bottom.unwrap_or(in_lower_half),
                _ => in_lower_half,
            };
            if anchor_bottom {
                top_left.y = inner.max.y - size.y;
            }
            top_left.x = top_left.x.clamp(screen.min.x, (screen.max.x - size.x).max(screen.min.x));
            top_left.y = top_left.y.clamp(screen.min.y, (screen.max.y - size.y).max(screen.min.y));
        }
        if (top_left - inner.min).length() > 1.0 {
            self.move_window_to(ctx, top_left);
        }
        self.layout = layout;
        self.layout_applied = true;
        self.save();
    }

    /// Flytter vinduet, så indholdet starter ved `target` (skærmpunkter).
    fn move_window_to(&mut self, ctx: &egui::Context, target: Pos2) {
        let (inner, outer) = ctx.input(|i| (i.viewport().inner_rect, i.viewport().outer_rect));
        let (Some(inner), Some(outer)) = (inner, outer) else { return };
        let request = outer.min + (target - inner.min) + self.move_correction;
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(request));
        self.pending_move = Some(PendingMove { target, request, at: Instant::now(), tries: 0 });
        ctx.request_repaint_after(StdDuration::from_millis(150));
    }

    /// Retter en flytning til, hvis vinduet ikke landede, hvor det skulle.
    pub(super) fn handle_pending_move(&mut self, ctx: &egui::Context) {
        let Some(pending) = &mut self.pending_move else { return };
        let wait = StdDuration::from_millis(150);
        if pending.at.elapsed() < wait {
            ctx.request_repaint_after(wait);
            return;
        }
        let Some(inner) = ctx.input(|i| i.viewport().inner_rect) else { return };
        let off = pending.target - inner.min;
        if off.length() <= 1.0 || pending.tries >= 3 {
            self.pending_move = None;
            return;
        }
        pending.request += off;
        pending.tries += 1;
        // Husk afvigelsen, så næste flytning lander rigtigt i første forsøg.
        self.move_correction += off;
        pending.at = Instant::now();
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pending.request));
        ctx.request_repaint_after(wait);
    }

    /// Bjælken: status, tid, projekt og små ikonknapper.
    pub(super) fn compact_bar(&mut self, ui: &mut egui::Ui) {
        let active = self.data.active.as_ref();
        let running = active.is_some();
        let on_call = active.is_some_and(|a| a.project_id.is_none());
        let on_pause = active.is_some_and(|a| a.is_note(NOTE_PAUSE));
        let (color, time, label) = match active {
            None => (ui.visuals().weak_text_color(), "0:00:00".to_owned(), "Ingen tid kører".to_owned()),
            Some(a) => {
                let color = if on_pause {
                    Color32::from_rgb(240, 170, 60)
                } else if on_call {
                    Color32::from_rgb(170, 120, 230)
                } else {
                    ACCENT
                };
                let label = match a.project_id {
                    Some(id) => self.data.project_label(id),
                    None => "📞 Telefonopkald".to_owned(),
                };
                (color, format_duration(a.elapsed()), label)
            }
        };
        let resume = self.data.resume_project().map(|id| self.data.project_label(id));
        let pause_hover = match &resume {
            Some(label) if on_pause => format!("Fortsæt på {label}"),
            _ if on_pause => "Afslut pausen".to_owned(),
            _ => "Pause (Intern tid)".to_owned(),
        };
        let call_hover = match &resume {
            Some(label) if on_call => format!("Afslut opkaldet og fortsæt på {label}"),
            _ if on_call => "Afslut opkaldet".to_owned(),
            _ => "Telefon".to_owned(),
        };

        let hint = |action| self.shortcut_hint(action);
        let (fold_hover, stop_hover, call_hover, switch_hover) = (
            format!("Fold ud til det fulde program{}", hint(Action::Fold)),
            format!("Stop{}", hint(Action::Stop)),
            format!("{call_hover}{}", hint(Action::Call)),
            format!("Skift projekt{}", hint(Action::SwitchProject)),
        );
        let pause_hover = format!("{pause_hover}{}", hint(Action::Pause));
        let mut open_picker = false;
        ui.horizontal_centered(|ui| {
            let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 5.0, color);
            ui.label(RichText::new(time).monospace().size(15.0).strong());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, "🗖", &fold_hover, true, false) {
                    self.data.compact = false;
                    self.save();
                }
                if icon_button(ui, "⏹", &stop_hover, running, false) {
                    self.data.stop();
                    self.save();
                }
                if icon_button(ui, if on_pause { "▶" } else { "☕" }, &pause_hover, true, on_pause) {
                    self.data.toggle_pause();
                    self.save();
                }
                if on_pause && self.breathing.is_none() && icon_button(ui, "💨", "Åndedrætsøvelse", true, false) {
                    self.start_breathing();
                }
                if icon_button(ui, "📞", &call_hover, true, on_call) {
                    self.data.toggle_call();
                    self.save();
                }
                if icon_button(ui, "🔀", &switch_hover, true, false) {
                    open_picker = true;
                }
                // Projektnavnet fylder resten; klik på det åbner også vælgeren.
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let response = ui
                        .add(egui::Label::new(RichText::new(&label).weak()).truncate().sense(egui::Sense::click()))
                        .on_hover_text(format!("{label}\nKlik for at skifte projekt"));
                    open_picker |= response.clicked();
                });
            });
        });

        if open_picker {
            self.picker = match self.picker {
                Some(_) => None,
                None => Some(ProjectPicker::default()),
            };
        }
    }

    /// Vælg et projekt at starte tid på, eller opret et nyt. Enter starter det
    /// eneste søgeresultat (eller opretter det nye projekt), Esc lukker.
    pub(super) fn project_picker(&mut self, ui: &mut egui::Ui) {
        let Some(mut picker) = self.picker.take() else { return };
        let active_project = self.data.active.as_ref().and_then(|a| a.project_id);
        let mut chosen = None;
        let mut create = false;
        let mut close = false;

        ui.horizontal(|ui| {
            ui.label(RichText::new(if picker.moving { "Flyt tidstagningen til" } else { "Skift projekt" }).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close |= ui.small_button("×").on_hover_text("Luk (Esc)").clicked();
            });
        });
        let response = ui.add(
            egui::TextEdit::singleline(&mut picker.search)
                .hint_text("🔍 Søg kunde eller projekt…")
                .desired_width(f32::INFINITY),
        );
        if !picker.focused {
            response.request_focus();
            picker.focused = true;
        }

        let mut matches = Vec::new();
        let complete = !picker.new_customer.trim().is_empty() && !picker.new_project.trim().is_empty();
        // "Nyt projekt" nederst; søgeresultaterne fylder pladsen ovenover.
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 90.0) / 2.0;
                ui.add(egui::TextEdit::singleline(&mut picker.new_customer).hint_text("Kunde").desired_width(width));
                ui.add(egui::TextEdit::singleline(&mut picker.new_project).hint_text("Projekt").desired_width(width));
                create |= ui.add_enabled(complete, egui::Button::new("Opret")).on_hover_text("Opret og start tid").clicked();
            });
            ui.label(RichText::new("Nyt projekt").weak().small());
            ui.separator();

            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    for customer in self.data.visible_customers() {
                        let Some(projects) = matching_projects(&self.data, customer, &picker.search) else {
                            continue;
                        };
                        if projects.is_empty() {
                            continue;
                        }
                        ui.label(RichText::new(&customer.name).strong());
                        for project in projects {
                            matches.push(project.id);
                            let is_active = active_project == Some(project.id);
                            let text =
                                if is_active { format!("   ▶ {}", project.name) } else { format!("   {}", project.name) };
                            if ui.selectable_label(is_active, text).clicked() && !is_active {
                                chosen = Some(project.id);
                            }
                        }
                    }
                    if matches.is_empty() {
                        ui.label(RichText::new("Ingen projekter matcher søgningen.").weak());
                    }
                });
            });
        });

        if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if !picker.search.trim().is_empty() && matches.len() == 1 && active_project != Some(matches[0]) {
                chosen = Some(matches[0]);
            } else if complete {
                create = true;
            }
        }
        close |= ui.input(|i| i.key_pressed(egui::Key::Escape));

        if create && chosen.is_none() {
            let target = CallTarget {
                customer: picker.new_customer.clone(),
                project: picker.new_project.clone(),
                ..Default::default()
            };
            chosen = self.data.resolve_target(&target);
        }
        if let Some(id) = chosen {
            if picker.moving {
                self.data.move_active(id);
            } else {
                self.data.start(id);
            }
            self.save();
            close = true;
        }
        if !close {
            self.picker = Some(picker);
        }
    }
}

/// En lille ikonknap til bjælken. `selected` viser den som trykket ned.
fn icon_button(ui: &mut egui::Ui, icon: &str, hover: &str, enabled: bool, selected: bool) -> bool {
    let text = RichText::new(icon).size(15.0);
    let button = if selected { egui::Button::selectable(true, text) } else { egui::Button::new(text) };
    ui.add_enabled(enabled, button.min_size(Vec2::new(28.0, 24.0))).on_hover_text(hover).clicked()
}

/// Den skærm vinduet står på, i skærmpunkter.
fn current_screen(ctx: &egui::Context, frame: &eframe::Frame) -> Option<egui::Rect> {
    let monitor = frame.winit_window()?.current_monitor()?;
    let rect = monitor_rect(&monitor);
    let ppp = ctx.pixels_per_point();
    Some(egui::Rect::from_min_max(rect.min / ppp, rect.max / ppp))
}
