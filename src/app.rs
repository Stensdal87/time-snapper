//! Brugerfladen.

mod break_reminder;
mod compact;
mod shortcuts_dialog;

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::path::PathBuf;
use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Duration, Local, NaiveDate};
use eframe::egui::{self, Color32, Pos2, RichText, Stroke, Vec2};

use crate::entry_editor::{EditorAction, EntryEditor};
use crate::export_dialog::ExportDialog;
use crate::idle::{IdleEvent, IdleWatcher};
use crate::tray::{Tray, TrayAction, TrayProject, TrayState};
use break_reminder::BreakReminder;
use compact::{PendingMove, ProjectPicker, WindowLayout};
use shortcuts_dialog::ShortcutsDialog;
use crate::hotkeys::GlobalHotkeys;
use crate::shortcuts::Action;
use crate::data::{format_duration, DayEntry, format_hours_minutes, round_down_quarter, round_up_quarter, CallTarget, Customer, Data, Id, Project, ThemeChoice, NOTE_PAUSE};

/// Hvor ofte en kørende timers "sidst set" gemmes.
const HEARTBEAT: StdDuration = StdDuration::from_secs(30);

const ACCENT: Color32 = Color32::from_rgb(70, 160, 230);

#[derive(PartialEq)]
enum Tab {
    Timer,
    History,
}

/// En kunde eller et projekt, f.eks. til sletning eller omdøbning.
#[derive(Clone, Copy)]
enum Item {
    Customer(Id),
    Project(Id),
}

/// En igangværende omdøbning af en kunde eller et projekt.
struct Rename {
    item: Item,
    name: String,
    error: Option<String>,
    focused: bool,
}

#[derive(PartialEq)]
enum HistoryView {
    /// Én linje pr. projekt.
    Summary,
    /// Hver registrering for sig.
    All,
}

pub struct TimeSnapperApp {
    data: Data,
    path: PathBuf,
    error: Option<String>,
    tab: Tab,
    always_on_top: bool,
    new_customer: String,
    /// Tekstfelter til "nyt projekt" pr. kunde.
    new_project: HashMap<Id, String>,
    history_date: NaiveDate,
    history_view: HistoryView,
    last_heartbeat: Instant,
    started: Instant,
    window_level_applied: bool,
    call_dialog_open: bool,
    /// Antal ikke-registrerede opkald sidst vi kiggede. Kommer der flere, åbnes dialogen.
    seen_unassigned: usize,
    /// Søgefelter i projektlisten og i opkaldsdialogen.
    search: String,
    dialog_search: String,
    /// "Udfold alle"/"Sammenklap alle" anvendes på kunderne i næste frame.
    set_all_open: Option<bool>,
    /// Kunde eller projekt der venter på bekræftelse af sletning.
    confirm_delete: Option<Item>,
    rename: Option<Rename>,
    /// Åben dialog til at rette/tilføje en registrering.
    editor: Option<EntryEditor>,
    export: Option<ExportDialog>,
    /// Indstillingsmenuen i venstre side.
    settings_open: bool,
    idle: IdleWatcher,
    /// Hvornår brugeren gik fra computeren, mens vi venter på at de kommer tilbage.
    away_since: Option<DateTime<Local>>,
    /// Væk-perioder før dette tidspunkt er allerede besvaret.
    away_handled_until: DateTime<Local>,
    /// Spørgsmålet "skal tiden være pause?", når brugeren er kommet tilbage.
    away_prompt: Option<AwayPrompt>,
    /// Ikonet i systembakken; `None` hvis systemet ikke har en.
    tray: Option<Tray>,
    window_visible: bool,
    /// Sat når programmet afsluttes fra systembakken, så lukning ikke skjuler vinduet.
    quitting: bool,
    /// Om programmet starter, når man logger på. Læses fra systemet ved opstart.
    autostart: bool,
    /// Den visning vinduets størrelse sidst blev sat til.
    layout: WindowLayout,
    layout_applied: bool,
    picker: Option<ProjectPicker>,
    pending_move: Option<PendingMove>,
    /// Hvor meget en flytning skal justeres, fordi vinduesmanageren lægger
    /// titellinjen oveni (KWin/X11: ca. 28 px). Læres ved opstart og ved første
    /// rettede flytning, så senere flytninger lander rigtigt med det samme.
    move_correction: Vec2,
    /// Om bjælken sidst voksede opad (bunden fast). Samme kant bruges, når den
    /// folder sig sammen igen, så den vender tilbage til samme sted.
    bar_anchor_bottom: Option<bool>,
    /// Globale genvejstaster (kun Windows indtil videre).
    hotkeys: Option<GlobalHotkeys>,
    shortcuts_dialog: Option<ShortcutsDialog>,
    /// Sæt fokus i søgefeltet i næste frame (Ctrl+F).
    focus_search: bool,
    break_reminder: BreakReminder,
}

/// En periode hvor brugeren var væk, mens der kørte tid på et projekt.
struct AwayPrompt {
    project_id: Id,
    from: DateTime<Local>,
    to: DateTime<Local>,
}

impl TimeSnapperApp {
    pub fn new(ctx: &egui::Context, path: PathBuf) -> Self {
        let (data, error) = match Data::load(&path) {
            Ok(d) => (d, None),
            // Gem aldrig oven i en fil vi ikke kunne læse.
            Err(e) => (Data::default(), Some(e)),
        };
        let mut app = Self {
            data,
            path,
            error,
            tab: Tab::Timer,
            always_on_top: true,
            new_customer: String::new(),
            new_project: HashMap::new(),
            history_date: Local::now().date_naive(),
            history_view: HistoryView::Summary,
            last_heartbeat: Instant::now(),
            started: Instant::now(),
            window_level_applied: false,
            call_dialog_open: false,
            seen_unassigned: 0,
            search: String::new(),
            dialog_search: String::new(),
            set_all_open: None,
            confirm_delete: None,
            rename: None,
            editor: None,
            export: None,
            settings_open: false,
            idle: IdleWatcher::start(ctx),
            away_since: None,
            away_handled_until: Local::now(),
            away_prompt: None,
            tray: Tray::start(ctx),
            window_visible: true,
            quitting: false,
            autostart: {
                crate::autostart::migrate_old_shortcut();
                crate::autostart::is_enabled()
            },
            layout: WindowLayout::Normal,
            layout_applied: false,
            picker: None,
            pending_move: None,
            move_correction: Vec2::ZERO,
            bar_anchor_bottom: None,
            hotkeys: None,
            shortcuts_dialog: None,
            focus_search: false,
            break_reminder: BreakReminder::new(),
        };
        app.hotkeys = GlobalHotkeys::start(ctx, app.data.settings.shortcuts.globals());
        // Var vinduet foldet sammen ved sidste lukning, har det allerede den lille
        // størrelse; den normale størrelse er gemt for sig.
        if app.data.compact {
            app.layout = WindowLayout::Bar;
        }
        ctx.set_theme(app.data.theme.to_egui());
        if app.error.is_none() {
            app.save();
        }
        app
    }

    fn save(&mut self) {
        if let Err(e) = self.data.save(&self.path) {
            self.error = Some(e);
        }
    }

    fn apply_window_level(&self, ctx: &egui::Context) {
        let level = if self.always_on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
    }

    /// eframe husker størrelse og placering, men på X11 lægger vinduesmanageren
    /// titellinjen oven i den placering vi beder om, så vinduet kryber nedad for
    /// hver genstart. eframe har bedt om den gemte placering og fået den plus en
    /// forskydning; bed derfor om den gemte placering minus forskydningen.
    fn restore_window_position(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        if self.move_to_primary_if_offscreen(ctx, frame) {
            return;
        }
        let Some([x, y]) = self.data.window_pos else { return };
        let Some(inner) = ctx.input(|i| i.viewport().inner_rect) else { return };
        let wanted = Pos2::new(x, y);
        let drift = wanted - inner.min;
        // eframe bad om den gemte placering og fik den plus titellinjen: det er
        // netop den justering, senere flytninger skal have.
        if drift.length() > 1.0 && drift.length() < 100.0 {
            self.move_correction = drift;
        }
        if drift.length() > 1.0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(wanted + drift));
        }
    }

    /// Ligger vinduet (eller den gemte placering) uden for alle tilsluttede
    /// skærme, f.eks. fordi en ekstra skærm er taget fra, flyttes det til midten
    /// af den primære skærm. Returnerer true, hvis vinduet blev flyttet.
    fn move_to_primary_if_offscreen(&mut self, ctx: &egui::Context, frame: &eframe::Frame) -> bool {
        use winit::dpi::{PhysicalPosition, PhysicalSize};

        let Some(window) = frame.winit_window() else { return false };
        let monitors: Vec<egui::Rect> = window.available_monitors().map(|m| monitor_rect(&m)).collect();
        if monitors.is_empty() {
            return false;
        }
        let size = window.outer_size();
        let size = Vec2::new(size.width as f32, size.height as f32);
        // Den placering vinduet skal have: den gemte, ellers den nuværende (i pixels).
        let ppp = ctx.pixels_per_point();
        let wanted = match self.data.window_pos {
            Some([x, y]) => Pos2::new(x * ppp, y * ppp),
            None => match window.inner_position() {
                Ok(p) => Pos2::new(p.x as f32, p.y as f32),
                Err(_) => return false,
            },
        };
        if title_bar_visible(egui::Rect::from_min_size(wanted, size), &monitors) {
            return false;
        }

        let Some(target) = window.primary_monitor().or_else(|| window.available_monitors().next()) else {
            return false;
        };
        let screen = monitor_rect(&target);
        // Større end skærmen? Gør vinduet mindre, så det kan ses helt.
        let fitted = size.min(screen.size() - Vec2::splat(80.0)).max(Vec2::splat(200.0));
        if fitted != size {
            let _ = window.request_inner_size(PhysicalSize::new(fitted.x as u32, fitted.y as u32));
        }
        let pos = screen.center() - fitted / 2.0;
        window.set_outer_position(PhysicalPosition::new(pos.x as i32, pos.y as i32));
        // Den gamle placering er ugyldig; den nye gemmes, når vinduet lukkes.
        self.data.window_pos = None;
        self.save();
        true
    }

    fn timer_tab(&mut self, ui: &mut egui::Ui) {
        if !self.call_dialog_open && !self.data.unassigned.is_empty() {
            let text = format!("📞 {} opkald mangler registrering", self.data.unassigned.len());
            if ui
                .add(egui::Button::new(RichText::new(text).color(Color32::BLACK)).fill(Color32::from_rgb(240, 190, 80)))
                .clicked()
            {
                self.call_dialog_open = true;
            }
        }
        ui.vertical_centered(|ui| {
            self.round_timer(ui);
            if self.data.active.is_none() {
                ui.label(RichText::new("Vælg et projekt for at starte").weak());
            }
        });
        ui.add_space(4.0);

        let active = self.data.active.as_ref();
        let on_call = active.is_some_and(|a| a.project_id.is_none());
        let on_pause = active.is_some_and(|a| a.is_note(NOTE_PAUSE));
        let running = active.is_some();
        let (call_hint, stop_hint) = (self.shortcut_hint(Action::Call), self.shortcut_hint(Action::Stop));
        let pause_hint = self.shortcut_hint(Action::Pause);
        let resume_label = self.data.resume_project().map(|id| self.data.project_label(id));
        let (call_text, call_hover) = match (on_call, &resume_label) {
            (true, Some(label)) => ("📞 Afslut", format!("Afslut opkaldet og fortsæt på {label}{call_hint}")),
            (true, None) => ("📞 Afslut", format!("Afslut opkaldet{call_hint}")),
            (false, _) => (
                "📞 Telefon",
                format!("Start tid på et opkald. Tryk igen for at afslutte og fortsætte, hvor du slap.{call_hint}"),
            ),
        };
        ui.columns(3, |cols| {
            let size = [cols[0].available_width(), 26.0];
            // Markeret mens opkaldet kører, ligesom pause-knappen.
            let call_button =
                if on_call { egui::Button::selectable(true, call_text) } else { egui::Button::new(call_text) };
            if cols[0].add(call_button.min_size(size.into())).on_hover_text(call_hover).clicked() {
                self.data.toggle_call();
                self.save();
            }
            let (pause_text, pause_hover) = if on_pause {
                match self.data.resume_project() {
                    Some(id) => (
                        "▶ Fortsæt".to_owned(),
                        format!("Afslut pausen og fortsæt på {}{pause_hint}", self.data.project_label(id)),
                    ),
                    None => ("☕ Afslut pause".to_owned(), format!("Afslut pausen og stop tiden{pause_hint}")),
                }
            } else {
                ("☕ Pause".to_owned(), format!("Registreres på Intern tid. Tryk igen for at fortsætte, hvor du slap.{pause_hint}"))
            };
            // Almindelig knap som de andre; markeret mens pausen kører.
            let pause_button = if on_pause {
                egui::Button::selectable(true, pause_text)
            } else {
                egui::Button::new(pause_text)
            };
            if cols[1]
                .add(pause_button.min_size(size.into()))
                .on_hover_text(pause_hover)
                .clicked()
            {
                self.data.toggle_pause();
                self.save();
            }
            if cols[2]
                .add_enabled(running, egui::Button::new("⏹ Stop").min_size(size.into()))
                .on_hover_text(format!("Stop tiden{stop_hint}"))
                .clicked()
            {
                self.data.stop();
                self.save();
            }
        });
        if let Some(active) = &mut self.data.active {
            ui.add_space(4.0);
            let changed = ui
                .add(
                    egui::TextEdit::singleline(&mut active.description)
                        .hint_text("📝 Note til denne tidstagning…")
                        .desired_width(f32::INFINITY),
                )
                .changed();
            if changed {
                self.save();
            }
        }
        if on_call {
            ui.add_space(4.0);
            self.call_target_panel(ui);
        }
        ui.separator();

        let today = Local::now().date_naive();
        let totals: HashMap<Id, Duration> = self.data.totals_on(today).into_iter().collect();
        let active_project = self.data.active.as_ref().and_then(|a| a.project_id);
        let mut start_project = None;
        let mut add_project = None;

        let search = search_field(ui, &mut self.search);
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.new_customer)
                    .hint_text("Ny kunde…")
                    .desired_width(ui.available_width() - 30.0),
            );
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("+").clicked() || enter) && !self.new_customer.trim().is_empty() {
                let name = self.new_customer.trim().to_owned();
                self.data.add_customer(&name);
                self.new_customer.clear();
                self.save();
            }
        });
        ui.add_space(4.0);
        let searching = !self.search.trim().is_empty();
        let force_open = self.set_all_open.take();
        let mut fold_all = None;
        let mut delete = None;
        let mut rename = None;

        egui::ScrollArea::vertical().show(ui, |ui| {
            let mut any_match = false;
            for customer in self.data.visible_customers() {
                let Some(projects) = matching_projects(&self.data, customer, &self.search) else {
                    continue;
                };
                any_match = true;
                let internal_customer = self.data.is_internal_customer(customer.id);
                let header = egui::CollapsingHeader::new(RichText::new(&customer.name).strong())
                    .id_salt(customer.id)
                    .default_open(true)
                    // Under søgning vises alle træffere, også under sammenklappede kunder.
                    .open(if searching { Some(true) } else { force_open })
                    .show(ui, |ui| {
                        for project in projects {
                            ui.horizontal(|ui| {
                                let is_active = active_project == Some(project.id);
                                let label = if is_active {
                                    RichText::new(format!("▶ {}", project.name)).color(ACCENT).strong()
                                } else {
                                    RichText::new(&project.name)
                                };
                                let response = ui.selectable_label(is_active, label);
                                if response.clicked() && !is_active {
                                    start_project = Some(project.id);
                                }
                                response.context_menu(|ui| {
                                    if ui.button("✏ Omdøb…").clicked() {
                                        rename = Some((Item::Project(project.id), project.name.clone()));
                                        ui.close();
                                    }
                                    let item = ui
                                        .add_enabled(!self.data.is_internal(project.id), egui::Button::new("🗑 Slet projekt…"))
                                        .on_disabled_hover_text("Intern tid kan ikke slettes");
                                    if item.clicked() {
                                        delete = Some(Item::Project(project.id));
                                        ui.close();
                                    }
                                });
                                if let Some(d) = totals.get(&project.id) {
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new(format_duration(*d)).weak().monospace());
                                    });
                                }
                            });
                        }
                        if searching {
                            return;
                        }
                        ui.horizontal(|ui| {
                            let text = self.new_project.entry(customer.id).or_default();
                            let response = ui.add(
                                egui::TextEdit::singleline(text)
                                    .hint_text("Nyt projekt…")
                                    .desired_width(ui.available_width() - 30.0),
                            );
                            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if (ui.button("+").clicked() || enter) && !text.trim().is_empty() {
                                add_project = Some((customer.id, text.trim().to_owned()));
                                text.clear();
                            }
                        });
                    });
                header.header_response.context_menu(|ui| {
                    if ui.button("⊞ Udfold alle").clicked() {
                        fold_all = Some(true);
                        ui.close();
                    }
                    if ui.button("⊟ Sammenklap alle").clicked() {
                        fold_all = Some(false);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("✏ Omdøb…").clicked() {
                        rename = Some((Item::Customer(customer.id), customer.name.clone()));
                        ui.close();
                    }
                    let item = ui
                        .add_enabled(!internal_customer, egui::Button::new("🗑 Slet kunde…"))
                        .on_disabled_hover_text("Den interne konto kan ikke slettes");
                    if item.clicked() {
                        delete = Some(Item::Customer(customer.id));
                        ui.close();
                    }
                });
            }

            if searching && !any_match {
                ui.label(RichText::new("Ingen kunder eller projekter matcher søgningen.").weak());
            }
        });

        if fold_all.is_some() {
            self.set_all_open = fold_all;
            ui.ctx().request_repaint();
        }
        if delete.is_some() {
            self.confirm_delete = delete;
        }
        if let Some((item, name)) = rename {
            self.rename = Some(Rename { item, name, error: None, focused: false });
        }

        if let Some((customer_id, name)) = add_project {
            self.data.add_project(customer_id, &name);
            self.save();
        }
        if let Some(id) = start_project {
            self.data.start(id);
            self.save();
        }
    }

    /// Rundt ur: ringen fyldes én gang i timen, prikken går rundt én gang i minuttet.
    fn round_timer(&self, ui: &mut egui::Ui) {
        let size = 150.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let center = rect.center();
        let ring_width = 12.0;
        // Ringens midterlinje. Ring, bue og prik centreres alle om denne radius.
        let radius = size / 2.0 - ring_width / 2.0 - 1.0;
        let track = ui.visuals().widgets.inactive.bg_fill;

        // egui tegner cirklers kant *udenfor* radius, så træk en halv ringbredde fra.
        painter.circle_stroke(center, radius - ring_width / 2.0, Stroke::new(ring_width, track));

        let (elapsed, label, total) = match &self.data.active {
            Some(a) => {
                let (label, total) = match a.project_id {
                    Some(id) => (self.data.project_label(id), Some(self.data.project_total(id))),
                    None => match self.target_label(&a.target) {
                        Some(target) => (format!("📞 {target}"), None),
                        None => ("📞 Telefonopkald".to_owned(), None),
                    },
                };
                let label = match (&a.note, a.project_id) {
                    (Some(note), Some(_)) => format!("{label} ({note})"),
                    _ => label,
                };
                (a.elapsed(), label, total)
            }
            None => (Duration::zero(), String::new(), None),
        };
        // Hele sekunder, så prikken tikker i takt med sekundtallet i midten.
        let secs = elapsed.num_seconds() as f32;

        if self.data.active.is_some() {
            let hour_fraction = (secs % 3600.0) / 3600.0;
            let point_at = |fraction: f32| {
                let angle = fraction * TAU - TAU / 4.0; // start kl. 12
                center + Vec2::angled(angle) * radius
            };
            let steps = (hour_fraction * 100.0).ceil().max(1.0) as usize;
            let arc: Vec<Pos2> = (0..=steps)
                .map(|i| point_at(hour_fraction * i as f32 / steps as f32))
                .collect();
            painter.add(egui::Shape::line(arc, Stroke::new(ring_width - 4.0, ACCENT)));
            painter.circle_filled(point_at((secs % 60.0) / 60.0), ring_width / 2.0 - 2.0, Color32::WHITE);
        }

        let text_color = ui.visuals().strong_text_color();
        let current_pos = if total.is_some() { center - Vec2::new(0.0, 8.0) } else { center };
        painter.text(
            current_pos,
            egui::Align2::CENTER_CENTER,
            format_duration(elapsed),
            egui::FontId::monospace(24.0),
            text_color,
        );
        if let Some(total) = total {
            painter.text(
                center + Vec2::new(0.0, 16.0),
                egui::Align2::CENTER_CENTER,
                format!("I alt {}", format_duration(total)),
                egui::FontId::monospace(12.0),
                ui.visuals().weak_text_color(),
            );
        }
        if !label.is_empty() {
            ui.label(RichText::new(label).strong());
        }
    }

    fn history_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("◀").clicked() {
                self.history_date -= Duration::days(1);
            }
            ui.label(RichText::new(self.history_date.format("%d-%m-%Y").to_string()).strong());
            if ui.button("▶").clicked() {
                self.history_date += Duration::days(1);
            }
            if ui.button("I dag").clicked() {
                self.history_date = Local::now().date_naive();
            }
        });
        ui.separator();

        let entries = self.data.entries_on(self.history_date);
        let add_clicked = ui.horizontal(|ui| {
            ui.selectable_value(&mut self.history_view, HistoryView::Summary, "Summeret");
            ui.selectable_value(&mut self.history_view, HistoryView::All, "Alle registreringer");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("📤").on_hover_text("Eksportér til Excel/CSV").clicked() {
                    self.export = Some(ExportDialog::new());
                }
                ui.button("➕").on_hover_text("Tilføj registrering").clicked()
            })
            .inner
        });
        if add_clicked.inner {
            let last_end = entries.iter().filter(|e| e.index.is_some()).map(|e| e.entry.end).max();
            self.editor = Some(EntryEditor::new(self.history_date, last_end));
        }
        ui.add_space(4.0);

        if entries.is_empty() {
            ui.label(RichText::new("Ingen registreringer denne dag.").weak());
            return;
        }

        let totals = self.data.totals_on(self.history_date);
        let day_total = totals.iter().fold(Duration::zero(), |sum, (_, d)| sum + *d);
        let mut open_editor = None;
        let mut toggle_rounding = None;
        let date = self.history_date;

        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("history").striped(true).show(ui, |ui| {
                match self.history_view {
                    HistoryView::Summary => {
                        ui.label("");
                        ui.label("");
                        ui.label(RichText::new("Tid").weak().small());
                        ui.label(RichText::new("Afrundet").weak().small())
                            .on_hover_text("Rundet op til nærmeste påbegyndte kvarter");
                        ui.end_row();
                        for (project_id, d) in &totals {
                            let count = entries.iter().filter(|e| e.entry.project_id == *project_id).count();
                            let notes = day_notes(&entries, *project_id);
                            let response = ui.label(self.data.project_label(*project_id));
                            if !notes.is_empty() {
                                response.on_hover_text(format!("Noter:\n• {}", notes.join("\n• ")));
                            }
                            ui.label(RichText::new(format!("{count}×")).weak())
                                .on_hover_text(format!("{count} registreringer"));
                            ui.label(RichText::new(format_duration(*d)).monospace());
                            // Klik på den afrundede tid for at runde ned i stedet for op.
                            let r = self.data.rounded(date, *project_id, *d);
                            let mut text = RichText::new(format_hours_minutes(r.time)).monospace();
                            if r.down {
                                text = RichText::new(format!("{} ↓", format_hours_minutes(r.time)))
                                    .monospace()
                                    .color(ui.visuals().warn_fg_color);
                            }
                            let hover = match (r.can_toggle, r.down) {
                                (false, _) => "Præcis på et kvarter".to_owned(),
                                (true, false) => format!(
                                    "Rundet op fra {}. Klik for at runde ned til {}",
                                    format_duration(*d),
                                    format_hours_minutes(round_down_quarter(*d))
                                ),
                                (true, true) => format!(
                                    "Rundet ned manuelt fra {}. Klik for at runde op til {}",
                                    format_duration(*d),
                                    format_hours_minutes(round_up_quarter(*d))
                                ),
                            };
                            if r.can_toggle {
                                let response = ui.add(egui::Button::new(text).frame(false)).on_hover_text(hover);
                                if response.clicked() {
                                    toggle_rounding = Some(*project_id);
                                }
                            } else {
                                ui.label(text).on_hover_text(hover);
                            }
                            ui.end_row();
                        }
                    }
                    HistoryView::All => {
                        for day_entry in &entries {
                            let e = &day_entry.entry;
                            let mut label = self.data.project_label(e.project_id);
                            if let Some(note) = &e.note {
                                label = format!("{label} ({note})");
                            }
                            ui.vertical(|ui| {
                                match day_entry.index {
                                    Some(index) => {
                                        let response = ui.selectable_label(false, label).on_hover_text("Klik for at rette");
                                        if response.clicked() {
                                            open_editor = Some(EntryEditor::edit(index, e));
                                        }
                                        response.context_menu(|ui| {
                                            if ui.button("✏ Ret…").clicked() {
                                                open_editor = Some(EntryEditor::edit(index, e));
                                                ui.close();
                                            }
                                            if ui.button("🗑 Slet…").clicked() {
                                                open_editor = Some(EntryEditor::delete(index, e));
                                                ui.close();
                                            }
                                        });
                                    }
                                    // Den kørende timer kan ikke rettes, før den er stoppet.
                                    None => {
                                        ui.label(RichText::new(format!("▶ {label}")).color(ACCENT))
                                            .on_hover_text("Kører stadig. Stop timeren for at rette den.");
                                    }
                                }
                                if !e.description.is_empty() {
                                    ui.label(RichText::new(format!("📝 {}", shorten(&e.description, 40))).weak().small())
                                        .on_hover_text(&e.description);
                                }
                            });
                            ui.label(
                                RichText::new(format!("{}–{}", e.start.format("%H:%M"), e.end.format("%H:%M")))
                                    .monospace()
                                    .weak(),
                            );
                            ui.label(RichText::new(format_duration(e.duration())).monospace());
                            ui.end_row();
                        }
                    }
                }
                ui.label(RichText::new("I alt").strong());
                ui.label("");
                ui.label(RichText::new(format_duration(day_total)).monospace().strong());
                if self.history_view == HistoryView::Summary {
                    // Summen af de afrundede projekttider, som når der faktureres pr. projekt.
                    let rounded = totals
                        .iter()
                        .fold(Duration::zero(), |sum, (id, d)| sum + self.data.rounded(date, *id, *d).time);
                    ui.label(RichText::new(format_hours_minutes(rounded)).monospace().strong())
                        .on_hover_text("Summen af de afrundede tider pr. projekt");
                }
                ui.end_row();
            });
        });

        if open_editor.is_some() {
            self.editor = open_editor;
        }
        if let Some(project_id) = toggle_rounding {
            self.data.toggle_round_down(date, project_id);
            self.save();
        }
    }

    fn entry_editor(&mut self, ctx: &egui::Context) {
        let Some(editor) = self.editor.as_mut() else { return };
        match editor.show(ctx, &self.data) {
            EditorAction::None => return,
            EditorAction::Cancel => {}
            EditorAction::Save(Some(index), entry) => self.data.update_entry(index, entry),
            EditorAction::Save(None, entry) => self.data.add_entry(entry),
            EditorAction::Delete(index) => self.data.delete_entry(index),
        }
        self.editor = None;
        self.save();
    }
}

impl eframe::App for TimeSnapperApp {
    /// Kaldes før hver `ui` og også mens vinduet er skjult i systembakken, så
    /// timeren, systembakken og "væk"-registreringen virker uden vinduet.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_tray_actions(ctx);
        let global_actions = self.hotkeys.as_ref().map(|h| h.poll()).unwrap_or_default();
        for action in global_actions {
            self.run_action(ctx, action, true);
        }
        self.handle_idle_events();

        if let Some(active) = &mut self.data.active {
            if self.last_heartbeat.elapsed() >= HEARTBEAT {
                active.last_seen = Local::now();
                self.last_heartbeat = Instant::now();
                self.save();
            }
            // Holder systembakkens status opdateret, også når vinduet er skjult.
            ctx.request_repaint_after(StdDuration::from_secs(1));
        }

        let pending = self.data.unassigned.len();
        if pending > self.seen_unassigned {
            self.call_dialog_open = true;
        }
        self.seen_unassigned = pending;

        // Et spørgsmål skal kunne ses: vis vinduet, hvis det er skjult.
        self.update_break_reminder(ctx);
        let needs_answer = self.away_prompt.is_some()
            || (self.call_dialog_open && pending > 0)
            || self.break_reminder.is_open();
        if needs_answer && !self.window_visible {
            self.show_window(ctx);
        }
        self.update_tray();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.tray.is_some() && self.data.settings.close_to_tray && !self.quitting {
                // Luk til systembakken: tiden kører videre.
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide_window(&ctx);
            } else if !self.quitting {
                self.prepare_quit(&ctx);
            }
        }

        // På X11 går "hold øverst" tabt, hvis det sættes før vinduet er vist,
        // så det sendes igen kort efter opstart.
        if !self.window_level_applied {
            let delay = StdDuration::from_millis(500);
            if self.started.elapsed() >= delay {
                self.apply_window_level(&ctx);
                self.restore_window_position(&ctx, frame);
                self.window_level_applied = true;
            } else {
                ctx.request_repaint_after(delay);
            }
        }

        // Tegn igen præcis når uret skifter sekund, så prikken tikker jævnt.
        if let Some(active) = &self.data.active {
            let into_second = active.elapsed().num_milliseconds().rem_euclid(1000) as u64;
            ctx.request_repaint_after(StdDuration::from_millis(1000 - into_second + 5));
        }

        // Tooltips må ikke blive bredere end vinduet; ellers kan de ikke læses
        // i det smalle vindue. Lange tekster brydes i stedet om.
        let tooltip_width = (ctx.content_rect().width() - 16.0).clamp(120.0, 320.0);
        if ctx.global_style().spacing.tooltip_width != tooltip_width {
            ctx.all_styles_mut(|style| style.spacing.tooltip_width = tooltip_width);
        }

        if !self.dialog_open() {
            for action in self.data.settings.shortcuts.take_local_actions(&ctx) {
                self.run_action(&ctx, action, false);
            }
        }

        let layout = self.wanted_layout();
        if self.window_level_applied && (!self.layout_applied || layout != self.layout) {
            self.apply_layout(&ctx, frame, layout);
        }
        self.handle_pending_move(&ctx);
        match layout {
            WindowLayout::Normal => self.full_ui(ui, &ctx),
            WindowLayout::Bar => {
                egui::CentralPanel::default()
                    .frame(egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 4)))
                    .show(ui, |ui| self.compact_bar(ui));
            }
            WindowLayout::Picker => {
                egui::Panel::top("compact_bar")
                    .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 4)))
                    .show(ui, |ui| self.compact_bar(ui));
                egui::CentralPanel::default().show(ui, |ui| self.project_picker(ui));
            }
        }

        if self.call_dialog_open && !self.data.unassigned.is_empty() {
            self.call_dialog(&ctx);
        }
        if let Some(target) = self.confirm_delete {
            self.delete_dialog(&ctx, target);
        }
        self.entry_editor(&ctx);
        self.away_dialog(&ctx);
        self.break_dialog(&ctx);
        self.rename_dialog(&ctx);
        self.shortcuts_dialog(&ctx);
        if layout == WindowLayout::Normal && self.picker.is_some() {
            let modal = egui::Modal::new(egui::Id::unique("picker_modal")).show(&ctx, |ui| {
                ui.allocate_ui(Vec2::new(320.0, 380.0), |ui| {
                    ui.set_min_size(Vec2::new(320.0, 380.0));
                    self.project_picker(ui);
                });
            });
            if modal.should_close() {
                self.picker = None;
            }
        }
        if let Some(export) = self.export.as_mut()
            && !export.show(&ctx, &self.data, frame, self.always_on_top)
        {
            self.export = None;
        }
    }
}

impl TimeSnapperApp {
    /// Det fulde program: faner, indstillinger og Timer/Historik.
    fn full_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.toggle_value(&mut self.settings_open, "☰").on_hover_text("Indstillinger");
                ui.selectable_value(&mut self.tab, Tab::Timer, "⏱ Timer");
                // Historikken åbner altid i den summerede visning.
                if ui.selectable_value(&mut self.tab, Tab::History, "📅 Historik").changed() {
                    self.history_view = HistoryView::Summary;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let fold_hint = self.shortcut_hint(Action::Fold);
                    if ui.button("🗕").on_hover_text(format!("Fold sammen til en lille bjælke{fold_hint}")).clicked() {
                        self.data.compact = true;
                        self.save();
                    }
                    if ui
                        .toggle_value(&mut self.always_on_top, "📌")
                        .on_hover_text("Hold vinduet øverst")
                        .changed()
                    {
                        self.apply_window_level(ctx);
                    }
                });
            });
        });

        if let Some(error) = self.error.clone() {
            egui::Panel::bottom("error").show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if ui.small_button("✕").clicked() {
                        self.error = None;
                    }
                });
            });
        }

        let mut settings_open = self.settings_open;
        egui::Panel::left("settings")
            .resizable(false)
            .drag_to_open(false)
            .exact_size(210.0)
            .show_collapsible(ui, &mut settings_open, |ui| self.settings_panel(ui, ctx));
        self.settings_open = settings_open;

        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Timer => self.timer_tab(ui),
            Tab::History => self.history_tab(ui),
        });
    }
}

impl TimeSnapperApp {
    /// Spørger hvor det ældste ikke-registrerede telefonopkald skal registreres.
    /// Enter registrerer (når et mål er udfyldt), Esc lukker dialogen uden at
    /// miste opkaldet.
    fn call_dialog(&mut self, ctx: &egui::Context) {
        let call = self.data.unassigned[0].clone();
        let mut target = call.target.clone();
        let internal = self.data.internal_project_id();
        let internal_name = self.data.project(internal).map_or_else(String::new, |p| p.name.clone());
        let mut confirm = false;
        let mut discard = false;

        let modal = egui::Modal::new(egui::Id::unique("call_dialog")).show(ctx, |ui| {
            ui.set_width(270.0);
            ui.heading("📞 Telefonopkald");
            ui.label(format!(
                "{}  {}–{}  ({})",
                call.start.format("%d-%m"),
                call.start.format("%H:%M"),
                call.end.format("%H:%M"),
                format_duration(call.duration()),
            ));
            if self.data.unassigned.len() > 1 {
                ui.label(RichText::new(format!("{} opkald mere venter", self.data.unassigned.len() - 1)).weak());
            }
            ui.label("Hvor skal tiden registreres?");
            ui.separator();

            let full = Vec2::new(ui.available_width(), 24.0);
            let response = ui.add(
                egui::Button::selectable(target.project_id == Some(internal), RichText::new(internal_name).strong())
                    .min_size(full),
            );
            if response.clicked() {
                target = CallTarget { project_id: Some(internal), ..Default::default() };
            }
            confirm |= response.double_clicked();
            ui.add_space(4.0);

            search_field(ui, &mut self.dialog_search);
            let mut matches = Vec::new();
            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                for customer in self.data.visible_customers() {
                    let Some(projects) = matching_projects(&self.data, customer, &self.dialog_search) else {
                        continue;
                    };
                    let projects: Vec<_> = projects.into_iter().filter(|p| p.id != internal).collect();
                    if projects.is_empty() {
                        continue;
                    }
                    matches.extend(projects.iter().map(|p| p.id));
                    ui.label(RichText::new(&customer.name).strong());
                    for project in projects {
                        let response = ui.selectable_label(target.project_id == Some(project.id), format!("   {}", project.name));
                        if response.clicked() {
                            target = CallTarget { project_id: Some(project.id), ..Default::default() };
                        }
                        confirm |= response.double_clicked();
                    }
                }
            });

            ui.separator();
            ui.label(RichText::new("Eller skriv kunde og projekt (oprettes hvis de ikke findes):").weak());
            typed_target_fields(ui, &mut target);
            ui.add_space(4.0);

            let complete = self.data.target_is_complete(&target);
            ui.horizontal(|ui| {
                confirm |= ui
                    .add_enabled(complete, egui::Button::new("✔ Registrér"))
                    .on_hover_text("Enter")
                    .clicked();
                if ui.button("Senere").on_hover_text("Esc").clicked() {
                    ui.close();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("🗑 Kassér").on_hover_text("Opkaldet registreres ikke").clicked() {
                        discard = true;
                    }
                });
            });
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                // Har søgningen kun ét træf, vælger Enter det.
                if !complete && !self.dialog_search.trim().is_empty() && matches.len() == 1 {
                    target = CallTarget { project_id: Some(matches[0]), ..Default::default() };
                }
                confirm = true;
            }
            confirm &= self.data.target_is_complete(&target);
        });

        if modal.should_close() {
            self.call_dialog_open = false;
        }
        if target != call.target {
            self.data.unassigned[0].target = target;
            self.save();
        }
        if confirm && self.data.register_first_unassigned() {
            self.dialog_search.clear();
            self.save();
        } else if discard {
            self.data.discard_first_unassigned();
            self.dialog_search.clear();
            self.save();
        }
    }

    /// Omdøb en kunde eller et projekt. Enter gemmer, Esc annullerer.
    fn rename_dialog(&mut self, ctx: &egui::Context) {
        let Some(Rename { item, name, error, focused }) = self.rename.as_mut() else { return };
        let item = *item;
        let title = match item {
            Item::Customer(_) => "✏ Omdøb kunde".to_owned(),
            Item::Project(id) => {
                let customer = self.data.project(id).and_then(|p| self.data.customer(p.customer_id));
                format!("✏ Omdøb projekt hos {}", customer.map_or("?", |c| c.name.as_str()))
            }
        };
        let mut save = false;
        let mut cancel = false;

        let modal = egui::Modal::new(egui::Id::unique("rename_dialog")).show(ctx, |ui| {
            ui.set_width(260.0);
            ui.heading(title);
            ui.add_space(4.0);
            let response = ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
            // Markér feltet, når dialogen åbner, så man kan skrive med det samme.
            if !*focused {
                response.request_focus();
                *focused = true;
            }
            if response.changed() {
                *error = None;
            }
            if let Some(e) = error.as_ref() {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                save |= ui.button("✔ Gem").on_hover_text("Enter").clicked();
                cancel |= ui.button("Annuller").on_hover_text("Esc").clicked();
            });
            save |= ui.input(|i| i.key_pressed(egui::Key::Enter));
        });
        cancel |= modal.should_close();

        if save {
            let result = match item {
                Item::Customer(id) => self.data.rename_customer(id, name),
                Item::Project(id) => self.data.rename_project(id, name),
            };
            match result {
                Ok(()) => {
                    self.rename = None;
                    self.save();
                }
                Err(e) => *error = Some(e),
            }
        } else if cancel {
            self.rename = None;
        }
    }

    /// Spørger om man er sikker, før en kunde eller et projekt slettes.
    fn delete_dialog(&mut self, ctx: &egui::Context, target: Item) {
        let (title, name, details) = match target {
            Item::Customer(id) => {
                let name = self.data.customer(id).map_or_else(String::new, |c| c.name.clone());
                let projects = self.data.visible_projects().filter(|p| p.customer_id == id).count();
                let (count, total) = self.data.customer_stats(id);
                let details = format!(
                    "Kundens {projects} projekt(er) slettes også. {count} registrering(er) ({}) bevares i historikken.",
                    format_duration(total)
                );
                ("Slet kunde", name, details)
            }
            Item::Project(id) => {
                let (count, total) = self.data.project_stats(id);
                let details = format!("{count} registrering(er) ({}) bevares i historikken.", format_duration(total));
                ("Slet projekt", self.data.project_label(id), details)
            }
        };
        let affects_timer = self.data.active.as_ref().and_then(|a| a.project_id).is_some_and(|id| match target {
            Item::Project(p) => p == id,
            Item::Customer(c) => self.data.project(id).is_some_and(|p| p.customer_id == c),
        });

        let mut confirmed = false;
        let modal = egui::Modal::new(egui::Id::unique("delete_dialog")).show(ctx, |ui| {
            ui.set_width(260.0);
            ui.heading(format!("🗑 {title}"));
            ui.label(format!("Er du sikker på, at du vil slette \"{name}\"?"));
            ui.add_space(4.0);
            ui.label(RichText::new(details).weak());
            if affects_timer {
                ui.label(RichText::new("Timeren kører på dette projekt og bliver stoppet.").color(ui.visuals().warn_fg_color));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let delete = egui::Button::new(RichText::new("🗑 Slet").color(Color32::WHITE))
                    .fill(Color32::from_rgb(200, 60, 60));
                if ui.add(delete).clicked() {
                    confirmed = true;
                }
                if ui.button("Annuller").on_hover_text("Esc").clicked() {
                    ui.close();
                }
            });
        });

        if confirmed {
            match target {
                Item::Customer(id) => self.data.delete_customer(id),
                Item::Project(id) => self.data.delete_project(id),
            }
            self.confirm_delete = None;
            self.save();
        } else if modal.should_close() {
            self.confirm_delete = None;
        }
    }

    /// Valg af projekt mens et opkald kører, så det registreres direkte når det slutter.
    fn call_target_panel(&mut self, ui: &mut egui::Ui) {
        let Some(active) = &self.data.active else { return };
        let mut target = active.target.clone();
        let internal = self.data.internal_project_id();

        ui.label(RichText::new("Registrér opkaldet på:").weak());
        let selected = match target.project_id {
            Some(id) => self.data.project_label(id),
            None => "Vælg eksisterende projekt…".to_owned(),
        };
        egui::ComboBox::from_id_salt("call_target")
            .width(ui.available_width() - 8.0)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                let ids = std::iter::once(internal)
                    .chain(self.data.visible_projects().map(|p| p.id).filter(|id| *id != internal));
                for id in ids {
                    if ui.selectable_label(target.project_id == Some(id), self.data.project_label(id)).clicked() {
                        target = CallTarget { project_id: Some(id), ..Default::default() };
                    }
                }
            });
        typed_target_fields(ui, &mut target);

        if let Some(active) = &mut self.data.active
            && active.target != target
        {
            active.target = target;
            self.save();
        }
    }

    /// "Kunde / Projekt" for et opkalds mål, hvis det er udfyldt.
    fn target_label(&self, target: &CallTarget) -> Option<String> {
        if let Some(id) = target.project_id {
            return Some(self.data.project_label(id));
        }
        let (customer, project) = (target.customer.trim(), target.project.trim());
        (!customer.is_empty() && !project.is_empty()).then(|| format!("{customer} / {project}"))
    }
}

impl TimeSnapperApp {
    /// Om en dialog er åben (så genveje og bjælken venter).
    fn dialog_open(&self) -> bool {
        (self.call_dialog_open && !self.data.unassigned.is_empty())
            || self.away_prompt.is_some()
            || self.confirm_delete.is_some()
            || self.editor.is_some()
            || self.rename.is_some()
            || self.export.is_some()
            || self.shortcuts_dialog.is_some()
            || self.break_reminder.is_open()
    }

    /// Udfører en handling fra en genvejstast. `global` er når genvejen blev
    /// trykket i et andet program; så hentes vinduet frem, hvis det skal bruges.
    fn run_action(&mut self, ctx: &egui::Context, action: Action, global: bool) {
        match action {
            Action::SwitchProject => {
                if global {
                    self.show_window(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                self.picker = match (&self.picker, global) {
                    (Some(_), false) => None,
                    _ => Some(ProjectPicker::default()),
                };
            }
            Action::Pause => self.data.toggle_pause(),
            Action::Call => self.data.toggle_call(),
            Action::Stop => self.data.stop(),
            Action::ToggleWindow => {
                if self.window_visible {
                    self.hide_window(ctx);
                } else {
                    self.show_window(ctx);
                }
            }
            Action::Fold => self.data.compact = !self.data.compact,
            Action::Search => {
                if self.data.compact {
                    self.picker = Some(ProjectPicker::default());
                } else {
                    self.tab = Tab::Timer;
                    self.focus_search = true;
                }
            }
            Action::TimerTab | Action::HistoryTab => {
                self.data.compact = false;
                self.tab = if action == Action::TimerTab { Tab::Timer } else { Tab::History };
                self.history_view = HistoryView::Summary;
            }
            Action::Export => {
                self.data.compact = false;
                self.export = Some(ExportDialog::new());
            }
        }
        self.save();
        ctx.request_repaint();
    }

    /// " (Ctrl+P)" til tooltips, hvis handlingen har en genvej i programmet.
    fn shortcut_hint(&self, action: Action) -> String {
        self.data.settings.shortcuts.local(action).map_or_else(String::new, |c| format!(" ({c})"))
    }

    fn show_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.window_visible = true;
        // Når vinduet vises igen, går "hold øverst" og placeringen tabt på X11;
        // sæt dem igen efter et øjeblik, som ved opstart.
        self.started = Instant::now();
        self.window_level_applied = false;
        ctx.request_repaint();
    }

    fn hide_window(&mut self, ctx: &egui::Context) {
        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.data.window_pos = Some([rect.min.x, rect.min.y]);
            self.save();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.window_visible = false;
    }

    /// Stopper timeren og gemmer, inden programmet lukker.
    fn prepare_quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        self.data.stop();
        if self.window_visible
            && let Some(rect) = ctx.input(|i| i.viewport().inner_rect)
        {
            self.data.window_pos = Some([rect.min.x, rect.min.y]);
        }
        self.save();
    }

    fn handle_tray_actions(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else { return };
        for action in tray.poll() {
            match action {
                TrayAction::Toggle if self.window_visible => self.hide_window(ctx),
                TrayAction::Toggle => self.show_window(ctx),
                TrayAction::Start(id) => self.data.start(id),
                TrayAction::Call => self.data.toggle_call(),
                TrayAction::Pause => self.data.toggle_pause(),
                TrayAction::Stop => self.data.stop(),
                TrayAction::Quit => {
                    self.prepare_quit(ctx);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            self.save();
        }
    }

    /// Beskriver systembakkens menu ud fra den aktuelle tilstand.
    fn update_tray(&mut self) {
        if self.tray.is_none() {
            return;
        }
        let active = self.data.active.as_ref();
        let active_project = active.and_then(|a| a.project_id);
        let status = match active {
            None => "Ingen tid kører".to_owned(),
            Some(a) => {
                let what = match a.project_id {
                    Some(id) => self.data.project_label(id),
                    None => "📞 Telefonopkald".to_owned(),
                };
                let what = match &a.note {
                    Some(note) if a.project_id.is_some() => format!("{what} ({note})"),
                    _ => what,
                };
                format!("▶ {what} · {}", format_duration(a.elapsed()))
            }
        };
        let on_pause = active.is_some_and(|a| a.is_note(NOTE_PAUSE));
        let on_call = active.is_some_and(|a| a.is_call());
        let resume = self.data.resume_project().map(|id| self.data.project_label(id));
        let pause_label = match (on_pause, &resume) {
            (false, _) => "☕ Pause".to_owned(),
            (true, Some(label)) => format!("▶ Fortsæt på {label}"),
            (true, None) => "☕ Afslut pause".to_owned(),
        };
        let call_label = match (on_call, &resume) {
            (false, _) => "📞 Telefon".to_owned(),
            (true, Some(label)) => format!("📞 Afslut opkald og fortsæt på {label}"),
            (true, None) => "📞 Afslut opkald".to_owned(),
        };
        let customers = self
            .data
            .visible_customers()
            .map(|c| {
                let projects = self
                    .data
                    .visible_projects()
                    .filter(|p| p.customer_id == c.id)
                    .map(|p| TrayProject { id: p.id, name: p.name.clone(), active: active_project == Some(p.id) })
                    .collect::<Vec<_>>();
                (c.name.clone(), projects)
            })
            .filter(|(_, projects)| !projects.is_empty())
            .collect();
        let state = TrayState {
            status,
            running: active.is_some(),
            call_label,
            pause_label,
            window_visible: self.window_visible,
            customers,
        };
        if let Some(tray) = &mut self.tray {
            tray.update(state);
        }
    }

    fn settings_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(6.0);
        ui.heading("Indstillinger");
        ui.add_space(10.0);

        ui.label(RichText::new("Tema").strong());
        let mut theme = self.data.theme;
        ui.radio_value(&mut theme, ThemeChoice::System, "💻 Følg systemet");
        ui.radio_value(&mut theme, ThemeChoice::Light, "☀ Lyst");
        ui.radio_value(&mut theme, ThemeChoice::Dark, "🌙 Mørkt");
        if theme != self.data.theme {
            self.data.theme = theme;
            ctx.set_theme(theme.to_egui());
            self.save();
        }
        ui.add_space(14.0);

        ui.label(RichText::new("Start").strong());
        let mut autostart = self.autostart;
        if ui.checkbox(&mut autostart, "Start Time Snapper, når jeg logger på").changed() {
            if let Err(e) = crate::autostart::set_enabled(autostart) {
                self.error = Some(e);
            }
            self.autostart = crate::autostart::is_enabled();
        }
        ui.add_space(14.0);

        ui.label(RichText::new("Pausepåmindelse").strong());
        let mut settings = self.data.settings.clone();
        ui.checkbox(&mut settings.break_reminder, "Mind mig om at holde en kort pause");
        ui.add_enabled_ui(settings.break_reminder, |ui| {
            ui.horizontal(|ui| {
                ui.label("Efter");
                ui.add(egui::Slider::new(&mut settings.break_minutes, 15..=180).step_by(5.0).suffix(" min"));
            });
        });
        ui.label(
            RichText::new("Arbejdstid tælles fra sidste pause, eller fra du har været væk eller haft tiden stoppet i mindst 5 min.")
                .weak()
                .small(),
        );
        if settings != self.data.settings {
            self.data.settings = settings;
            self.save();
        }
        ui.add_space(14.0);

        ui.label(RichText::new("Væk fra computeren").strong());
        let mut settings = self.data.settings.clone();
        ui.checkbox(&mut settings.away_enabled, "Spørg om tiden skal være pause, når jeg har været væk");
        ui.add_enabled_ui(settings.away_enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label("Efter");
                ui.add(egui::Slider::new(&mut settings.away_minutes, 5..=60).step_by(5.0).suffix(" min"));
            });
        });
        ui.label(
            RichText::new("Gælder når der kører tid på et projekt, og man ikke har rørt mus eller tastatur.")
                .weak()
                .small(),
        );
        ui.add_space(14.0);
        ui.label(RichText::new("Genvejstaster").strong());
        if ui.button("⌨ Genvejstaster…").clicked() {
            self.shortcuts_dialog = Some(ShortcutsDialog::default());
        }

        if self.tray.is_some() {
            ui.add_space(14.0);
            ui.label(RichText::new("Systembakke").strong());
            ui.checkbox(&mut settings.close_to_tray, "Luk til systembakken (tiden kører videre)");
            ui.label(
                RichText::new("Afslut helt via ikonet i systembakken. Venstreklik på ikonet viser vinduet.")
                    .weak()
                    .small(),
            );
        }

        if settings != self.data.settings {
            self.data.settings = settings;
            self.save();
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(format!("Time Snapper {}", env!("CARGO_PKG_VERSION"))).weak().small());
        });
    }

    fn handle_idle_events(&mut self) {
        for event in self.idle.poll() {
            match event {
                IdleEvent::Away { since } => {
                    if self.away_since.is_none_or(|s| since < s) {
                        self.away_since = Some(since);
                    }
                }
                IdleEvent::Back { at } => {
                    if let Some(since) = self.away_since.take() {
                        self.break_reminder.note_away(since, at);
                        self.consider_away(since, at);
                    }
                }
            }
        }
    }

    /// Spørg om pause, hvis brugeren har været væk længe nok, mens der kørte
    /// tid på et projekt (ikke under en pause eller et opkald uden projekt).
    fn consider_away(&mut self, since: DateTime<Local>, at: DateTime<Local>) {
        let settings = &self.data.settings;
        let Some(active) = &self.data.active else { return };
        let Some(project_id) = active.project_id else { return };
        if !settings.away_enabled || active.is_note(NOTE_PAUSE) {
            return;
        }
        let from = since.max(active.start).max(self.away_handled_until);
        if at - from < Duration::minutes(i64::from(settings.away_minutes)) {
            return;
        }
        match &mut self.away_prompt {
            // Flere meldinger om samme fravær (f.eks. både inaktivitet og dvale) slås sammen.
            Some(prompt) if prompt.project_id == project_id => {
                prompt.from = prompt.from.min(from);
                prompt.to = prompt.to.max(at);
            }
            _ => self.away_prompt = Some(AwayPrompt { project_id, from, to: at }),
        }
    }

    /// "Velkommen tilbage"-dialogen. Enter registrerer pause, Esc beholder tiden på projektet.
    fn away_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = &self.away_prompt else { return };
        let (project_id, from, to) = (prompt.project_id, prompt.from, prompt.to);
        let minutes = (to - from).num_minutes();
        let length = if minutes < 60 {
            format!("{minutes} min")
        } else {
            format!("{} t {:02} min", minutes / 60, minutes % 60)
        };
        let mut answer = None;

        let modal = egui::Modal::new(egui::Id::unique("away_dialog")).show(ctx, |ui| {
            ui.set_width(270.0);
            ui.heading("👋 Velkommen tilbage");
            ui.add_space(4.0);
            ui.label(format!(
                "Du har været væk fra computeren fra {} til {} ({length}).",
                from.format("%H:%M"),
                to.format("%H:%M")
            ));
            ui.label(format!("Imens kørte tiden på {}.", self.data.project_label(project_id)));
            ui.add_space(4.0);
            ui.label(RichText::new("Skal tiden registreres som pause?").strong());
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("☕ Ja, som pause").on_hover_text("Enter").clicked() {
                    answer = Some(true);
                }
                if ui.button("Nej, behold på projektet").on_hover_text("Esc").clicked() {
                    answer = Some(false);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                answer = Some(true);
            }
        });
        if answer.is_none() && modal.should_close() {
            answer = Some(false);
        }

        if let Some(as_pause) = answer {
            if as_pause && self.data.convert_to_pause(project_id, from, to) {
                self.save();
            }
            self.away_handled_until = self.away_handled_until.max(to);
            self.away_prompt = None;
        }
    }
}

/// To tekstfelter til ny/eksisterende kunde og projekt. At skrive i dem fravælger
/// et valgt eksisterende projekt.
fn typed_target_fields(ui: &mut egui::Ui, target: &mut CallTarget) {
    let mut changed = false;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0 - 4.0;
        changed |= ui
            .add(egui::TextEdit::singleline(&mut target.customer).hint_text("Kunde").desired_width(width))
            .changed();
        changed |= ui
            .add(egui::TextEdit::singleline(&mut target.project).hint_text("Projekt").desired_width(width))
            .changed();
    });
    if changed {
        target.project_id = None;
    }
}

fn search_field(ui: &mut egui::Ui, query: &mut String) -> egui::Response {
    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(query)
                .hint_text("🔍 Søg kunde eller projekt…")
                .desired_width(ui.available_width() - 30.0),
        );
        if ui.add_enabled(!query.is_empty(), egui::Button::new("✕")).clicked() {
            query.clear();
        }
        response
    })
    .inner
}

/// Kundens projekter der matcher søgningen (uden hensyn til store/små bogstaver).
/// Matcher kundens navn, kommer alle projekterne med. `None` betyder at kunden
/// skal skjules.
fn matching_projects<'a>(data: &'a Data, customer: &Customer, query: &str) -> Option<Vec<&'a Project>> {
    let query = query.trim().to_lowercase();
    let projects = data.visible_projects().filter(|p| p.customer_id == customer.id);
    if query.is_empty() || customer.name.to_lowercase().contains(&query) {
        return Some(projects.collect());
    }
    let hits: Vec<_> = projects.filter(|p| p.name.to_lowercase().contains(&query)).collect();
    (!hits.is_empty()).then_some(hits)
}

/// Forkorter en tekst til højst `max` tegn med "…" til sidst.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

/// Dagens forskellige noter på et projekt, i den rækkefølge de blev skrevet.
fn day_notes(entries: &[DayEntry], project_id: Id) -> Vec<String> {
    let mut notes: Vec<String> = Vec::new();
    for e in entries.iter().filter(|e| e.entry.project_id == project_id) {
        let note = e.entry.description.trim();
        if !note.is_empty() && !notes.iter().any(|n| n == note) {
            notes.push(note.to_owned());
        }
    }
    notes
}

fn monitor_rect(monitor: &winit::monitor::MonitorHandle) -> egui::Rect {
    let (pos, size) = (monitor.position(), monitor.size());
    egui::Rect::from_min_size(
        Pos2::new(pos.x as f32, pos.y as f32),
        Vec2::new(size.width as f32, size.height as f32),
    )
}

/// Om vinduets øverste del (hvor titellinjen er) kan ses og gribes på en skærm:
/// mindst 80 × 20 pixels af den skal ligge på samme skærm.
fn title_bar_visible(window: egui::Rect, monitors: &[egui::Rect]) -> bool {
    let top = egui::Rect::from_min_size(window.min, Vec2::new(window.width(), window.height().min(40.0)));
    monitors.iter().any(|m| {
        let overlap = m.intersect(top);
        overlap.width() >= 80.0_f32.min(window.width()) && overlap.height() >= 20.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> egui::Rect {
        egui::Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    #[test]
    fn title_bar_must_be_on_a_monitor() {
        // To skærme over hinanden, 2560 × 1440.
        let monitors = [rect(0.0, 0.0, 2560.0, 1440.0), rect(0.0, 1440.0, 2560.0, 1440.0)];
        let window = |x, y| rect(x, y, 340.0, 560.0);

        assert!(title_bar_visible(window(100.0, 100.0), &monitors));
        assert!(title_bar_visible(window(100.0, 2000.0), &monitors), "på den nederste skærm");
        assert!(title_bar_visible(window(2400.0, 100.0), &monitors), "delvist udenfor, men kan gribes");
        assert!(!title_bar_visible(window(3000.0, 100.0), &monitors), "til højre for alle skærme");
        assert!(!title_bar_visible(window(100.0, 2870.0), &monitors), "kun 10 px af toppen synlig");
        assert!(!title_bar_visible(window(100.0, -500.0), &monitors), "titellinjen over skærmen");

        // Kun den nederste skærm tilbage: et vindue fra den øverste er nu væk.
        assert!(!title_bar_visible(window(100.0, 100.0), &monitors[1..]));
    }
}
