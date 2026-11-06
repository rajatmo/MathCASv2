//! Magnification by the wheel with Ctrl and by keys (user request, round 13: "ctrl+scroll is chaotic").
//!
//! The magnification moves along one fixed ladder, 50 % to 300 % in steps of 10 %. Ctrl+wheel events are taken out
//! of egui's input before egui sees them (egui smooths wheel deltas over several frames, so one notch gave one or
//! two steps at uneven times, and a smoothed remainder still arriving after Ctrl was released scrolled the page);
//! their deltas are summed here into whole steps: a mouse notch is one step, a precision touchpad's small deltas add
//! up to steps, a pinch (`Zoom` event) adds its logarithm in steps of 10 %. All the steps of one frame are applied
//! together, once, by the caller; the point under the pointer is kept in place by the canvas.
use eframe::egui::{Event, MouseWheelUnit};

/// The magnification ladder: 50 % to 300 % in steps of 10 %.
pub const LADDER: [f32; 26] = [
    0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8, 1.9, 2.0, 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7,
    2.8, 2.9, 3.0,
];

/// Points of touchpad scrolling that make one step.
const POINTS_PER_STEP: f32 = 50.0;
/// A pause after which a partial step is forgotten (seconds).
const FORGET: f64 = 0.6;

/// The magnification `steps` rungs from `m` on the ladder (`m` first snapped to its nearest rung); `steps` 0 gives
/// 100 %.
pub fn step(m: f32, steps: i32) -> f32 {
    if steps == 0 {
        return 1.0;
    }
    let at = LADDER
        .iter()
        .enumerate()
        .min_by(|a, b| (a.1 - m).abs().total_cmp(&(b.1 - m).abs()))
        .map_or(5, |(i, _)| i) as i32;
    LADDER[(at + steps).clamp(0, LADDER.len() as i32 - 1) as usize]
}

/// Ctrl+wheel input summed into whole magnification steps.
#[derive(Debug, Default)]
pub struct WheelZoom {
    /// the part of a step not yet taken, in steps (positive: larger)
    partial: f32,
    /// when the last delta came
    last: f64,
}

impl WheelZoom {
    /// Take the Ctrl+wheel and pinch events out of `events` and return the whole steps they make (positive:
    /// larger). Events without Ctrl stay for egui (scrolling).
    pub fn take(&mut self, events: &mut Vec<Event>, time: f64) -> i32 {
        let mut add = 0.0f32;
        let mut any = false;
        events.retain(|e| match e {
            Event::MouseWheel { unit, delta, modifiers, .. } if modifiers.ctrl || modifiers.command => {
                // up (content moving down) is larger
                let d = delta.y + delta.x;
                add += match unit {
                    MouseWheelUnit::Line => d,
                    MouseWheelUnit::Page => d,
                    MouseWheelUnit::Point => d / POINTS_PER_STEP,
                };
                any = true;
                false
            }
            Event::Zoom(f) if f.is_finite() && *f > 0.0 => {
                add += f.ln() / 1.1f32.ln();
                any = true;
                false
            }
            _ => true,
        });
        if !any {
            return 0;
        }
        self.steps(add, time)
    }

    /// Add `d` steps' worth of input at `time` and return the whole steps now complete.
    pub fn steps(&mut self, d: f32, time: f64) -> i32 {
        if !d.is_finite() {
            return 0;
        }
        // a partial step from a while ago, or one in the other direction, is forgotten
        if time - self.last > FORGET || self.partial * d < 0.0 {
            self.partial = 0.0;
        }
        self.last = time;
        self.partial += d;
        // a step is complete at 1.0; a touchpad's tiny remainder (0.98) still counts as one
        let whole = (self.partial + self.partial.signum() * 0.02).trunc();
        self.partial -= whole;
        if whole.abs() < 1.0 {
            return 0;
        }
        whole.clamp(-25.0, 25.0) as i32
    }
}

/// Where the canvas keeps the point under the pointer while the magnification changes: the cell under it, the
/// fraction of the cell's height, and the pointer's distance from the top of the canvas.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    /// the cell's index
    pub cell: usize,
    /// the fraction of the cell's height above the point
    pub frac: f32,
    /// the pointer's distance from the canvas's top edge
    pub screen: f32,
    /// until when the anchor is held (seconds): cells measured after the change still move what is above
    pub until: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui;

    /// A Ctrl+wheel event of `y` in `unit`s (positive: away from the user).
    fn wheel(unit: MouseWheelUnit, y: f32) -> Event {
        Event::MouseWheel {
            unit,
            delta: egui::vec2(0.0, y),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::CTRL,
        }
    }

    #[test]
    fn the_ladder_moves_one_rung_per_step_and_stops_at_its_ends() {
        assert_eq!(step(1.0, 1), 1.1);
        assert_eq!(step(1.0, -1), 0.9);
        assert_eq!(step(1.26, 1), 1.4, "an off-ladder value snaps to its nearest rung first");
        assert_eq!(step(2.9, 5), 3.0);
        assert_eq!(step(0.5, -3), 0.5);
        assert_eq!(step(2.2, 0), 1.0, "Ctrl+0 resets");
    }

    #[test]
    fn a_notch_is_one_step_and_a_burst_is_its_notch_count() {
        let mut z = WheelZoom::default();
        let mut ev = vec![wheel(MouseWheelUnit::Line, 1.0)];
        assert_eq!(z.take(&mut ev, 0.0), 1);
        assert!(ev.is_empty(), "the event is taken from egui");
        // a burst of five notches in one frame: five steps at once, applied once
        let mut ev: Vec<Event> = (0..5).map(|_| wheel(MouseWheelUnit::Line, -1.0)).collect();
        assert_eq!(z.take(&mut ev, 0.1), -5);
        // a wheel without Ctrl stays for scrolling
        let mut ev = vec![Event::MouseWheel {
            unit: MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 1.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        }];
        assert_eq!(z.take(&mut ev, 0.2), 0);
        assert_eq!(ev.len(), 1);
    }

    #[test]
    fn touchpad_deltas_add_up_to_whole_steps() {
        let mut z = WheelZoom::default();
        let mut total = 0;
        // 120 small deltas of 2.5 points over one second: 300 points, six steps, never more than one at a time
        for k in 0..120 {
            let mut ev = vec![wheel(MouseWheelUnit::Point, 2.5)];
            let s = z.take(&mut ev, k as f64 / 120.0);
            assert!((0..=1).contains(&s));
            total += s;
        }
        assert_eq!(total, 6);
        // fractional lines (a precision touchpad on Windows) likewise
        let mut z = WheelZoom::default();
        let total: i32 = (0..10).map(|k| z.take(&mut vec![wheel(MouseWheelUnit::Line, 0.3)], k as f64 * 0.01)).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn a_partial_step_is_forgotten_after_a_pause_or_a_reversal() {
        let mut z = WheelZoom::default();
        assert_eq!(z.steps(0.6, 0.0), 0);
        assert_eq!(z.steps(0.6, 2.0), 0, "the first 0.6 was forgotten");
        assert_eq!(z.steps(-0.5, 2.1), 0, "a reversal drops the 0.6");
        assert_eq!(z.steps(-0.5, 2.2), -1);
        // a pinch of 1.21 is two steps of 10 %
        let mut ev = vec![Event::Zoom(1.21)];
        assert_eq!(z.take(&mut ev, 5.0), 2);
    }
}
