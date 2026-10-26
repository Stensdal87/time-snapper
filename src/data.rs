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
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct TimeEntry {
    pub project_id: Id,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    /// Typen, f.eks. "Telefon" eller "Pause".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Brugerens egen note om hvad tiden er brugt på.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl TimeEntry {
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// En registrering på en bestemt dag. `index` peger ind i `Data.entries`;
/// `None` er den kørende timer, som ikke kan rettes.
pub struct DayEntry {
    pub index: Option<usize>,
    pub entry: TimeEntry,
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
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

impl UnassignedEntry {
    pub fn duration(&self) -> Duration {
        self.end - self.start
    }
}

/// Den timer der kører lige nu. `last_seen` opdateres løbende, så en timer der
/// efterlades af et nedbrud kan afsluttes på et fornuftigt tidspunkt.
#[derive(Serialize, Deserialize, Clone, Default)]
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
    /// Under en pause eller et opkald: tidstagningen der kørte før, som der
    /// fortsættes på, når pausen/opkaldet afsluttes med samme knap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<Box<ActiveTimer>>,
    /// Tid fra før en pause, som uret tæller videre fra. Kun til visning;
    /// registreringerne gemmes stadig hver for sig.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub carried_secs: i64,
    /// Note der skrives mens tiden kører; følger med registreringen.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Kopi af en kørende tidstagning med uret frosset (`carried_secs` = vist tid),
/// så den kan genoptages efter en pause eller et opkald.
fn frozen(active: &ActiveTimer) -> Box<ActiveTimer> {
    Box::new(ActiveTimer { carried_secs: active.elapsed().num_seconds(), ..active.clone() })
}

fn is_zero(n: &i64) -> bool {
    *n == 0
}

impl ActiveTimer {
    pub fn is_note(&self, note: &str) -> bool {
        self.note.as_deref() == Some(note)
    }

    /// Et telefonopkald, der endnu ikke hører til et projekt.
    pub fn is_call(&self) -> bool {
        self.project_id.is_none()
    }

    /// Tiden uret viser: denne tidstagning plus evt. tid fra før en pause.
    pub fn elapsed(&self) -> Duration {
        Local::now() - self.start + Duration::seconds(self.carried_secs)
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
    /// Projekter hvis afrundede tid en given dag er rundet ned manuelt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub round_down: Vec<RoundDown>,
    #[serde(default)]
    pub settings: Settings,
    /// Vinduet er foldet sammen til en lille bjælke.
    #[serde(default)]
    pub compact: bool,
    /// Vinduets størrelse (skærmpunkter), før det blev foldet sammen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal_size: Option<[f32; 2]>,
}

/// Brugerens indstillinger (menuen ☰).
#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Spørg om tiden skal være pause, når man har været væk fra computeren.
    pub away_enabled: bool,
    /// Hvor mange minutter man skal have været væk, før der spørges.
    pub away_minutes: u32,
    /// Lukkeknappen skjuler vinduet i systembakken i stedet for at afslutte.
    pub close_to_tray: bool,
    /// Foreslå en kort pause, når man har arbejdet `break_minutes` uden.
    pub break_reminder: bool,
    pub break_minutes: u32,
    pub shortcuts: crate::shortcuts::Shortcuts,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            away_enabled: true,
            away_minutes: 15,
            close_to_tray: true,
            break_reminder: false,
            break_minutes: 60,
            shortcuts: Default::default(),
        }
    }
}

/// Et manuelt valg om at runde et projekts tid en bestemt dag ned i stedet for op.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct RoundDown {
    pub date: NaiveDate,
    pub project_id: Id,
}

/// Et projekts tid en dag, afrundet til kvarter.
pub struct Rounded {
    pub time: Duration,
    /// Rundet ned manuelt i stedet for op.
    pub down: bool,
    /// Om op og ned giver forskellige tider (ellers er der intet at vælge).
    pub can_toggle: bool,
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
    /// Kunder der ikke er slettet: den interne kunde først, derefter alfabetisk.
    pub fn visible_customers(&self) -> impl Iterator<Item = &Customer> {
        let mut customers: Vec<&Customer> = self.customers.iter().filter(|c| !c.deleted).collect();
        customers.sort_by_cached_key(|c| (!self.is_internal_customer(c.id), sort_key(&c.name)));
        customers.into_iter()
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

    /// Omdøber en kunde. Navnet må ikke være tomt eller allerede bruges af en
    /// anden kunde (uanset store/små bogstaver).
    pub fn rename_customer(&mut self, customer_id: Id, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Navnet må ikke være tomt".to_owned());
        }
        if self.visible_customers().any(|c| c.id != customer_id && same_name(&c.name, name)) {
            return Err("Der findes allerede en kunde med det navn".to_owned());
        }
        let customer = self.customers.iter_mut().find(|c| c.id == customer_id).ok_or("Kunden findes ikke")?;
        customer.name = name.to_owned();
        Ok(())
    }

    /// Omdøber et projekt. Navnet må ikke være tomt eller allerede bruges af et
    /// andet projekt hos samme kunde.
    pub fn rename_project(&mut self, project_id: Id, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Navnet må ikke være tomt".to_owned());
        }
        let customer_id = self.project(project_id).ok_or("Projektet findes ikke")?.customer_id;
        if self
            .visible_projects()
            .any(|p| p.id != project_id && p.customer_id == customer_id && same_name(&p.name, name))
        {
            return Err("Kunden har allerede et projekt med det navn".to_owned());
        }
        if let Some(project) = self.projects.iter_mut().find(|p| p.id == project_id) {
            project.name = name.to_owned();
        }
        Ok(())
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

    /// Starter en pause, som registreres på "Intern tid". Kører der en pause,
    /// afsluttes den i stedet, og tiden fortsætter, hvor den slap før pausen.
    pub fn toggle_pause(&mut self) {
        if self.active.as_ref().is_some_and(|a| a.is_note(NOTE_PAUSE)) {
            self.end_interruption();
            return;
        }
        // Under et opkald: pausen afslutter opkaldet og vender bagefter tilbage
        // til det, der kørte før opkaldet.
        let previous = match self.active.as_ref() {
            Some(a) if a.is_call() => a.previous.clone(),
            other => other.map(frozen),
        };
        let id = self.internal_project_id();
        self.start_timer(Some(id), Some(NOTE_PAUSE));
        if let Some(active) = &mut self.active {
            active.previous = previous;
        }
    }

    /// Starter et telefonopkald. Kører der et opkald, afsluttes det i stedet, og
    /// tiden fortsætter på det, der kørte, da telefonen ringede. Opkaldet
    /// registreres på sit projekt, eller venter på at få et.
    pub fn toggle_call(&mut self) {
        if self.active.as_ref().is_some_and(ActiveTimer::is_call) {
            self.end_interruption();
            return;
        }
        let previous = self.active.as_ref().map(frozen);
        self.start_call();
        if let Some(active) = &mut self.active {
            active.previous = previous;
        }
    }

    /// Afslutter den kørende pause/det kørende opkald og fortsætter på det, der
    /// kørte før: samme projekt, note og type, og uret tæller videre derfra.
    fn end_interruption(&mut self) {
        let Some(active) = self.active.as_ref() else { return };
        let previous = active.previous.clone();
        self.stop();
        let Some(previous) = previous else { return };
        if let Some(id) = previous.project_id
            && !self.project(id).is_some_and(|p| !p.deleted)
        {
            return; // projektet er slettet i mellemtiden
        }
        let now = Local::now();
        self.active = Some(ActiveTimer { start: now, last_seen: now, ..*previous });
    }

    /// Projektet en kørende pause eller et kørende opkald vender tilbage til,
    /// hvis det stadig findes.
    pub fn resume_project(&self) -> Option<Id> {
        let id = self.active.as_ref()?.previous.as_ref()?.project_id?;
        self.project(id).filter(|p| !p.deleted).map(|p| p.id)
    }

    /// Flytter tiden fra `from` til `to` fra den kørende timer over på pause.
    /// Tiden før `from` gemmes på projektet, perioden bliver en pause, og timeren
    /// kører videre på projektet fra `to`, mens uret fortsætter uden pausen.
    /// Gør intet og returnerer false, hvis timeren ikke længere passer.
    pub fn convert_to_pause(&mut self, project_id: Id, from: DateTime<Local>, to: DateTime<Local>) -> bool {
        let Some(active) = self.active.clone() else { return false };
        if active.project_id != Some(project_id) || active.is_note(NOTE_PAUSE) || from < active.start || to <= from {
            return false;
        }
        let internal = self.internal_project_id();
        let before = from - active.start;
        if before >= Duration::seconds(1) {
            self.entries.push(TimeEntry {
                project_id,
                start: active.start,
                end: from,
                note: active.note.clone(),
                description: active.description.clone(),
            });
        }
        self.entries.push(TimeEntry {
            project_id: internal,
            start: from,
            end: to,
            note: Some(NOTE_PAUSE.to_owned()),
            ..Default::default()
        });
        if let Some(active) = &mut self.active {
            active.start = to;
            active.carried_secs += before.num_seconds();
            active.last_seen = active.last_seen.max(to);
        }
        true
    }

    fn start_timer(&mut self, project_id: Option<Id>, note: Option<&str>) {
        let now = Local::now();
        self.stop_at(now);
        self.active = Some(ActiveTimer {
            project_id,
            start: now,
            last_seen: now,
            note: note.map(str::to_owned),
            ..Default::default()
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
                description: active.description,
            }),
            None => {
                let call = UnassignedEntry {
                    start: active.start,
                    end,
                    target: active.target,
                    description: active.description,
                };
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
    pub fn resolve_target(&mut self, target: &CallTarget) -> Option<Id> {
        if let Some(id) = target.project_id
            && self.project(id).is_some_and(|p| !p.deleted)
        {
            return Some(id);
        }
        let (customer, project) = target.typed()?;
        let existing = self.visible_customers().find(|c| same_name(&c.name, customer)).map(|c| c.id);
        let customer_id = match existing {
            Some(id) => id,
            None => self.add_customer(customer),
        };
        let existing = self
            .visible_projects()
            .find(|p| p.customer_id == customer_id && same_name(&p.name, project))
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
            description: call.description.clone(),
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
    /// hvis den hører til et projekt. Sorteret efter starttid.
    pub fn entries_on(&self, date: NaiveDate) -> Vec<DayEntry> {
        let mut list: Vec<DayEntry> = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.start.date_naive() == date)
            .map(|(i, e)| DayEntry { index: Some(i), entry: e.clone() })
            .collect();
        if let Some(entry) = self.running_entry()
            && entry.start.date_naive() == date
        {
            list.push(DayEntry { index: None, entry });
        }
        list.sort_by_key(|e| e.entry.start);
        list
    }

    /// Den kørende timer som registrering frem til nu, hvis den hører til et projekt.
    fn running_entry(&self) -> Option<TimeEntry> {
        let a = self.active.as_ref()?;
        Some(TimeEntry {
            project_id: a.project_id?,
            start: a.start,
            end: Local::now(),
            note: a.note.clone(),
            description: a.description.clone(),
        })
    }

    pub fn add_entry(&mut self, entry: TimeEntry) {
        self.entries.push(entry);
    }

    pub fn update_entry(&mut self, index: usize, entry: TimeEntry) {
        if let Some(e) = self.entries.get_mut(index) {
            *e = entry;
        }
    }

    pub fn delete_entry(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
        }
    }

    /// Andre registreringer (inkl. den kørende timer) der overlapper tidsrummet.
    /// `skip` er registreringen selv, når den rettes.
    pub fn overlapping(&self, entry: &TimeEntry, skip: Option<usize>) -> Vec<TimeEntry> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != skip)
            .map(|(_, e)| e.clone())
            .chain(self.running_entry())
            .filter(|e| e.start < entry.end && entry.start < e.end)
            .collect()
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

    /// Afrundet tid for et projekt en dag: op til nærmeste kvarter, eller ned
    /// hvis det er valgt manuelt.
    pub fn rounded(&self, date: NaiveDate, project_id: Id, time: Duration) -> Rounded {
        let (up, down) = (round_up_quarter(time), round_down_quarter(time));
        let chosen_down = self.round_down.contains(&RoundDown { date, project_id });
        Rounded {
            time: if chosen_down { down } else { up },
            down: chosen_down && up != down,
            can_toggle: up != down,
        }
    }

    /// Skifter mellem at runde projektets tid den dag op eller ned.
    pub fn toggle_round_down(&mut self, date: NaiveDate, project_id: Id) {
        let item = RoundDown { date, project_id };
        match self.round_down.iter().position(|r| *r == item) {
            Some(i) => {
                self.round_down.remove(i);
            }
            None => self.round_down.push(item),
        }
    }

    /// Samlet tid pr. projekt på en dag, sorteret efter mest tid først.
    pub fn totals_on(&self, date: NaiveDate) -> Vec<(Id, Duration)> {
        let mut totals: Vec<(Id, Duration)> = Vec::new();
        for DayEntry { entry: e, .. } in self.entries_on(date) {
            match totals.iter_mut().find(|(id, _)| *id == e.project_id) {
                Some((_, d)) => *d += e.duration(),
                None => totals.push((e.project_id, e.duration())),
            }
        }
        totals.sort_by_key(|(_, d)| std::cmp::Reverse(*d));
        totals
    }
}

/// Nøgle til alfabetisk sortering efter dansk alfabet: store og små bogstaver
/// sorteres ens, og æ, ø, å kommer efter z i den rækkefølge (i Unicode ligger
/// å før æ). "{", "|" og "}" kommer lige efter "z" i ASCII.
fn sort_key(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'æ' => '{',
            'ø' => '|',
            'å' => '}',
            c => c,
        })
        .collect()
}

/// Navne sammenlignes uden hensyn til store/små bogstaver (også æ, ø, å).
fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Runder op til nærmeste påbegyndte kvarter. Hele kvarterer og nul ændres ikke.
pub fn round_up_quarter(d: Duration) -> Duration {
    let quarter = 15 * 60;
    let secs = d.num_seconds().max(0);
    Duration::seconds((secs + quarter - 1) / quarter * quarter)
}

/// Runder ned til nærmeste hele kvarter.
pub fn round_down_quarter(d: Duration) -> Duration {
    let quarter = 15 * 60;
    Duration::seconds(d.num_seconds().max(0) / quarter * quarter)
}

/// Timer og minutter, f.eks. "1:15".
pub fn format_hours_minutes(d: Duration) -> String {
    let mins = d.num_minutes().max(0);
    format!("{}:{:02}", mins / 60, mins % 60)
}

pub fn format_duration(d: Duration) -> String {
    let secs = d.num_seconds().max(0);
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn active_since(project_id: Option<Id>, minutes: i64) -> Option<ActiveTimer> {
        Some(ActiveTimer {
            project_id,
            start: Local::now() - Duration::minutes(minutes),
            last_seen: Local::now(),
            note: None,
            ..Default::default()
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
    fn edit_and_delete_entries_and_find_overlaps() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        let at = |h: u32, m: u32| {
            Local.from_local_datetime(&NaiveDate::from_ymd_opt(2026, 10, 20).unwrap().and_hms_opt(h, m, 0).unwrap())
                .unwrap()
        };
        let entry = |from, to| TimeEntry { project_id: p, start: from, end: to, ..Default::default() };
        data.add_entry(entry(at(8, 0), at(9, 0)));
        data.add_entry(entry(at(10, 0), at(11, 0)));

        // 8:30-10:30 rammer begge; rettes nr. 0 til det, overlapper den kun nr. 1.
        let changed = entry(at(8, 30), at(10, 30));
        assert_eq!(data.overlapping(&changed, None).len(), 2);
        assert_eq!(data.overlapping(&changed, Some(0)).len(), 1);
        // Registreringer der bare støder op til hinanden overlapper ikke.
        assert!(data.overlapping(&entry(at(9, 0), at(10, 0)), None).is_empty());

        data.update_entry(0, changed);
        assert_eq!(data.entries[0].duration(), Duration::hours(2));
        data.delete_entry(1);
        assert_eq!(data.entries.len(), 1);
        assert_eq!(data.entries_on(NaiveDate::from_ymd_opt(2026, 10, 20).unwrap())[0].index, Some(0));
    }

    #[test]
    fn rename_rejects_empty_and_duplicate_names() {
        let mut data = Data::default();
        let a = data.add_customer("Alfa");
        let b = data.add_customer("Beta");
        let p1 = data.add_project(a, "Web");
        let p2 = data.add_project(a, "App");
        let other = data.add_project(b, "Andet");

        assert!(data.rename_customer(a, "  ").is_err());
        assert!(data.rename_customer(a, "beta").is_err(), "navnet bruges af en anden kunde");
        assert!(data.rename_customer(a, "ALFA").is_ok(), "samme kunde må skifte store/små bogstaver");
        assert!(data.rename_customer(a, " Gamma ").is_ok());
        assert_eq!(data.customer(a).unwrap().name, "Gamma");

        assert!(data.rename_project(p1, "app").is_err(), "navnet bruges hos samme kunde");
        assert!(data.rename_project(other, "Web").is_ok(), "samme navn hos en anden kunde er fint");
        assert!(data.rename_project(p2, "Mobil").is_ok());
        assert_eq!(data.project_label(p2), "Gamma / Mobil");
    }

    #[test]
    fn pressing_pause_again_resumes_previous_project() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.active = active_since(Some(p), 30);

        data.toggle_pause();
        let internal = data.internal_project_id();
        assert_eq!(data.active.as_ref().unwrap().project_id, Some(internal));
        assert_eq!(data.resume_project(), Some(p));
        data.active.as_mut().unwrap().start -= Duration::minutes(10);

        data.toggle_pause();
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(p), "tilbage på projektet");
        let shown = active.elapsed().num_minutes();
        assert_eq!(shown, 30, "uret tæller videre fra de 30 min før pausen, ikke fra 0");
        assert!(active.note.is_none());
        assert_eq!(data.entries.len(), 2, "projekttid og pause er gemt");
        assert_eq!(data.entries[1].note.as_deref(), Some(NOTE_PAUSE));
    }

    #[test]
    fn away_time_can_be_converted_to_pause() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        let now = Local::now();
        data.active = Some(ActiveTimer {
            project_id: Some(p),
            start: now - Duration::minutes(60),
            last_seen: now,
            note: None,
            ..Default::default()
        });
        // Væk fra 40 til 10 minutter siden.
        let (from, to) = (now - Duration::minutes(40), now - Duration::minutes(10));
        assert!(data.convert_to_pause(p, from, to));

        let internal = data.internal_project_id();
        assert_eq!(data.entries.len(), 2);
        assert_eq!(data.entries[0].project_id, p);
        assert_eq!(data.entries[0].duration(), Duration::minutes(20));
        assert_eq!(data.entries[1].project_id, internal);
        assert_eq!(data.entries[1].duration(), Duration::minutes(30));
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(p));
        assert_eq!(active.start, to);
        assert_eq!(active.elapsed().num_minutes(), 30, "uret viser 20 + 10 min uden pausen");

        assert!(!data.convert_to_pause(c + 100, from, to), "andet projekt end det der kører");
    }

    #[test]
    fn settings_default_when_missing_from_file() {
        let data: Data = serde_json::from_str(r#"{"next_id":0,"customers":[],"projects":[],"entries":[],"active":null}"#).unwrap();
        assert!(data.settings.away_enabled);
        assert_eq!(data.settings.away_minutes, 15);
    }

    #[test]
    fn note_follows_entry_through_stop_pause_and_call() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");

        data.start(p);
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(10);
        active.description = "Forside".to_owned();
        data.toggle_pause();
        assert_eq!(data.entries[0].description, "Forside");
        assert!(data.active.as_ref().unwrap().description.is_empty(), "pausen har sin egen note");

        data.active.as_mut().unwrap().start -= Duration::minutes(5);
        data.toggle_pause();
        assert_eq!(data.active.as_ref().unwrap().description, "Forside", "noten følger med tilbage");

        data.start_call();
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(3);
        active.description = "Spørgsmål om faktura".to_owned();
        data.stop();
        assert_eq!(data.unassigned[0].description, "Spørgsmål om faktura");
        data.unassigned[0].target.project_id = Some(p);
        assert!(data.register_first_unassigned());
        assert_eq!(data.entries.last().unwrap().description, "Spørgsmål om faktura");
    }

    #[test]
    fn pressing_phone_again_ends_call_and_resumes_project() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.start(p);
        let active = data.active.as_mut().unwrap();
        active.start -= Duration::minutes(30);
        active.description = "Forside".to_owned();

        data.toggle_call();
        assert!(data.active.as_ref().unwrap().is_call());
        assert_eq!(data.resume_project(), Some(p));
        data.active.as_mut().unwrap().start -= Duration::minutes(5);

        data.toggle_call();
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(p), "tilbage på projektet");
        assert_eq!(active.description, "Forside", "noten er med");
        assert_eq!(active.elapsed().num_minutes(), 30, "uret tæller videre uden opkaldet");
        assert_eq!(data.entries.len(), 1, "projekttiden før opkaldet");
        assert_eq!(data.unassigned.len(), 1, "opkaldet venter på et projekt");
    }

    #[test]
    fn call_during_pause_returns_to_pause_then_project() {
        let mut data = Data::default();
        let c = data.add_customer("Kunde");
        let p = data.add_project(c, "A");
        data.start(p);
        data.active.as_mut().unwrap().start -= Duration::minutes(20);
        data.toggle_pause();
        data.active.as_mut().unwrap().start -= Duration::minutes(5);

        data.toggle_call();
        data.active.as_mut().unwrap().target.project_id = Some(p);
        data.active.as_mut().unwrap().start -= Duration::minutes(2);
        data.toggle_call();
        assert!(data.active.as_ref().unwrap().is_note(NOTE_PAUSE), "tilbage på pausen");
        assert_eq!(data.entries.last().unwrap().note.as_deref(), Some(NOTE_PHONE), "opkaldet registreret");

        data.toggle_pause();
        let active = data.active.as_ref().unwrap();
        assert_eq!(active.project_id, Some(p), "og så tilbage på projektet");
        assert_eq!(active.elapsed().num_minutes(), 20);
    }

    #[test]
    fn call_without_running_timer_just_stops() {
        let mut data = Data::default();
        data.toggle_call();
        data.active.as_mut().unwrap().start -= Duration::minutes(3);
        data.toggle_call();
        assert!(data.active.is_none());
        assert_eq!(data.unassigned.len(), 1);
    }

    #[test]
    fn pause_without_previous_project_just_stops() {
        let mut data = Data::default();
        data.toggle_pause();
        data.active.as_mut().unwrap().start -= Duration::minutes(5);
        data.toggle_pause();
        assert!(data.active.is_none());
        assert_eq!(data.entries.len(), 1);
    }

    #[test]
    fn rounds_up_to_next_quarter() {
        let r = |secs| round_up_quarter(Duration::seconds(secs)).num_minutes();
        assert_eq!(r(0), 0);
        assert_eq!(r(1), 15);
        assert_eq!(r(15 * 60), 15);
        assert_eq!(r(15 * 60 + 1), 30);
        assert_eq!(r(47 * 60), 60);
        assert_eq!(r(61 * 60), 75);
        assert_eq!(format_hours_minutes(Duration::minutes(75)), "1:15");
    }

    #[test]
    fn rounding_can_be_toggled_down_per_project_and_day() {
        let mut data = Data::default();
        let day = NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        let other_day = day.succ_opt().unwrap();
        let just_over = Duration::seconds(60 * 60 + 1);

        assert_eq!(data.rounded(day, 1, just_over).time, Duration::minutes(75));
        data.toggle_round_down(day, 1);
        let r = data.rounded(day, 1, just_over);
        assert_eq!(r.time, Duration::minutes(60));
        assert!(r.down);
        assert_eq!(data.rounded(other_day, 1, just_over).time, Duration::minutes(75), "kun den ene dag");
        assert_eq!(data.rounded(day, 2, just_over).time, Duration::minutes(75), "kun det ene projekt");

        let exact = data.rounded(day, 1, Duration::minutes(60));
        assert!(!exact.can_toggle && !exact.down, "præcis et kvarter kan ikke rundes");

        data.toggle_round_down(day, 1);
        assert_eq!(data.rounded(day, 1, just_over).time, Duration::minutes(75));
        assert!(data.round_down.is_empty());
    }

    #[test]
    fn customers_are_sorted_with_internal_first() {
        let mut data = Data::default();
        for name in ["Øster", "beta", "Århus", "Alfa", "Ærø", "Zulu"] {
            data.add_customer(name);
        }
        data.internal_project_id(); // opretter "Intern" sidst
        let names: Vec<&str> = data.visible_customers().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Intern", "Alfa", "beta", "Zulu", "Ærø", "Øster", "Århus"]);
    }

    #[test]
    fn pause_is_registered_on_internal_project() {
        let mut data = Data::default();
        data.toggle_pause();
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
        data.active = Some(ActiveTimer { project_id: Some(p), start, last_seen, ..Default::default() });
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
