# Time Snapper ⏱

Et lille, simpelt tidsregistreringsprogram til at tage tid på projekter for kunder.
Det kan ligge øverst på skærmen uden at være i vejen.

## Funktioner

- Kunder og projekter. Klik på et projekt for at starte tiden, og når du skifter projekt, gemmes tiden automatisk
- Rundt ur der tæller op og viser både den aktuelle tidstagning og den samlede tid på projektet
- **📞 Telefon**: start tid på et opkald og vælg kunde/projekt under eller efter opkaldet
- **☕ Pause**: registreres automatisk på "Intern tid". Tryk igen for at fortsætte på projektet fra før pausen
- Historik pr. dag, enten summeret pr. projekt eller med alle registreringer
- Ret, slet og tilføj registreringer, og omdøb kunder og projekter
- Eksportér en periode til Excel (.xlsx) eller CSV
- Søgning i kunder og projekter, og højreklik for at udfolde/sammenklappe alle eller slette
- Spørger om tiden skal være pause, når du har været væk fra computeren (indstilles i ☰-menuen)
- Lyst/mørkt tema, "hold øverst", og programmet husker vinduets størrelse og placering

## Byg og kør

Kræver [Rust](https://rustup.rs/).

```sh
cargo run --release
```

Programmet er skrevet med [egui](https://github.com/emilk/egui) og er lavet til at kunne køre på både Linux og Windows.

## Data

Alt gemmes lokalt i én JSON-fil:

- Linux: `~/.local/share/timesnapper/data.json`
- Windows: `%APPDATA%\TimeSnapper\data\data.json`
