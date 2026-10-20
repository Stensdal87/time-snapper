//! Datamodel og lagring. Alt gemmes i én JSON-fil i brugerens datamappe.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Local, NaiveDate};
use serde::{Deserialize, Serialize};

pub type Id = u64;

pub const NOTE_PHONE: &str = "Telefon";
pub const NOTE_PAUSE: &str = "Pause";

#[derive(Serialize, Deserialize, Clone)]
pub struct Customer {
    pub id: Id,
    pub name: String,
    /// Slettede kunder skjules, men beholdes så historikken stadig kan vise navnet.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Project {
    pub id: Id,
    pub customer_id: Id,
    pub name: String,
    /// Slettede projekter skjules, men beholdes så historikken stadig kan vise navnet.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

/// En afsluttet tidsregistrering.
#[derive(Serialize, Deserialize, Clone)]
pub struct TimeEntry {
    pub project_id: Id,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    /// F.eks. "Telefon" eller "Pause".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl TimeEntry {
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// Hvor et telefonopkald skal registreres: enten et eksisterende projekt eller
/// en kunde og et projekt skrevet som tekst (oprettes hvis de ikke findes).
#[derive(Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct CallTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Id>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub customer: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub project: String,
}

impl CallTarget {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn typed(&self) -> Option<(&str, &str)> {
        let (customer, project) = (self.customer.trim(), self.project.trim());
        (!customer.is_empty() && !project.is_empty()).then_some((customer, project))
    }
}

/// Et telefonopkald der er afsluttet, men endnu ikke registreret på et projekt.
#[derive(Serialize, Deserialize, Clone)]
pub struct UnassignedEntry {
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    #[serde(default, skip_serializing_if = "CallTarget::is_empty")]
    pub target: CallTarget,
}

impl UnassignedEntry {
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// Den timer der kører lige nu. `last_seen` opdateres løbende, så en timer der
/// efterlades af et nedbrud kan afsluttes på et fornuftigt tidspunkt.
#[derive(Serialize, Deserialize, Clone)]
pub struct ActiveTimer {
    /// `None` er et telefonopkald, der endnu ikke hører til et projekt.
    pub project_id: Option<Id>,
    pub start: DateTime<Local>,
    pub last_seen: DateTime<Local>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Kan udfyldes mens et opkald kører, så det registreres direkte når det slutter.
    #[serde(default, skip_serializing_if = "CallTarget::is_empty")]
    pub target: CallTarget,
}

impl ActiveTimer {
    pub fn is_note(&self, note: &str) -> bool {
        self.note.as_deref() == Some(note)
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub fn next(self) -> Self {
        match self {
            Self::System => Self::Light,
            Self::Light => Self::Dark,
            Self::Dark => Self::System,
        }
    }

    pub fn to_egui(self) -> eframe::egui::ThemePreference {
        use eframe::egui::ThemePreference;
        match self {
            Self::System => ThemePreference::System,
            Self::Light => ThemePreference::Light,
            Self::Dark => ThemePreference::Dark,
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Data {
    next_id: Id,
    pub customers: Vec<Customer>,
    pub projects: Vec<Project>,
    pub entries: Vec<TimeEntry>,
    pub active: Option<ActiveTimer>,
    #[serde(default)]
    pub unassigned: Vec<UnassignedEntry>,
    /// Projektet "Intern tid", som pauser og intern tid registreres på.
    #[serde(default)]
    internal_project: Option<Id>,
    #[serde(default)]
    pub theme: ThemeChoice,
    /// Vinduets indre placering (skærmpunkter) da det sidst blev lukket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_pos: Option<[f32; 2]>,
}

impl Data {
    /// Standardplacering: ~/.local/share/timesnapper/data.json på Linux,
    /// %APPDATA%\TimeSnapper\data\data.json på Windows.
    pub fn default_path() -> PathBuf {
        Self::path_for("TimeSnapper")
    }

    /// Datafilens placering for et givet programnavn.
    pub fn path_for(app_name: &str) -> PathBuf {
        directories::ProjectDirs::from("", "", app_name)
            .map(|d| d.data_dir().join("data.json"))
            .unwrap_or_else(|| PathBuf::from(format!("{}-data.json", app_name.to_lowercase())))
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let mut data: Data = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("Kunne ikke læse {}: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Data::default(),
            Err(e) => return Err(format!("Kunne ikke åbne {}: {e}", path.display())),
        };
        // En timer der stadig "kører" i filen betyder at programmet ikke blev
        // lukket pænt. Afslut den ved sidste livstegn i stedet for at tælle videre.
        if let Some(last_seen) = data.active.as_ref().map(|a| a.last_seen) {
            data.stop_at(last_seen);
        }
        data.internal_project_id();
        Ok(data)
    }

    /// Skriver først til en midlertidig fil og omdøber bagefter, så datafilen
    /// ikke bliver ødelagt hvis programmet stoppes midt i en skrivning.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Kunne ikke oprette mappe: {e}"))?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("Kunne ikke gemme: {e}"))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("Kunne ikke gemme: {e}"))
    }

    fn new_id(&mut self) -> Id {
        self.next_id += 1;
        self.next_id
    }

    pub fn add_customer(&mut self, name: &str) -> Id {
        let id = self.new_id();
        self.customers.push(Customer { id, name: name.to_owned(), deleted: false });
        id
    }

    pub fn add_project(&mut self, customer_id: Id, name: &str) -> Id {
        let id = self.new_id();
        self.projects.push(Project { id, customer_id, name: name.to_owned(), deleted: false });
        id
    }

    /// Id på projektet "Intern tid". Opretter kunden "Intern" og projektet, hvis de mangler.
    pub fn internal_project_id(&mut self) -> Id {
        if let Some(id) = self.internal_project
            && self.project(id).is_some()
        {
            return id;
        }
        let customer = self.add_customer("Intern");
        let id = self.add_project(customer, "Intern tid");
        self.internal_project = Some(id);
        id
    }

    /// Kunder der ikke er slettet.
    pub fn visible_customers(&self) -> impl Iterator<Item = &Customer> {
        self.customers.iter().filter(|c| !c.deleted)
    }

    /// Projekter der ikke er slettet.
    pub fn visible_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.iter().filter(|p| !p.deleted)
    }

    pub fn is_internal(&self, project_id: Id) -> bool {
        self.internal_project == Some(project_id)
    }

    pub fn is_internal_customer(&self, customer_id: Id) -> bool {
        self.internal_project
            .and_then(|id| self.project(id))
            .is_some_and(|p| p.customer_id == customer_id)
    }

    /// Antal registreringer og samlet tid på et projekt.
    pub fn project_stats(&self, project_id: Id) -> (usize, Duration) {
        let entries: Vec<_> = self.entries.iter().filter(|e| e.project_id == project_id).collect();
        (entries.len(), self.project_total(project_id))
    }

    /// Antal registreringer og samlet tid på alle en kundes projekter.
    pub fn customer_stats(&self, customer_id: Id) -> (usize, Duration) {
        self.projects
            .iter()
            .filter(|p| p.customer_id == customer_id)
            .map(|p| self.project_stats(p.id))
            .fold((0, Duration::zero()), |(n, d), (pn, pd)| (n + pn, d + pd))
    }

    /// Skjuler et projekt. Kører timeren på det, stoppes den først, så tiden gemmes.
    /// Registreringerne bevares i historikken. "Intern tid" kan ikke slettes.
    pub fn delete_project(&mut self, project_id: Id) {
        if self.is_internal(project_id) {
            return;
        }
        if self.active.as_ref().is_some_and(|a| a.project_id == Some(project_id)) {
            self.stop();
        }
        if let Some(p) = self.projects.iter_mut().find(|p| p.id == project_id) {
            p.deleted = true;
        }
    }

    /// Skjuler en kunde og alle dens projekter.
    pub fn delete_customer(&mut self, customer_id: Id) {
        if self.is_internal_customer(customer_id) {
            return;
        }
        let ids: Vec<Id> = self.projects.iter().filter(|p| p.customer_id == customer_id).map(|p| p.id).collect();
        for id in ids {
            self.delete_project(id);
        }
        if let Some(c) = self.customers.iter_mut().find(|c| c.id == customer_id) {
            c.deleted = true;
        }
    }

    pub fn project(&self, id: Id) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn customer(&self, id: Id) -> Option<&Customer> {
        self.customers.iter().find(|c| c.id == id)
    }

    /// "Kunde / Projekt" til visning.
    pub fn project_label(&self, project_id: Id) -> String {
        let Some(project) = self.project(project_id) else {
            return "(slettet projekt)".to_owned();
        };
        let customer = self.customer(project.customer_id).map_or("?", |c| c.name.as_str());
        format!("{customer} / {}", project.name)
    }

    /// Starter tid på et projekt. Kører der allerede en timer, gemmes den først.
    pub fn start(&mut self, project_id: Id) {
        self.start_timer(Some(project_id), None);
    }

    /// Starter et telefonopkald, som først skal registreres på et projekt bagefter.
    pub fn start_call(&mut self) {
        self.start_timer(None, Some(NOTE_PHONE));
    }

    /// Starter en pause, som registreres på "Intern tid".
    pub fn start_pause(&mut self) {
        let id = self.internal_project_id();
        self.start_timer(Some(id), Some(NOTE_PAUSE));
    }

    fn start_timer(&mut self, project_id: Option<Id>, note: Option<&str>) {
        let now = Local::now();
        self.stop_at(now);
        self.active = Some(ActiveTimer {
            project_id,
            start: now,
            last_seen: now,
            note: note.map(str::to_owned),
            target: CallTarget::default(),
        });
    }

    pub fn stop(&mut self) {
        self.stop_at(Local::now());
    }

    fn stop_at(&mut self, end: DateTime<Local>) {
        let Some(active) = self.active.take() else { return };
        // Spring fejlklik over (under ét sekund).
        if end - active.start < Duration::seconds(1) {
            return;
        }
        match active.project_id {
            Some(project_id) => self.entries.push(TimeEntry {
                project_id,
                start: active.start,
                end,
                note: active.note,
            }),
            None => {
                let call = UnassignedEntry { start: active.start, end, target: active.target };
                // Er projektet valgt under opkaldet, registreres det med det samme.
                if !self.register_call(&call) {
                    self.unassigned.push(call);
                }
            }
        }
    }

    pub fn target_is_complete(&self, target: &CallTarget) -> bool {
        target.project_id.is_some_and(|id| self.project(id).is_some_and(|p| !p.deleted)) || target.typed().is_some()
    }

    /// Projektet som et mål peger på. Indtastet kunde og projekt genbruges hvis
    /// de findes i forvejen (uanset store/små bogstaver), ellers oprettes de.
    fn resolve_target(&mut self, target: &CallTarget) -> Option<Id> {
        if let Some(id) = target.project_id
            && self.project(id).is_some_and(|p| !p.deleted)
        {
            return Some(id);
        }
        let (customer, project) = target.typed()?;
        let same = |a: &str, b: &str| a.to_lowercase() == b.to_lowercase();
        let existing = self.visible_customers().find(|c| same(&c.name, customer)).map(|c| c.id);
        let customer_id = match existing {
            Some(id) => id,
            None => self.add_customer(customer),
        };
        let existing = self
            .visible_projects()
            .find(|p| p.customer_id == customer_id && same(&p.name, project))
            .map(|p| p.id);
        Some(match existing {
            Some(id) => id,
            None => self.add_project(customer_id, project),
        })
    }

    /// Gemmer opkaldet som registrering, hvis dets mål er udfyldt.
    fn register_call(&mut self, call: &UnassignedEntry) -> bool {
        let Some(project_id) = self.resolve_target(&call.target) else {
            return false;
        };
        self.entries.push(TimeEntry {
            project_id,
            start: call.start,
            end: call.end,
            note: Some(NOTE_PHONE.to_owned()),
        });
        true
    }

    /// Registrerer det første ikke-registrerede opkald på det mål, der er udfyldt
    /// i det. Returnerer false hvis målet ikke er udfyldt.
    pub fn register_first_unassigned(&mut self) -> bool {
        let Some(call) = self.unassigned.first().cloned() else {
            return false;
        };
        let registered = self.register_call(&call);
        if registered {
            self.unassigned.remove(0);
        }
        registered
    }

    pub fn discard_first_unassigned(&mut self) {
        if !self.unassigned.is_empty() {
            self.unassigned.remove(0);
        }
    }

    /// Alle registreringer der startede på den givne dag, inkl. den aktive timer
    /// hvis den hører til et projekt.
    pub fn entries_on(&self, date: NaiveDate) -> Vec<TimeEntry> {
        let mut list: Vec<TimeEntry> = self
            .entries
            .iter()
            .filter(|e| e.start.date_naive() == date)
            .cloned()
            .collect();
        if let Some(a) = &self.active
            && let Some(project_id) = a.project_id
            && a.start.date_naive() == date
        {
            list.push(TimeEntry {
                project_id,
                start: a.start,
                end: Local::now(),
                note: a.note.clone(),
            });
        }
        list.sort_by_key(|e| e.start);
        list
    }

    /// Al tid brugt på et projekt nogensinde, inkl. den aktive timer.
    pub fn project_total(&self, project_id: Id) -> Duration {
        let finished = self
            .entries
            .iter()
            .filter(|e| e.project_id == project_id)
            .fold(Duration::zero(), |sum, e| sum + e.duration());
        let running = match &self.active {
            Some(a) if a.project_id == Some(project_id) => Local::now() - a.start,
            _ => Duration::zero(),
        };
        finished + running
    }

    /// Samlet tid pr. projekt på en dag, sorteret efter mest tid først.
    pub fn totals_on(&self, date: NaiveDate) -> Vec<(Id, Duration)> {
        let mut totals: Vec<(Id, Duration)> = Vec::new();
        for e in self.entries_on(date) {
            match totals.iter_mut().find(|(id, _)| *id == e.project_id) {
                Some((_, d)) => *d += e.duration(),
                None => totals.push((e.project_id, e.duration())),
            }
        }
        totals.sort_by_key(|(_, d)| std::cmp::Reverse(*d));
        totals
    }
}

pub fn format_duration(d: Duration) -> String {
    let secs = d.num_seconds().max(0);
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_since(project_id: Option<Id>, minutes: i64) -> Option<ActiveTimer> {
        Some(ActiveTimer {
            project_id,
            start: Local::now() - Duration::minutes(minutes),
            last_seen: Local::now(),
            note: None,
            target: CallTarget::default(),
        })
    }

    #[test]
    fn switching_project_saves_previous_entry() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p1 = data.add_project(c, "A");
        let p2 = data.add_project(c, "B");

        data.active = active_since(Some(p1), 5);
        data.start(p2);

        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].project_id, p1);
        assert_eq!(data.active.as_ref().unwrap().project_id, Some(p2));
    }

    #[test]
    fn call_becomes_unassigned_and_can_be_assigned_later() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");

        data.start_call();
        data.active.as_mut().unwrap().start -= Duration::minutes(10);
        data.start(p);

        assert!(data.entries.is_empty());
        assert_eq!(data.unassigned.len(), 1);
        assert_eq!(data.active.as_ref().unwrap().project_id, Some(p));

        assert!(!data.register_first_unassigned(), "intet mål valgt endnu");
        let internal = data.internal_project_id();
        data.unassigned[0].target.project_id = Some(internal);
        assert!(data.register_first_unassigned());
        assert!(data.unassigned.is_empty());
        assert_eq!(data.entries[0].project_id, internal);
        assert_eq!(data.entries[0].note.as_deref(), Some(NOTE_PHONE));
    }

    #[test]
    fn call_with_target_chosen_during_call_is_registered_directly() {
        let mut data = Data::default();
        let c = data.add_customer("Ærø Træ");
        let p = data.add_project(c, "Hjemmeside");

        data.start_call();
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(5);
        // Skrevet med andre store/små bogstaver: skal genbruge kunde og projekt.
        active.target.customer = "ærø træ".to_owned();
        active.target.project = " HJEMMESIDE ".to_owned();
        data.stop();

        assert!(data.unassigned.is_empty());
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].project_id, p);
        assert_eq!(data.customers.len(), 1);
        assert_eq!(data.projects.len(), 1);
    }

    #[test]
    fn typed_new_customer_and_project_are_created() {
        let mut data = Data::default();
        data.start_call();
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(5);
        active.target.customer = "Ny kunde".to_owned();
        active.target.project = "Support".to_owned();
        data.stop();

        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.project_label(data.entries[0].project_id), "Ny kunde / Support");
    }

    #[test]
    fn deleting_project_hides_it_but_keeps_history() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.active = active_since(Some(p), 20);

        data.delete_project(p);

        assert!(data.active.is_none(), "timeren på projektet stoppes");
        assert_eq!(data.entries.len(), 1, "tiden er gemt");
        assert_eq!(data.visible_projects().count(), 0);
        assert_eq!(data.project_label(p), "Kunde / A", "historikken kan stadig vise navnet");
    }

    #[test]
    fn deleting_customer_hides_its_projects_and_typed_name_creates_new() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.delete_customer(c);

        assert_eq!(data.visible_customers().count(), 0);
        assert!(data.project(p).unwrap().deleted);

        let target = CallTarget { customer: "Kunde".into(), project: "A".into(), ..Default::default() };
        let new_id = data.resolve_target(&target).unwrap();
        assert_ne!(new_id, p);
    }

    #[test]
    fn internal_project_cannot_be_deleted() {
        let mut data = Data::default();
        let internal = data.internal_project_id();
        let customer = data.project(internal).unwrap().customer_id;
        data.delete_customer(customer);
        data.delete_project(internal);
        assert!(!data.project(internal).unwrap().deleted);
        assert!(!data.customer(customer).unwrap().deleted);
    }

    #[test]
    fn pause_is_registered_on_internal_project() {
        let mut data = Data::default();
        data.start_pause();
        data.active.as_mut().unwrap().start -= Duration::minutes(15);
        data.stop();

        let internal = data.internal_project_id();
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries[0].project_id, internal);
        assert_eq!(data.entries[0].note.as_deref(), Some(NOTE_PAUSE));
        assert_eq!(data.project_label(internal), "Intern / Intern tid");
    }

    #[test]
    fn leftover_active_timer_is_closed_at_last_seen_on_load() {
        let dir = std::env::temp_dir().join(format!("ts-test-{}", std::process::id()));
        let path = dir.join("data.json");
        let start = Local::now() - Duration::hours(10);
        let last_seen = start + Duration::minutes(30);

        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.active = Some(ActiveTimer { project_id: Some(p), start, last_seen, note: None, target: CallTarget::default() });
        data.save(&path).unwrap();

        let loaded = Data::load(&path).unwrap();
        std::fs::remove_dir_all(&dir).ok();

        assert!(loaded.active.is_none());
        assert_eq!(loaded.entries.len(), 1);
        assert_eq!(loaded.entries[0].duration(), Duration::minutes(30));
    }

    #[test]
    fn old_data_file_without_new_fields_still_loads() {
        let json = r#"{"next_id":2,"customers":[{"id":1,"name":"K"}],
            "projects":[{"id":2,"customer_id":1,"name":"P"}],
            "entries":[{"project_id":2,"start":"2026-10-20T10:00:00+02:00","end":"2026-10-20T11:00:00+02:00"}],
            "active":null}"#;
        let data: Data = serde_json::from_str(json).unwrap();
        assert_eq!(data.entries.len(), 1);
        assert!(data.unassigned.is_empty());
    }
}
