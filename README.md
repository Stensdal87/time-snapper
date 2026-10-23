# Time Snapper ⏱

Et lille, simpelt tidsregistreringsprogram til at tage tid på projekter for kunder.
Det kan ligge øverst på skærmen uden at være i vejen.

## Funktioner

- Kunder og projekter. Klik på et projekt for at starte tiden, og når du skifter projekt, gemmes tiden automatisk
- Rundt ur der tæller op og viser både den aktuelle tidstagning og den samlede tid på projektet
- **📞 Telefon**: start tid på et opkald og vælg kunde/projekt under eller efter opkaldet
- **☕ Pause**: registreres automatisk på "Intern tid". Tryk igen for at fortsætte på projektet fra før pausen
- Historik pr. dag, enten summeret pr. projekt eller med alle registreringer
- Noter på tidsregistreringer, også med i historik og eksport
- Ret, slet og tilføj registreringer, og omdøb kunder og projekter
- Eksportér en periode til Excel (.xlsx) eller CSV
- Søgning i kunder og projekter, og højreklik for at udfolde/sammenklappe alle eller slette
- Spørger om tiden skal være pause, når du har været væk fra computeren (indstilles i ☰-menuen)
- Kan foldes sammen til en lille bjælke med tid og ikonknapper (som en mini-afspiller), der kan ligge i et hjørne af skærmen
- Ikon i systembakken: start, skift projekt, pause og stop uden at vise vinduet
- Kan starte automatisk, når du logger på (slås til/fra i ☰-menuen)
- Lyst/mørkt tema, "hold øverst", og programmet husker vinduets størrelse og placering

## Byg og kør

Kræver [Rust](https://rustup.rs/).

```sh
cargo run --release
```

Programmet er skrevet med [egui](https://github.com/emilk/egui) og er lavet til at kunne køre på både Linux og Windows.

## Installation

Hent den nyeste version under [Releases](https://github.com/Stensdal87/time-snapper/releases):

- **Windows:** `TimeSnapper-Setup-<version>.exe` (installerer for din bruger, uden administratorrettigheder)
- **Linux:** `TimeSnapper-<version>-linux-x86_64.tar.gz`. Pak den ud og kør `./install.sh` (installerer for din bruger i `~/.local/bin`, uden root)

## Data

Alt gemmes lokalt i én JSON-fil:

- Linux: `~/.local/share/timesnapper/data.json`
- Windows: `%APPDATA%\TimeSnapper\data\data.json`
