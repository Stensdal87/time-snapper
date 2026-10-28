//! Sprog: dansk og engelsk. Teksterne står i `locales/app.yml` (dansk og
//! engelsk side om side) og hentes med `tr!("nøgle")`.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default, Debug)]
pub enum Language {
    /// Systemets sprog: dansk hvis systemet er på dansk, ellers engelsk.
    #[default]
    System,
    Danish,
    English,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::System, Language::Danish, Language::English];

    fn code(self) -> &'static str {
        match self {
            Language::System => {
                let danish = sys_locale::get_locale().is_some_and(|l| l.to_lowercase().starts_with("da"));
                if danish { "da" } else { "en" }
            }
            Language::Danish => "da",
            Language::English => "en",
        }
    }

    /// Navnet i menuen, på sproget selv (så man kan finde sit sprog).
    pub fn label(self) -> String {
        match self {
            Language::System => tr!("settings.language_system"),
            Language::Danish => "Dansk".to_owned(),
            Language::English => "English".to_owned(),
        }
    }
}

/// Skifter sprog for hele programmet.
pub fn apply(language: Language) {
    rust_i18n::set_locale(language.code());
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    /// Læser locales/app.yml: nøgle → (dansk, engelsk).
    fn load() -> BTreeMap<String, BTreeMap<String, String>> {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/locales/app.yml")).unwrap();
        let mut keys: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut current = None;
        for line in text.lines() {
            if line.starts_with("_version") || line.trim().is_empty() || line.trim_start().starts_with('#') {
                continue;
            }
            if let Some(key) = line.strip_suffix(':').filter(|_| !line.starts_with(' ')) {
                assert!(keys.insert(key.to_owned(), BTreeMap::new()).is_none(), "nøglen {key} står to gange");
                current = Some(key.to_owned());
            } else if let Some((lang, value)) = line.trim_start().split_once(": ") {
                let key = current.clone().expect("oversættelse uden nøgle");
                let value: String = serde_json::from_str(value).unwrap_or_else(|e| panic!("{key}.{lang}: {e}"));
                keys.get_mut(&key).unwrap().insert(lang.to_owned(), value);
            }
        }
        keys
    }

    fn placeholders(text: &str) -> BTreeSet<String> {
        text.split("%{").skip(1).filter_map(|s| s.split_once('}').map(|(name, _)| name.to_owned())).collect()
    }

    /// Hver tekst skal findes på både dansk og engelsk med de samme pladsholdere.
    #[test]
    fn every_text_is_in_danish_and_english() {
        for (key, langs) in load() {
            let (da, en) = (&langs["da"], &langs["en"]);
            assert!(!da.is_empty() && !en.is_empty(), "{key} mangler en oversættelse");
            assert_eq!(placeholders(da), placeholders(en), "{key}: forskellige pladsholdere");
        }
    }

    /// Alle nøgler, koden bruger, skal findes i sprogfilen.
    #[test]
    fn every_key_in_the_code_exists() {
        let keys = load();
        let mut missing = Vec::new();
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = vec![src];
        while let Some(path) = files.pop() {
            if path.is_dir() {
                files.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            // Kommentarer (eksempler som tr!("nøgle")) tæller ikke.
            for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
                for part in line.split("tr!(\"").skip(1) {
                    let key = part.split('"').next().unwrap();
                    if !keys.contains_key(key) {
                        missing.push(format!("{key} (i {})", path.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "nøgler uden tekst:\n{}", missing.join("\n"));
    }
}
