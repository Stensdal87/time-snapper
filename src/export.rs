//! Eksport af registreringer til CSV og Excel.

use std::path::Path;

use chrono::{DateTime, Duration, Local, NaiveDate};
use rust_xlsxwriter::{Format, FormatBorder, Workbook, Worksheet, XlsxError};

use crate::data::{format_duration, Data};

/// Én registrering klar til eksport, med navne slået op.
pub struct ExportRow {
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub customer: String,
    pub project: String,
    /// "", "Telefon" eller "Pause".
    pub kind: String,
    pub description: String,
}

impl ExportRow {
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// Alle registreringer i et projekt lagt sammen.
pub struct SummaryRow {
    pub customer: String,
    pub project: String,
    pub count: usize,
    pub duration: Duration,
    /// Projektets forskellige noter i perioden.
    pub notes: Vec<String>,
}

/// Afsluttede registreringer der startede fra og med `from` til og med `to`,
/// sorteret efter starttid. Den kørende timer kommer ikke med.
pub fn collect(data: &Data, from: NaiveDate, to: NaiveDate) -> Vec<ExportRow> {
    let mut rows: Vec<ExportRow> = data
        .entries
        .iter()
        .filter(|e| (from..=to).contains(&e.start.date_naive()))
        .map(|e| {
            let project = data.project(e.project_id);
            let customer = project.and_then(|p| data.customer(p.customer_id));
            ExportRow {
                start: e.start,
                end: e.end,
                customer: customer.map_or("?", |c| c.name.as_str()).to_owned(),
                project: project.map_or("?", |p| p.name.as_str()).to_owned(),
                kind: e.note.clone().unwrap_or_default(),
                description: e.description.clone(),
            }
        })
        .collect();
    rows.sort_by_key(|r| r.start);
    rows
}

/// Én linje pr. kunde/projekt, sorteret alfabetisk.
pub fn summarize(rows: &[ExportRow]) -> Vec<SummaryRow> {
    let mut summary: Vec<SummaryRow> = Vec::new();
    for row in rows {
        let i = match summary.iter().position(|s| s.customer == row.customer && s.project == row.project) {
            Some(i) => i,
            None => {
                summary.push(SummaryRow {
                    customer: row.customer.clone(),
                    project: row.project.clone(),
                    count: 0,
                    duration: Duration::zero(),
                    notes: Vec::new(),
                });
                summary.len() - 1
            }
        };
        let s = &mut summary[i];
        s.count += 1;
        s.duration += row.duration();
        let note = row.description.trim();
        if !note.is_empty() && !s.notes.iter().any(|n| n == note) {
            s.notes.push(note.to_owned());
        }
    }
    summary.sort_by_key(|s| (s.customer.to_lowercase(), s.project.to_lowercase()));
    summary
}

#[derive(Clone, Copy, PartialEq)]
pub enum CsvContent {
    Entries,
    Summary,
}

/// Kolonnenavne og "I alt" på eksportens sprog (`lang` er "da" eller "en").
fn heading(key: &str, lang: &str) -> String {
    rust_i18n::t!(key, locale = lang).to_string()
}

fn entry_headers(lang: &str) -> Vec<String> {
    ["date", "start", "end", "customer", "project", "type", "note", "duration", "hours"]
        .iter()
        .map(|k| heading(&format!("export.col_{k}"), lang))
        .collect()
}

fn summary_headers(lang: &str) -> Vec<String> {
    ["customer", "project", "count", "duration", "hours", "notes"]
        .iter()
        .map(|k| heading(&format!("export.col_{k}"), lang))
        .collect()
}

/// Typen ("Telefon"/"Pause") på eksportens sprog.
fn kind_label(kind: &str, lang: &str) -> String {
    match kind {
        crate::data::NOTE_PHONE => heading("note.phone", lang),
        crate::data::NOTE_PAUSE => heading("note.pause", lang),
        other => other.to_owned(),
    }
}

/// CSV som Excel læser det på eksportens sprog: på dansk semikolon,
/// decimalkomma og dd-mm-åååå; på engelsk komma, decimalpunktum og åååå-mm-dd.
/// UTF-8 med BOM, så Excel viser æøå korrekt.
pub fn to_csv(rows: &[ExportRow], content: CsvContent, lang: &str) -> String {
    let danish = lang == "da";
    let (separator, date_format) = if danish { (';', "%d-%m-%Y") } else { (',', "%Y-%m-%d") };
    let hours_text = |d: Duration| {
        let text = format!("{:.2}", hours(d));
        if danish { text.replace('.', ",") } else { text }
    };
    let mut lines: Vec<Vec<String>> = Vec::new();
    match content {
        CsvContent::Entries => {
            lines.push(entry_headers(lang));
            for r in rows {
                lines.push(vec![
                    r.start.format(date_format).to_string(),
                    r.start.format("%H:%M").to_string(),
                    r.end.format("%H:%M").to_string(),
                    r.customer.clone(),
                    r.project.clone(),
                    kind_label(&r.kind, lang),
                    r.description.clone(),
                    format_duration(r.duration()),
                    hours_text(r.duration()),
                ]);
            }
        }
        CsvContent::Summary => {
            lines.push(summary_headers(lang));
            for s in summarize(rows) {
                lines.push(vec![
                    s.customer,
                    s.project,
                    s.count.to_string(),
                    format_duration(s.duration),
                    hours_text(s.duration),
                    s.notes.join("; "),
                ]);
            }
        }
    }
    let body: Vec<String> = lines
        .iter()
        .map(|fields| fields.iter().map(|f| csv_field(f, separator)).collect::<Vec<_>>().join(&separator.to_string()))
        .collect();
    format!("\u{feff}{}\r\n", body.join("\r\n"))
}

fn csv_field(text: &str, separator: char) -> String {
    if text.contains([separator, '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

fn hours(d: Duration) -> f64 {
    d.num_seconds() as f64 / 3600.0
}

pub fn write_csv(path: &Path, rows: &[ExportRow], content: CsvContent, lang: &str) -> Result<(), String> {
    std::fs::write(path, to_csv(rows, content, lang)).map_err(|e| tr!("export.err_save", error = e))
}

/// Excel-fil med to faner: alle registreringer og summeret pr. projekt,
/// begge med totaler nederst.
pub fn write_xlsx(path: &Path, rows: &[ExportRow], lang: &str) -> Result<(), String> {
    build_xlsx(path, rows, lang).map_err(|e| tr!("export.err_save_excel", error = e))
}

fn build_xlsx(path: &Path, rows: &[ExportRow], lang: &str) -> Result<(), XlsxError> {
    let bold = Format::new().set_bold();
    let header = Format::new().set_bold().set_border_bottom(FormatBorder::Thin);
    let date = Format::new().set_num_format(if lang == "da" { "dd-mm-yyyy" } else { "yyyy-mm-dd" });
    let time = Format::new().set_num_format("hh:mm");
    // Varighed som brøkdel af et døgn, vist som timer:minutter:sekunder.
    let duration = Format::new().set_num_format("[h]:mm:ss");
    let decimal = Format::new().set_num_format("0.00");
    let total_duration = duration.clone().set_bold().set_border_top(FormatBorder::Thin);
    let total_decimal = decimal.clone().set_bold().set_border_top(FormatBorder::Thin);
    let total_label = bold.clone().set_border_top(FormatBorder::Thin);

    let mut workbook = Workbook::new();

    let sheet = workbook.add_worksheet();
    sheet.set_name(heading("export.sheet_entries", lang))?;
    write_header(sheet, &entry_headers(lang), &header)?;
    for (i, r) in rows.iter().enumerate() {
        let row = i as u32 + 1;
        sheet.write_datetime_with_format(row, 0, r.start.date_naive(), &date)?;
        sheet.write_datetime_with_format(row, 1, r.start.time(), &time)?;
        sheet.write_datetime_with_format(row, 2, r.end.time(), &time)?;
        sheet.write_string(row, 3, &r.customer)?;
        sheet.write_string(row, 4, &r.project)?;
        sheet.write_string(row, 5, kind_label(&r.kind, lang))?;
        sheet.write_string(row, 6, &r.description)?;
        sheet.write_number_with_format(row, 7, days(r.duration()), &duration)?;
        sheet.write_number_with_format(row, 8, hours(r.duration()), &decimal)?;
    }
    let last = rows.len() as u32;
    let total_row = last + 1;
    sheet.write_string_with_format(total_row, 0, heading("export.total", lang), &total_label)?;
    for col in 1..7 {
        sheet.write_blank(total_row, col, &total_label)?;
    }
    sheet.write_formula_with_format(total_row, 7, format!("=SUM(H2:H{})", last + 1).as_str(), &total_duration)?;
    sheet.write_formula_with_format(total_row, 8, format!("=SUM(I2:I{})", last + 1).as_str(), &total_decimal)?;
    for (col, width) in [11, 7, 7, 22, 22, 9, 35, 10, 8].into_iter().enumerate() {
        sheet.set_column_width(col as u16, width)?;
    }
    sheet.set_freeze_panes(1, 0)?;
    if !rows.is_empty() {
        sheet.autofilter(0, 0, last, 8)?;
    }

    let summary = summarize(rows);
    let sheet = workbook.add_worksheet();
    sheet.set_name(heading("export.sheet_summary", lang))?;
    write_header(sheet, &summary_headers(lang), &header)?;
    for (i, s) in summary.iter().enumerate() {
        let row = i as u32 + 1;
        sheet.write_string(row, 0, &s.customer)?;
        sheet.write_string(row, 1, &s.project)?;
        sheet.write_number(row, 2, s.count as f64)?;
        sheet.write_number_with_format(row, 3, days(s.duration), &duration)?;
        sheet.write_number_with_format(row, 4, hours(s.duration), &decimal)?;
        sheet.write_string(row, 5, s.notes.join("; "))?;
    }
    let last = summary.len() as u32;
    let total_row = last + 1;
    sheet.write_string_with_format(total_row, 0, heading("export.total", lang), &total_label)?;
    sheet.write_blank(total_row, 1, &total_label)?;
    sheet.write_formula_with_format(total_row, 2, format!("=SUM(C2:C{})", last + 1).as_str(), &total_label)?;
    sheet.write_formula_with_format(total_row, 3, format!("=SUM(D2:D{})", last + 1).as_str(), &total_duration)?;
    sheet.write_formula_with_format(total_row, 4, format!("=SUM(E2:E{})", last + 1).as_str(), &total_decimal)?;
    sheet.write_blank(total_row, 5, &total_label)?;
    for (col, width) in [22, 22, 7, 10, 8, 50].into_iter().enumerate() {
        sheet.set_column_width(col as u16, width)?;
    }
    sheet.set_freeze_panes(1, 0)?;

    workbook.save(path)
}

fn write_header(sheet: &mut Worksheet, names: &[String], format: &Format) -> Result<(), XlsxError> {
    for (col, name) in names.iter().enumerate() {
        sheet.write_string_with_format(0, col as u16, name, format)?;
    }
    Ok(())
}

/// Excel gemmer varigheder som brøkdel af et døgn.
fn days(d: Duration) -> f64 {
    d.num_seconds() as f64 / 86_400.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::TimeEntry;
    use chrono::TimeZone;

    fn sample() -> Data {
        let mut data = Data::default();
        let c = data.add_customer("Kunde; \"A\"");
        let web = data.add_project(c, "Web");
        let app = data.add_project(c, "App");
        let at = |d: u32, h: u32, m: u32| Local.with_ymd_and_hms(2026, 10, d, h, m, 0).unwrap();
        for (project_id, start, end, note, description) in [
            (web, at(19, 8, 0), at(19, 9, 0), None, "Forside"),
            (app, at(20, 8, 0), at(20, 8, 45), Some("Telefon"), ""),
            (web, at(20, 10, 0), at(20, 10, 30), None, "Kontaktformular"),
            (web, at(21, 8, 0), at(21, 9, 0), None, "Forside"),
        ] {
            data.add_entry(TimeEntry {
                project_id,
                start,
                end,
                note: note.map(str::to_owned),
                description: description.to_owned(),
            });
        }
        data
    }

    fn day(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, d).unwrap()
    }

    #[test]
    fn collects_whole_days_inclusive() {
        let data = sample();
        assert_eq!(collect(&data, day(20), day(20)).len(), 2);
        assert_eq!(collect(&data, day(19), day(21)).len(), 4);
        assert_eq!(collect(&data, day(22), day(31)).len(), 0);
    }

    #[test]
    fn summary_merges_per_project() {
        let rows = collect(&sample(), day(19), day(21));
        let summary = summarize(&rows);
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].project, "App");
        assert_eq!(summary[1].project, "Web");
        assert_eq!(summary[1].count, 3);
        assert_eq!(summary[1].duration, Duration::minutes(150));
        assert_eq!(summary[1].notes, ["Forside", "Kontaktformular"], "hver note kun én gang");
    }

    #[test]
    fn csv_uses_danish_format_and_quotes_fields() {
        let rows = collect(&sample(), day(20), day(20));
        let csv = to_csv(&rows, CsvContent::Entries, "da");
        assert!(csv.starts_with('\u{feff}'));
        let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
        assert_eq!(lines[0], "Dato;Start;Slut;Kunde;Projekt;Type;Note;Varighed;Timer");
        assert_eq!(lines[1], "20-10-2026;08:00;08:45;\"Kunde; \"\"A\"\"\";App;Telefon;;0:45:00;0,75");
        assert_eq!(lines[2], "20-10-2026;10:00;10:30;\"Kunde; \"\"A\"\"\";Web;;Kontaktformular;0:30:00;0,50");

        let summary = to_csv(&rows, CsvContent::Summary, "da");
        assert!(summary.contains(";Web;1;0:30:00;0,50;Kontaktformular"));
    }

    #[test]
    fn csv_in_english_uses_commas_points_and_iso_dates() {
        let rows = collect(&sample(), day(20), day(20));
        let csv = to_csv(&rows, CsvContent::Entries, "en");
        let lines: Vec<&str> = csv.trim_start_matches('\u{feff}').lines().collect();
        assert_eq!(lines[0], "Date,Start,End,Customer,Project,Type,Note,Duration,Hours");
        assert_eq!(lines[1], "2026-10-20,08:00,08:45,\"Kunde; \"\"A\"\"\",App,Phone,,0:45:00,0.75");
    }

    #[test]
    fn writes_xlsx_file() {
        let path = std::env::temp_dir().join(format!("ts-export-{}.xlsx", std::process::id()));
        write_xlsx(&path, &collect(&sample(), day(19), day(21)), "da").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert!(bytes.starts_with(b"PK"), "xlsx er en zip-fil");
    }
}
