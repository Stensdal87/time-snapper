//! Dialog til at rette, slette eller tilføje en registrering.

use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone};
use eframe::egui::{self, Color32, RichText};

use crate::data::{format_duration, Data, Id, TimeEntry, NOTE_PAUSE, NOTE_PHONE};

pub enum EditorAction {
    None,
    Cancel,
    /// Gem registreringen: `Some(index)` retter en eksisterende, `None` tilføjer en ny.
    Save(Option<usize>, TimeEntry),
    Delete(usize),
}

pub struct EntryEditor {
    /// `None` er en ny registrering.
    index: Option<usize>,
    /// Den oprindelige registrering. Uændrede felter beholder deres sekunder.
    original: Option<TimeEntry>,
    project_id: Option<Id>,
    note: Option<String>,
    date: String,
    start: String,
    end: String,
    confirm_delete: bool,
}

impl EntryEditor {
    pub fn edit(index: usize, entry: &TimeEntry) -> Self {
        Self {
            index: Some(index),
            original: Some(entry.clone()),
            project_id: Some(entry.project_id),
            note: entry.note.clone(),
            date: entry.start.format("%d-%m-%Y").to_string(),
            start: entry.start.format("%H:%M").to_string(),
            end: entry.end.format("%H:%M").to_string(),
            confirm_delete: false,
        }
    }

    /// Åbner direkte i "er du sikker"-trinnet.
    pub fn delete(index: usize, entry: &TimeEntry) -> Self {
        Self { confirm_delete: true, ..Self::edit(index, entry) }
    }

    /// Ny registrering på en dag. Starttiden foreslås som slutningen af dagens
    /// seneste registrering.
    pub fn new(date: NaiveDate, suggested_start: Option<DateTime<Local>>) -> Self {
        Self {
            index: None,
            original: None,
            project_id: None,
            note: None,
            date: date.format("%d-%m-%Y").to_string(),
            start: suggested_start.map(|t| t.format("%H:%M").to_string()).unwrap_or_default(),
            end: String::new(),
            confirm_delete: false,
        }
    }

    /// Registreringen som felterne beskriver den, eller hvad der er galt.
    fn build(&self) -> Result<TimeEntry, String> {
        let project_id = self.project_id.ok_or("Vælg et projekt")?;
        let date = parse_date(&self.date).ok_or("Ugyldig dato (brug dd-mm-åååå)")?;
        let original = self.original.as_ref();
        let unchanged_date = original.is_some_and(|o| o.start.date_naive() == date);

        let start = match original {
            Some(o) if unchanged_date && self.start == o.start.format("%H:%M").to_string() => o.start,
            _ => to_local(date, parse_time(&self.start).ok_or("Ugyldig fra-tid (brug tt:mm)")?)?,
        };
        let end = match original {
            Some(o) if unchanged_date && self.end == o.end.format("%H:%M").to_string() => o.end,
            _ => to_local(date, parse_time(&self.end).ok_or("Ugyldig til-tid (brug tt:mm)")?)?,
        };
        if end <= start {
            return Err("Til-tiden skal være efter fra-tiden".to_owned());
        }
        Ok(TimeEntry { project_id, start, end, note: self.note.clone() })
    }

    pub fn show(&mut self, ctx: &egui::Context, data: &Data) -> EditorAction {
        let built = self.build();
        let mut action = EditorAction::None;

        let modal = egui::Modal::new(egui::Id::unique("entry_editor")).show(ctx, |ui| {
            ui.set_width(280.0);
            ui.heading(if self.index.is_some() { "✏ Ret registrering" } else { "➕ Ny registrering" });
            ui.add_space(4.0);

            egui::Grid::new("entry_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Projekt");
                self.project_combo(ui, data);
                ui.end_row();

                ui.label("Type");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.note, None, "Normal");
                    ui.selectable_value(&mut self.note, Some(NOTE_PHONE.to_owned()), "📞 Telefon");
                    ui.selectable_value(&mut self.note, Some(NOTE_PAUSE.to_owned()), "☕ Pause");
                });
                ui.end_row();

                ui.label("Dato");
                ui.add(egui::TextEdit::singleline(&mut self.date).hint_text("dd-mm-åååå").desired_width(100.0));
                ui.end_row();

                ui.label("Tid");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.start).hint_text("fra").desired_width(50.0));
                    ui.label("–");
                    ui.add(egui::TextEdit::singleline(&mut self.end).hint_text("til").desired_width(50.0));
                });
                ui.end_row();
            });

            ui.add_space(4.0);
            match &built {
                Ok(entry) => {
                    ui.label(format!("Varighed: {}", format_duration(entry.duration())));
                    let overlaps = data.overlapping(entry, self.index);
                    if !overlaps.is_empty() {
                        ui.colored_label(ui.visuals().warn_fg_color, "⚠ Overlapper med:");
                        for o in overlaps {
                            ui.label(
                                RichText::new(format!(
                                    "   {}–{}  {}",
                                    o.start.format("%H:%M"),
                                    o.end.format("%H:%M"),
                                    data.project_label(o.project_id)
                                ))
                                .weak(),
                            );
                        }
                    }
                }
                Err(e) => {
                    ui.colored_label(ui.visuals().error_fg_color, e);
                }
            }
            ui.add_space(8.0);

            if self.confirm_delete {
                ui.label(RichText::new("Er du sikker på, at du vil slette registreringen?").strong());
                ui.horizontal(|ui| {
                    let yes = egui::Button::new(RichText::new("🗑 Ja, slet").color(Color32::WHITE))
                        .fill(Color32::from_rgb(200, 60, 60));
                    if ui.add(yes).clicked()
                        && let Some(index) = self.index
                    {
                        action = EditorAction::Delete(index);
                    }
                    if ui.button("Nej").clicked() {
                        self.confirm_delete = false;
                    }
                });
                return;
            }

            ui.horizontal(|ui| {
                if ui.add_enabled(built.is_ok(), egui::Button::new("✔ Gem")).on_hover_text("Enter").clicked() {
                    action = save(self.index, &built);
                }
                if ui.button("Annuller").on_hover_text("Esc").clicked() {
                    action = EditorAction::Cancel;
                }
                if self.index.is_some() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("🗑 Slet").clicked() {
                            self.confirm_delete = true;
                        }
                    });
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Enter)) && built.is_ok() {
                action = save(self.index, &built);
            }
        });

        if matches!(action, EditorAction::None) && modal.should_close() {
            action = EditorAction::Cancel;
        }
        action
    }

    fn project_combo(&mut self, ui: &mut egui::Ui, data: &Data) {
        let selected = self.project_id.map_or_else(|| "Vælg projekt…".to_owned(), |id| data.project_label(id));
        // Et slettet projekt kan stadig vælges for registreringer, der allerede ligger på det.
        let current_deleted = self.project_id.filter(|id| data.project(*id).is_some_and(|p| p.deleted));
        let mut ids: Vec<Id> = data.visible_projects().map(|p| p.id).collect();
        ids.extend(current_deleted);
        ids.sort_by_key(|id| (!data.is_internal(*id), data.project_label(*id).to_lowercase()));

        egui::ComboBox::from_id_salt("entry_project")
            .width(200.0)
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for id in ids {
                    ui.selectable_value(&mut self.project_id, Some(id), data.project_label(id));
                }
            });
    }
}

fn save(index: Option<usize>, built: &Result<TimeEntry, String>) -> EditorAction {
    match built {
        Ok(entry) => EditorAction::Save(index, entry.clone()),
        Err(_) => EditorAction::None,
    }
}

fn to_local(date: NaiveDate, time: NaiveTime) -> Result<DateTime<Local>, String> {
    // Ved skift til/fra sommertid kan et klokkeslæt være tvetydigt; tag det tidligste.
    Local
        .from_local_datetime(&date.and_time(time))
        .earliest()
        .ok_or_else(|| "Klokkeslættet findes ikke (sommertid)".to_owned())
}

/// Accepterer "8:30", "08:30", "8.30", "0830", "830" og "8".
fn parse_time(text: &str) -> Option<NaiveTime> {
    let text = text.trim().replace(['.', ','], ":");
    if let Some((h, m)) = text.split_once(':') {
        return NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0);
    }
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (h, m) = match text.len() {
        1 | 2 => (text.as_str(), "0"),
        3 | 4 => text.split_at(text.len() - 2),
        _ => return None,
    };
    NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0)
}

/// Accepterer dd-mm-åååå, dd.mm.åååå, dd/mm/åååå og åååå-mm-dd.
pub fn parse_date(text: &str) -> Option<NaiveDate> {
    let text = text.trim();
    ["%d-%m-%Y", "%d.%m.%Y", "%d/%m/%Y", "%Y-%m-%d"]
        .iter()
        .find_map(|f| NaiveDate::parse_from_str(text, f).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_time_formats() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0);
        assert_eq!(parse_time("8:30"), t(8, 30));
        assert_eq!(parse_time("08:30"), t(8, 30));
        assert_eq!(parse_time("8.30"), t(8, 30));
        assert_eq!(parse_time("0830"), t(8, 30));
        assert_eq!(parse_time("830"), t(8, 30));
        assert_eq!(parse_time("14"), t(14, 0));
        assert_eq!(parse_time(" 23:59 "), t(23, 59));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("8:60"), None);
        assert_eq!(parse_time("abc"), None);
        assert_eq!(parse_time(""), None);
    }

    #[test]
    fn parses_common_date_formats() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 20);
        assert_eq!(parse_date("20-10-2026"), d);
        assert_eq!(parse_date("20.10.2026"), d);
        assert_eq!(parse_date("2026-10-20"), d);
        assert_eq!(parse_date("32-10-2026"), None);
    }

    #[test]
    fn unchanged_times_keep_their_seconds() {
        let start = Local.with_ymd_and_hms(2026, 10, 20, 8, 0, 17).unwrap();
        let end = Local.with_ymd_and_hms(2026, 10, 20, 9, 30, 45).unwrap();
        let entry = TimeEntry { project_id: 1, start, end, note: None };

        let mut editor = EntryEditor::edit(0, &entry);
        assert_eq!(editor.build().unwrap().duration(), end - start);

        editor.end = "10:00".to_owned();
        let changed = editor.build().unwrap();
        assert_eq!(changed.start, start, "fra-tiden er ikke rørt");
        assert_eq!(changed.end, Local.with_ymd_and_hms(2026, 10, 20, 10, 0, 0).unwrap());

        editor.end = "7:00".to_owned();
        assert!(editor.build().is_err(), "til før fra");
    }
}
