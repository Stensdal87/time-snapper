# Time Snapper

Et tidsregistreringsprogram skrevet i Rust. Det hed tidligere "Time Tracker", og projektmappen hedder stadig `Dev/Time Tracker`. Projektet er på proof-of-concept-stadiet.

## Platforme
- Udvikles og testes på Linux (Fedora Atomic, `/home` -> `/var/home`).
- Skal på sigt også køre på Windows, så vælg cross-platform crates og undgå Linux-specifikke stier/API'er
  (brug f.eks. `directories`/`dirs` til config- og datamapper).

## Kommandoer
- Byg: `cargo build`
- Kør: `cargo run`
- Test: `cargo test`
- Lint/format: `cargo clippy` og `cargo fmt`

Cargo ligger i `~/.cargo/bin` (rustup). Den er måske ikke i PATH i alle shells.

## Hvad programmet gør
Et lille vindue (der kan holdes øverst), som tager tid på projekter under kunder. Det er til én bruger, uden samarbejde.
- Kunder har projekter. Man klikker på et projekt for at starte tid på det, og skifter man projekt, gemmes den forrige tid automatisk.
- Et rundt ur tæller op: ringen fyldes én gang i timen, og prikken går rundt én gang i minuttet.
- Telefon-knappen starter en timer uden projekt (`ActiveTimer.project_id = None`). Når den stoppes, eller man skifter projekt, havner opkaldet i `Data.unassigned`. Så længe listen ikke er tom, viser en modal dialog, hvor tiden skal registreres: Intern tid, et eksisterende projekt, et nyt projekt eller kassér. Den nye timer kører videre imens.
- Et opkalds mål (`CallTarget`) er enten et projekt-id eller indtastet kunde og projekt. Kunde og projekt genbruges hvis de findes, uden hensyn til store og små bogstaver, og oprettes ellers. Målet kan udfyldes under opkaldet (`ActiveTimer.target`), og så registreres opkaldet direkte ved stop eller skift. Ellers bruges `UnassignedEntry.target` i dialogen. I dialogen registrerer Enter, når målet er udfyldt, og det samme gør et dobbeltklik på et projekt. Esc eller "Senere" lukker dialogen, mens opkaldet bliver ved med at vente, og en gul knap i Timer-fanen åbner den igen.
- Søgefeltet (`matching_projects` i `app.rs`) findes i projektlisten og i opkaldsdialogen. Hvis kundenavnet matcher, vises alle kundens projekter, ellers kun de projekter, der matcher. Der skelnes ikke mellem store og små bogstaver. I dialogen vælger Enter det eneste træf, hvis søgningen kun giver ét.
- Tema-knappen (💻/☀/🌙) skifter mellem system, lyst og mørkt. Valget gemmes som `Data.theme`.
- Pause-knappen registrerer tid på projektet "Intern / Intern tid". Det oprettes automatisk og spores via `internal_project`.
- Registreringer kan have en `note` ("Telefon"/"Pause"), som vises i historikken.
- Højreklik på en kunde giver: Udfold alle, Sammenklap alle, Omdøb og Slet kunde. Højreklik på et projekt giver: Omdøb og Slet projekt. Omdøbning (`rename_customer` og `rename_project`) afviser tomme navne og navne, der allerede bruges (kunder globalt, projekter inden for samme kunde, uden hensyn til store og små bogstaver). Intern og Intern tid kan omdøbes, fordi de spores via id. Sletning skal bekræftes i en dialog (Esc annullerer). Det er en "blød" sletning: `deleted: true` skjuler kunden eller projektet alle steder (`visible_customers` og `visible_projects`), men historikken beholder navne og tid. Kører timeren på projektet, stoppes den først. Intern kan ikke slettes. Skriver man et slettet navn i opkaldsdialogen, oprettes en ny kunde eller et nyt projekt.
- Rette registreringer (`src/entry_editor.rs`): I "Alle registreringer" åbner et klik eller højreklik på en linje en modal dialog, hvor man retter projekt, type, dato og tid eller sletter. Sletning kræver et ekstra "Ja, slet". ➕ tilføjer en ny registrering. `Data::entries_on` returnerer `DayEntry` med et `index` ind i `entries` (`None` er den kørende timer, som ikke kan rettes). Tidsfelter, man ikke har rørt, beholder deres sekunder. Overlap giver kun en advarsel og blokerer ikke. Tid over midnat understøttes ikke.
- Eksport (📤 i Historik): `src/export.rs` henter rækkerne (`collect`, hele dage fra og med til og med, kun afsluttede registreringer), summerer dem og skriver filen. CSV er i dansk Excel-format (`;`, decimalkomma, UTF-8 med BOM) og har enten alle registreringer eller den summerede liste. Excel (`rust_xlsxwriter`) har fanerne Registreringer og Summeret med SUM-totaler. `src/export_dialog.rs` er dialogen med periodeknapper (uger går mandag til søndag) og "Gem som" via `rfd` (xdg-portal på Linux). "Gem som" får vores vindue som forælder (`set_parent(frame)`). Fordi KDE's dialog kan havne bag et vindue, der ligger øverst, sættes vinduesniveauet desuden til Normal en frame (150 ms) før dialogen åbnes, og bagefter tilbage til AlwaysOnTop.
- Historikken har to visninger. "Summeret" giver én linje pr. projekt med antal registreringer og total. "Alle registreringer" viser hver registrering for sig. Summeret er standard og vælges igen, hver gang man går over på fanen.
- Fanen Historik viser registreringer og totaler pr. dag, og man kan bladre mellem dagene.

## Struktur
- `src/main.rs` står for vinduesopsætningen (eframe/egui 0.37). Bemærk den nye API: `App::ui(&mut self, ui, frame)` og `egui::Panel::top/bottom(...).show(ui, ..)`.
- `src/data.rs` indeholder datamodellen og lagringen og har unit tests.
- `src/app.rs` indeholder brugerfladen.
- `src/export.rs` og `src/export_dialog.rs` står for eksport til CSV og Excel.
- `src/entry_editor.rs` indeholder dialogen til at rette og tilføje registreringer, inkl. tolkning af tid og dato.

Hold øverst på Linux: Wayland tillader det ikke, og winit ignorerer det her uden at sige noget. Derfor tvinger `main.rs` X11/XWayland (via `winit` med `with_x11`, kun Linux). På X11 går niveauet tabt, hvis det sættes før vinduet vises, så `app.rs` sender det igen 0,5 sekund efter opstart. Man kan tjekke det med `xprop -id <id> _NET_WM_STATE`.

Vinduets størrelse og placering huskes af eframe (feature `persistence`, filen `app.ron` ved siden af `data.json`). På X11/KWin kryber placeringen en titellinjehøjde nedad ved hver genstart, fordi winit ikke kender rammens størrelse (outer_rect er lig inner_rect). Derfor gemmes den indre placering også i `Data.window_pos` ved lukning. `restore_window_position` beder så om `gemt + (gemt − faktisk)` 0,5 sekund efter opstart. Det er testet med `wmctrl -e`/`wmctrl -c` og `xwininfo` over flere genstarter.

Data ligger i `~/.local/share/timesnapper/data.json` (på Windows i `%APPDATA%\TimeSnapper\data\`). Ved opstart kopierer `migrate_from_old_name` i `main.rs` filerne fra de gamle "Time Tracker"-placeringer, hvis der ikke allerede ligger nye. Filen gemmes atomisk ved hver ændring.
- Kørende timer: `active` gemmes med en `last_seen`, der opdateres hvert 30. sekund. Lukkes vinduet, stoppes timeren. Efter et nedbrud afsluttes timeren ved `last_seen`.

## Status
- Under git (branch `main`). Ligger på GitHub som privat repo: https://github.com/Stensdal87/time-snapper
- Første proof of concept er lavet. Mulige næste skridt: gendanne slettede, uge- og månedsoverblik, ikon i proceslinjen, Windows-build.
