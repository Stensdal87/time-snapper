//! Guidet åndedrætsøvelse i et lille vindue for sig, til pauserne.
//!
//! En øvelse er en række faser (ind, hold, ud, hold) med en varighed hver.
//! Hver øvelse tegnes som en figur med én side pr. fase, og retningen viser
//! hvad man gør: ind går opad, hold er vandret (langs toppen med fulde lunger,
//! langs bunden med tomme), og ud går nedad. Længden følger varigheden; ud er
//! den side der lukker figuren. Boksvejrtrækning bliver en firkant, 4-7-8 en
//! retvinklet trekant, og ind/ud uden hold en lodret linje. En cirkel i midten
//! vokser, når man trækker vejret ind.

use std::f32::consts::PI;

use serde::{Deserialize, Serialize};

use super::*;

/// De øvelser man kan vælge imellem i indstillingerne.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Default, Debug)]
pub enum Exercise {
    /// Ind 4 s, hold 4 s, ud 4 s, hold 4 s.
    #[default]
    Box,
    /// Ind 4 s, hold 7 s, ud 8 s.
    FourSevenEight,
    /// Ind 5 s, hold 7 s, ud 8 s.
    FiveSevenEight,
    /// Ind 4 s, ud 6 s.
    Calm,
}

impl Exercise {
    pub const ALL: [Exercise; 4] = [Exercise::Box, Exercise::FourSevenEight, Exercise::FiveSevenEight, Exercise::Calm];

    pub fn label(self) -> String {
        match self {
            Exercise::Box => tr!("breathing.ex_box"),
            Exercise::FourSevenEight => tr!("breathing.ex_478"),
            Exercise::FiveSevenEight => tr!("breathing.ex_578"),
            Exercise::Calm => tr!("breathing.ex_calm"),
        }
    }

    fn phases(self) -> &'static [(Phase, f32)] {
        match self {
            Exercise::Box => &[(Phase::In, 4.0), (Phase::HoldFull, 4.0), (Phase::Out, 4.0), (Phase::HoldEmpty, 4.0)],
            Exercise::FourSevenEight => &[(Phase::In, 4.0), (Phase::HoldFull, 7.0), (Phase::Out, 8.0)],
            Exercise::FiveSevenEight => &[(Phase::In, 5.0), (Phase::HoldFull, 7.0), (Phase::Out, 8.0)],
            Exercise::Calm => &[(Phase::In, 4.0), (Phase::Out, 6.0)],
        }
    }

    fn round_seconds(self) -> f32 {
        self.phases().iter().map(|(_, s)| s).sum()
    }

    /// Antal hele runder, der fylder omtrent `minutes` minutter (mindst én).
    pub fn rounds_for(self, minutes: u32) -> u32 {
        ((minutes as f32 * 60.0 / self.round_seconds()).round() as u32).max(1)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Phase {
    In,
    HoldFull,
    Out,
    HoldEmpty,
}

impl Phase {
    fn icon(self) -> &'static str {
        match self {
            Phase::In => "🌸",
            Phase::HoldFull | Phase::HoldEmpty => "⏸",
            Phase::Out => "🕯",
        }
    }

    fn text(self) -> String {
        match self {
            Phase::In => tr!("breathing.in"),
            Phase::HoldFull => tr!("breathing.hold_full"),
            Phase::Out => tr!("breathing.out"),
            Phase::HoldEmpty => tr!("breathing.hold_empty"),
        }
    }

    fn hint(self) -> String {
        match self {
            Phase::In => tr!("breathing.in_hint"),
            Phase::HoldFull => tr!("breathing.hold_full_hint"),
            Phase::Out => tr!("breathing.out_hint"),
            Phase::HoldEmpty => tr!("breathing.hold_empty_hint"),
        }
    }

    fn color(self) -> Color32 {
        match self {
            Phase::In => ACCENT,
            Phase::HoldFull | Phase::HoldEmpty => Color32::from_rgb(240, 170, 60),
            Phase::Out => Color32::from_rgb(90, 190, 130),
        }
    }
}

/// Sekunder til at gøre sig klar, før første runde.
const PREPARE: f32 = 3.0;

#[derive(Debug, PartialEq)]
enum Step {
    Prepare { left: f32 },
    Phase { round: u32, index: usize, phase: Phase, progress: f32, left: f32 },
    Done,
}

/// Hvor i øvelsen man er, `elapsed` sekunder efter start.
fn step_at(exercise: Exercise, rounds: u32, elapsed: f32) -> Step {
    if elapsed < PREPARE {
        return Step::Prepare { left: PREPARE - elapsed };
    }
    let t = elapsed - PREPARE;
    let round_len = exercise.round_seconds();
    let round = (t / round_len) as u32;
    if round >= rounds {
        return Step::Done;
    }
    let mut in_round = t - round as f32 * round_len;
    for (index, &(phase, secs)) in exercise.phases().iter().enumerate() {
        if in_round < secs {
            return Step::Phase { round: round + 1, index, phase, progress: in_round / secs, left: secs - in_round };
        }
        in_round -= secs;
    }
    Step::Done
}

pub(super) struct Breathing {
    exercise: Exercise,
    rounds: u32,
    started: Instant,
}

impl Breathing {
    pub(super) fn new(exercise: Exercise, minutes: u32) -> Self {
        Self { exercise, rounds: exercise.rounds_for(minutes), started: Instant::now() }
    }
}

impl TimeSnapperApp {
    pub(super) fn start_breathing(&mut self) {
        let settings = &self.data.settings;
        self.breathing = Some(Breathing::new(settings.breathing_exercise, settings.breathing_minutes));
    }

    /// Viser øvelsesvinduet, mens en øvelse er i gang.
    pub(super) fn breathing_window(&mut self, ctx: &egui::Context) {
        let Some(breathing) = &self.breathing else { return };
        let step = step_at(breathing.exercise, breathing.rounds, breathing.started.elapsed().as_secs_f32());
        let (rounds, exercise) = (breathing.rounds, breathing.exercise);
        let mut close = false;
        let mut again = false;
        // Skift øvelse i vinduet; gælder kun denne gang (standarden står i indstillingerne).
        let mut chosen = exercise;

        let builder = egui::ViewportBuilder::default()
            .with_title(tr!("breathing.window_title"))
            .with_inner_size([300.0, 410.0])
            .with_resizable(false)
            .with_always_on_top();
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("breathing"), builder, |ui, _class| {
            close |= ui.input(|i| i.viewport().close_requested());
            egui::CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    egui::ComboBox::from_id_salt("breathing_choice")
                        .width(200.0)
                        .selected_text(chosen.label())
                        .show_ui(ui, |ui| {
                            for option in Exercise::ALL {
                                ui.selectable_value(&mut chosen, option, option.label());
                            }
                        })
                        .response
                        .on_hover_text(tr!("breathing.switch_hint"));
                });
                ui.add_space(4.0);
                ui.vertical_centered(|ui| match &step {
                    Step::Prepare { left } => {
                        ui.add_space(30.0);
                        ui.heading(tr!("breathing.get_ready"));
                        ui.label(RichText::new(tr!("breathing.get_ready_hint")).weak());
                        draw_box(ui, exercise, None);
                        ui.label(RichText::new(format!("{}", left.ceil() as u32)).size(28.0).strong());
                    }
                    Step::Phase { round, index, phase, progress, left } => {
                        ui.label(RichText::new(tr!("breathing.round", round = round, rounds = rounds)).weak());
                        draw_box(ui, exercise, Some((*index, *phase, *progress, *left)));
                        ui.label(RichText::new(format!("{} {}", phase.icon(), phase.text())).size(17.0).strong());
                        ui.label(RichText::new(phase.hint()).italics().weak());
                    }
                    Step::Done => {
                        ui.add_space(70.0);
                        ui.label(RichText::new("🌟").size(48.0).color(Color32::from_rgb(240, 190, 80)));
                        ui.heading(tr!("breathing.done"));
                        ui.label(RichText::new(tr!("breathing.done_hint")).weak());
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            ui.add_space((ui.available_width() - 150.0) / 2.0);
                            again |= ui.button(tr!("breathing.again")).clicked();
                            close |= ui.button(tr!("common.close")).clicked();
                        });
                    }
                });
                if step != Step::Done {
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                        close |= ui.button(tr!("breathing.stop")).on_hover_text(tr!("breathing.stop_hint")).clicked();
                    });
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                close = true;
            }
        });

        let minutes = self.data.settings.breathing_minutes;
        if chosen != exercise || again {
            self.breathing = Some(Breathing::new(chosen, minutes));
        } else if close {
            self.breathing = None;
        } else if step != Step::Done {
            // Jævn animation, mens øvelsen kører.
            ctx.request_repaint();
        }
    }
}

/// Figurens hjørner i en enhedsboks (0..1). Hjørne i er hvor fase i starter;
/// siden fra hjørne i til i+1 hører til fase i. Skærmens y vender nedad.
///
/// Ind går lodret op og hold vandret (fuld: mod højre, tom: mod venstre), med
/// længder efter varigheden. Ud går ned igen og tager den vandrette rest med,
/// så figuren lukker; den bliver altså skrå, men går altid nedad.
fn figure(phases: &[(Phase, f32)]) -> Vec<Pos2> {
    let total_in: f32 = phases.iter().filter(|(p, _)| *p == Phase::In).map(|(_, s)| s).sum();
    let total_out: f32 = phases.iter().filter(|(p, _)| *p == Phase::Out).map(|(_, s)| s).sum();
    let holds_x: f32 = phases
        .iter()
        .map(|&(p, s)| match p {
            Phase::HoldFull => s,
            Phase::HoldEmpty => -s,
            _ => 0.0,
        })
        .sum();
    let steps: Vec<Vec2> = phases
        .iter()
        .map(|&(phase, secs)| match phase {
            Phase::In => Vec2::new(0.0, -secs),
            Phase::HoldFull => Vec2::new(secs, 0.0),
            Phase::HoldEmpty => Vec2::new(-secs, 0.0),
            // Ned med samme højde som ind, og tilbage med resten af vejen.
            Phase::Out if total_out > 0.0 => Vec2::new(-holds_x, total_in) * (secs / total_out),
            Phase::Out => Vec2::ZERO,
        })
        .collect();

    let mut points = Vec::with_capacity(steps.len());
    let mut at = Vec2::ZERO;
    for step in &steps {
        points.push(at);
        at += *step;
    }
    let (min, max) = points
        .iter()
        .fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
    let size = max - min;
    let scale = 1.0 / size.x.max(size.y).max(f32::EPSILON);
    let offset = (Vec2::splat(1.0) - size * scale) / 2.0;
    points.iter().map(|p| ((*p - min) * scale + offset).to_pos2()).collect()
}

/// Siderne som linjestykker med runde hjørner. Side i starter midt i hjørne
/// i's bue og slutter midt i hjørne i+1's bue, så hvert hjørne deles mellem de
/// to sider (og deres farver). `radius` 0 giver skarpe hjørner.
fn rounded_edges(corners: &[Pos2], radius: f32) -> Vec<Vec<Pos2>> {
    let n = corners.len();
    // Hvert hjørnes bue (en kvadratisk Bézier-kurve) delt i to halvdele.
    let halves: Vec<(Vec<Pos2>, Vec<Pos2>)> = (0..n)
        .map(|k| {
            let (prev, corner, next) = (corners[(k + n - 1) % n], corners[k], corners[(k + 1) % n]);
            let (to_prev, to_next) = (prev - corner, next - corner);
            let r = radius.min(to_prev.length() * 0.45).min(to_next.length() * 0.45);
            let (start, end) = (corner + to_prev.normalized() * r, corner + to_next.normalized() * r);
            let curve: Vec<Pos2> = (0..=8)
                .map(|j| {
                    let t = j as f32 / 8.0;
                    let point = start.to_vec2() * (1.0 - t).powi(2)
                        + corner.to_vec2() * (2.0 * (1.0 - t) * t)
                        + end.to_vec2() * t.powi(2);
                    point.to_pos2()
                })
                .collect();
            (curve[..=4].to_vec(), curve[4..].to_vec())
        })
        .collect();
    (0..n).map(|i| [halves[i].1.clone(), halves[(i + 1) % n].0.clone()].concat()).collect()
}

/// Stykket af en linje fra starten til `fraction` af dens længde, og punktet dér.
fn along(points: &[Pos2], fraction: f32) -> (Vec<Pos2>, Pos2) {
    let total: f32 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    let mut left = total * fraction.clamp(0.0, 1.0);
    let mut walked = vec![points[0]];
    for w in points.windows(2) {
        let step = (w[1] - w[0]).length();
        if left <= step && step > 0.0 {
            let dot = w[0] + (w[1] - w[0]) * (left / step);
            walked.push(dot);
            return (walked, dot);
        }
        left -= step;
        walked.push(w[1]);
    }
    let end = *points.last().unwrap_or(&points[0]);
    (walked, end)
}

/// Figuren med prikken, der følger siderne, og cirklen der "trækker vejret".
/// `None` er gør-klar-trinnet (cirklen lille, prikken i startpunktet).
fn draw_box(ui: &mut egui::Ui, exercise: Exercise, at: Option<(usize, Phase, f32, f32)>) {
    let size = 190.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size + 30.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let area = egui::Rect::from_center_size(rect.center(), Vec2::splat(size));
    let phases = exercise.phases();
    let unit = figure(phases);
    // Uden hold er figuren en lodret linje; den står i venstre side, så cirklen
    // kan være i midten.
    let is_line = unit.iter().all(|p| (p.x - unit[0].x).abs() < 1e-4);
    // Linjen står i venstre side, så cirklen kan være i midten.
    let place = |p: Pos2| {
        if is_line {
            Pos2::new(area.left(), area.top() + p.y * size)
        } else {
            area.min + p.to_vec2() * size
        }
    };
    let corners: Vec<Pos2> = unit.into_iter().map(place).collect();
    let n = corners.len();
    // Hver side som en linje med halvdelen af de runde hjørner i hver ende.
    let edges = rounded_edges(&corners, if is_line { 0.0 } else { 12.0 });
    let track = ui.visuals().widgets.inactive.bg_fill;
    let width = 4.0;
    let line = |points: Vec<Pos2>, color: Color32| egui::Shape::line(points, Stroke::new(width, color));

    // Hele figuren svagt i fasernes farver, så rytmen kan ses.
    for (points, (phase, _)) in edges.iter().zip(phases) {
        painter.add(line(points.clone(), track));
        painter.add(line(points.clone(), phase.color().gamma_multiply(0.15)));
    }

    let (index, phase, progress, left) = at.unwrap_or((0, Phase::In, 0.0, 0.0));
    let eased = (1.0 - (progress * PI).cos()) / 2.0;
    let color = phase.color();
    if at.is_some() {
        for (points, (done, _)) in edges.iter().zip(phases).take(index) {
            painter.add(line(points.clone(), done.color().gamma_multiply(0.5)));
        }
        let (walked, dot) = along(&edges[index], progress);
        painter.add(line(walked, color));
        painter.circle_filled(dot, 8.0, Color32::WHITE);
    } else {
        painter.circle_filled(edges[0][0], 8.0, Color32::WHITE);
    }

    // Cirklen: lille når lungerne er tomme, stor når de er fulde. I en trekant
    // står den i den indskrevne cirkels centrum og holder sig inden for siderne.
    let (center, small, big) = if n == 3 {
        let (a, b, c) = (corners[0], corners[1], corners[2]);
        let (la, lb, lc) = ((b - c).length(), (c - a).length(), (a - b).length());
        let perimeter = la + lb + lc;
        let center = ((a.to_vec2() * la + b.to_vec2() * lb + c.to_vec2() * lc) / perimeter).to_pos2();
        let twice_area = ((b - a).x * (c - a).y - (b - a).y * (c - a).x).abs();
        let inradius = twice_area / perimeter;
        (center, inradius * 0.35, inradius * 0.85)
    } else if is_line {
        (area.center(), 28.0, 78.0)
    } else {
        let sum = corners.iter().fold(Vec2::ZERO, |sum, p| sum + p.to_vec2());
        ((sum / n as f32).to_pos2(), 28.0, 78.0)
    };
    let radius = match phase {
        _ if at.is_none() => small,
        Phase::In => small + (big - small) * eased,
        Phase::HoldFull => big,
        Phase::Out => big - (big - small) * eased,
        Phase::HoldEmpty => small,
    };
    painter.circle_filled(center, radius, color.gamma_multiply(0.25));
    painter.circle_stroke(center, radius, Stroke::new(2.0, color));
    if at.is_some() {
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            format!("{}", left.ceil() as u32),
            egui::FontId::proportional(big.clamp(18.0, 78.0) * 0.44),
            ui.visuals().strong_text_color(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_breathing_rounds_and_phases() {
        assert_eq!(Exercise::Box.rounds_for(1), 4, "1 min ≈ 4 runder à 16 s");
        assert_eq!(Exercise::Box.rounds_for(2), 8);

        assert!(matches!(step_at(Exercise::Box, 4, 1.0), Step::Prepare { .. }));
        let at = |t| step_at(Exercise::Box, 4, PREPARE + t);
        assert!(matches!(at(0.5), Step::Phase { round: 1, phase: Phase::In, .. }));
        assert!(matches!(at(5.0), Step::Phase { round: 1, phase: Phase::HoldFull, .. }));
        assert!(matches!(at(9.0), Step::Phase { round: 1, phase: Phase::Out, .. }));
        assert!(matches!(at(13.0), Step::Phase { round: 1, phase: Phase::HoldEmpty, .. }));
        assert!(matches!(at(16.5), Step::Phase { round: 2, phase: Phase::In, .. }));
        assert!(matches!(at(63.9), Step::Phase { round: 4, phase: Phase::HoldEmpty, .. }));
        assert_eq!(at(64.0), Step::Done);

        let Step::Phase { left, progress, .. } = at(1.0) else { panic!() };
        assert!((left - 3.0).abs() < 1e-4 && (progress - 0.25).abs() < 1e-4);

        assert_eq!(Exercise::FourSevenEight.rounds_for(2), 6, "19 s pr. runde");
        assert_eq!(Exercise::Calm.rounds_for(1), 6, "10 s pr. runde");
    }

    fn close(a: Pos2, b: Pos2) -> bool {
        (a - b).length() < 1e-3
    }

    #[test]
    fn box_breathing_is_the_square() {
        let p = figure(Exercise::Box.phases());
        assert!(close(p[0], Pos2::new(0.0, 1.0)), "start nederst til venstre: {p:?}");
        assert!(close(p[1], Pos2::new(0.0, 0.0)), "op langs venstre side");
        assert!(close(p[2], Pos2::new(1.0, 0.0)), "hen over toppen");
        assert!(close(p[3], Pos2::new(1.0, 1.0)), "ned langs højre side");
    }

    #[test]
    fn directions_show_what_to_do() {
        for exercise in [Exercise::FourSevenEight, Exercise::FiveSevenEight] {
            let p = figure(exercise.phases());
            let phases = exercise.phases();
            assert_eq!(p.len(), 3);
            for (i, (phase, _)) in phases.iter().enumerate() {
                let step = p[(i + 1) % p.len()] - p[i];
                match phase {
                    Phase::In => assert!(step.y < 0.0 && step.x.abs() < 1e-4, "{exercise:?}: ind lodret op"),
                    Phase::HoldFull => assert!(step.x > 0.0 && step.y.abs() < 1e-4, "{exercise:?}: hold vandret"),
                    Phase::Out => assert!(step.y > 0.0, "{exercise:?}: ud går nedad"),
                    Phase::HoldEmpty => unreachable!(),
                }
            }
            // Ind og hold i forholdet mellem varighederne.
            let (up, across) = ((p[1] - p[0]).length(), (p[2] - p[1]).length());
            assert!((up / across - phases[0].1 / phases[1].1).abs() < 1e-3);
        }
    }

    #[test]
    fn hold_with_empty_lungs_runs_along_the_bottom() {
        let p = figure(&[(Phase::In, 4.0), (Phase::Out, 4.0), (Phase::HoldEmpty, 4.0)]);
        let out = p[2] - p[1];
        let hold = p[0] - p[2];
        assert!(out.y > 0.0, "ud går nedad");
        assert!(hold.x < 0.0 && hold.y.abs() < 1e-4, "hold går vandret tilbage langs bunden");
    }

    #[test]
    fn rounded_edges_join_up_and_dot_walks_along() {
        let square = [Pos2::new(0.0, 100.0), Pos2::new(0.0, 0.0), Pos2::new(100.0, 0.0), Pos2::new(100.0, 100.0)];
        let edges = rounded_edges(&square, 12.0);
        for i in 0..4 {
            let (end, next_start) = (*edges[i].last().unwrap(), edges[(i + 1) % 4][0]);
            assert!((end - next_start).length() < 1e-3, "siderne hænger sammen i hjørnerne");
        }
        // Midt på venstre side ligger prikken på den lige linje.
        let (_, dot) = along(&edges[0], 0.5);
        assert!(dot.x.abs() < 1e-3 && (dot.y - 50.0).abs() < 1.0, "{dot:?}");
        // Skarpe hjørner med radius 0.
        assert_eq!(rounded_edges(&square, 0.0)[0][0], square[0]);
    }

    #[test]
    fn in_and_out_without_holds_is_a_vertical_line() {
        let p = figure(Exercise::Calm.phases());
        assert!(close(p[0], Pos2::new(0.5, 1.0)) && close(p[1], Pos2::new(0.5, 0.0)), "{p:?}");
    }
}
