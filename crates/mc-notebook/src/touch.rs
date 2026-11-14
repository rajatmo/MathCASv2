//! A finger on a touch screen (Android): it scrolls, taps and, after a pause, drags.
//!
//! The window system reports a finger as a pointer whose button goes down where the finger lands. With a mouse that is
//! right: a drag selects text, moves cells, turns a 3D picture, and the wheel scrolls. A finger has no wheel, so the
//! events of a touch are rewritten before egui sees them ([`Touch::rewrite`], from the app's raw-input hook):
//!
//! - a finger that **moves** scrolls what is under it (the notebook; sideways the menus and the toolbar of a narrow
//!   window): its travel becomes wheel events, and the button never goes down;
//! - a finger lifted **without moving** is a click where it landed (the button goes down in that frame and up in
//!   the next, as a mouse click takes two);
//! - a finger that **rests** for [`HOLD`] seconds and then moves drags as the mouse does: from then on its events
//!   pass unchanged.
//!
//! A mouse or a pen is not a touch and is left alone.
use eframe::egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, PointerButton, Pos2, TouchPhase};

/// How far (points) a finger may move and still tap or rest.
const SLOP: f32 = 8.0;
/// How long (seconds) a finger rests before it drags.
pub const HOLD: f64 = 0.45;

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    /// no finger down
    Idle,
    /// a finger landed at `at` (time `since`) and has not moved yet; the button is not down for egui
    Landed { at: Pos2, since: f64, modifiers: Modifiers },
    /// the finger scrolls; it was last at `last`
    Scrolling { last: Pos2 },
    /// the finger drags as a mouse: events pass
    Dragging,
}

/// The state of the touch between frames.
#[derive(Debug)]
pub struct Touch {
    state: State,
    /// events for the next frame (the release of a tap)
    later: Vec<Event>,
}

impl Default for Touch {
    fn default() -> Self {
        Touch { state: State::Idle, later: vec![] }
    }
}

impl Touch {
    /// Rewrite the frame's events at time `now` (seconds). Returns after how many seconds another frame is needed
    /// although nothing may happen (the release of a tap: `0`; a resting finger: when it starts to drag).
    pub fn rewrite(&mut self, events: &mut Vec<Event>, now: f64) -> Option<f64> {
        let touched = events.iter().any(|e| matches!(e, Event::Touch { .. }));
        let mut out: Vec<Event> = std::mem::take(&mut self.later);
        if self.state == State::Idle && !touched {
            // a mouse, a pen, keys: unchanged (after what was kept for this frame)
            out.append(events);
            *events = out;
            return None;
        }
        for e in events.drain(..) {
            match e {
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers }
                    if touched && self.state == State::Idle =>
                {
                    self.state = State::Landed { at: pos, since: now, modifiers };
                }
                Event::PointerMoved(pos) => {
                    match self.state {
                        State::Landed { at, .. } if (pos - at).length() > SLOP => {
                            out.push(wheel(pos - at));
                            self.state = State::Scrolling { last: pos };
                        }
                        State::Scrolling { last } => {
                            out.push(wheel(pos - last));
                            self.state = State::Scrolling { last: pos };
                        }
                        _ => {}
                    }
                    out.push(Event::PointerMoved(pos));
                }
                Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers } => {
                    match self.state {
                        // a tap: down now, up in the next frame
                        State::Landed { at, modifiers: held, .. } => {
                            out.push(Event::PointerMoved(at));
                            out.push(press(at, held));
                            self.later.push(Event::PointerButton {
                                pos: at,
                                button: PointerButton::Primary,
                                pressed: false,
                                modifiers,
                            });
                        }
                        State::Scrolling { .. } => {}
                        State::Dragging | State::Idle => {
                            out.push(Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers })
                        }
                    }
                    self.state = State::Idle;
                }
                // the finger is gone: after the release of a tap, not before it
                Event::PointerGone if !self.later.is_empty() => self.later.push(Event::PointerGone),
                Event::Touch { phase: TouchPhase::Cancel, .. } => {
                    if self.state == State::Dragging {
                        out.push(e);
                    }
                    self.state = State::Idle;
                }
                other => out.push(other),
            }
        }
        let mut wake = (!self.later.is_empty()).then_some(0.0);
        if let State::Landed { at, since, modifiers } = self.state {
            if now - since >= HOLD {
                out.push(Event::PointerMoved(at));
                out.push(press(at, modifiers));
                self.state = State::Dragging;
            } else {
                wake = Some(wake.map_or(HOLD - (now - since), |w: f64| w.min(HOLD - (now - since))));
            }
        }
        *events = out;
        wake
    }
}

fn press(pos: Pos2, modifiers: Modifiers) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers }
}

/// The content moves with the finger.
fn wheel(delta: eframe::egui::Vec2) -> Event {
    Event::MouseWheel {
        unit: MouseWheelUnit::Point,
        delta,
        phase: TouchPhase::Move,
        source: MouseWheelSource::Unknown,
        modifiers: Modifiers::NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{pos2, vec2, TouchDeviceId, TouchId};

    fn touch(phase: TouchPhase, pos: Pos2) -> Event {
        Event::Touch { device_id: TouchDeviceId(0), id: TouchId(0), phase, pos, force: None }
    }

    fn down(pos: Pos2) -> Vec<Event> {
        vec![touch(TouchPhase::Start, pos), Event::PointerMoved(pos), press(pos, Modifiers::NONE)]
    }

    fn moved(pos: Pos2) -> Vec<Event> {
        vec![touch(TouchPhase::Move, pos), Event::PointerMoved(pos)]
    }

    fn up(pos: Pos2) -> Vec<Event> {
        let release = Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE };
        vec![touch(TouchPhase::End, pos), release, Event::PointerGone]
    }

    fn presses(events: &[Event]) -> Vec<bool> {
        events.iter().filter_map(|e| if let Event::PointerButton { pressed, .. } = e { Some(*pressed) } else { None }).collect()
    }

    fn wheels(events: &[Event]) -> Vec<eframe::egui::Vec2> {
        events.iter().filter_map(|e| if let Event::MouseWheel { delta, .. } = e { Some(*delta) } else { None }).collect()
    }

    #[test]
    fn a_tap_is_a_click_over_two_frames() {
        let mut t = Touch::default();
        let mut e = down(pos2(50.0, 60.0));
        assert!(t.rewrite(&mut e, 0.0).is_some());
        assert!(presses(&e).is_empty(), "the button is not down while the finger may still scroll");
        let mut e = up(pos2(52.0, 61.0));
        assert_eq!(t.rewrite(&mut e, 0.1), Some(0.0));
        assert_eq!(presses(&e), vec![true]);
        assert!(!e.contains(&Event::PointerGone));
        let mut e = vec![];
        assert_eq!(t.rewrite(&mut e, 0.12), None);
        assert_eq!(presses(&e), vec![false]);
        assert_eq!(e.last(), Some(&Event::PointerGone));
    }

    #[test]
    fn a_moving_finger_scrolls_and_never_clicks() {
        let mut t = Touch::default();
        let mut all = vec![];
        for (k, mut e) in [down(pos2(50.0, 300.0)), moved(pos2(50.0, 280.0)), moved(pos2(53.0, 250.0)), up(pos2(53.0, 250.0))]
            .into_iter()
            .enumerate()
        {
            t.rewrite(&mut e, k as f64 * 0.016);
            all.extend(e);
        }
        assert!(presses(&all).is_empty());
        assert_eq!(wheels(&all), vec![vec2(0.0, -20.0), vec2(3.0, -30.0)]);
        let mut e = vec![];
        assert_eq!(t.rewrite(&mut e, 1.0), None);
        assert!(e.is_empty());
    }

    #[test]
    fn a_resting_finger_drags() {
        let mut t = Touch::default();
        let mut e = down(pos2(10.0, 10.0));
        let wake = t.rewrite(&mut e, 0.0).unwrap();
        assert!((wake - HOLD).abs() < 1e-9);
        let mut e = vec![];
        t.rewrite(&mut e, HOLD + 0.01);
        assert_eq!(presses(&e), vec![true]);
        let mut e = moved(pos2(10.0, 80.0));
        t.rewrite(&mut e, 0.6);
        assert!(wheels(&e).is_empty());
        assert!(e.contains(&Event::PointerMoved(pos2(10.0, 80.0))));
        let mut e = up(pos2(10.0, 80.0));
        t.rewrite(&mut e, 0.7);
        assert_eq!(presses(&e), vec![false]);
    }

    #[test]
    fn a_mouse_is_left_alone() {
        let mut t = Touch::default();
        let click = vec![Event::PointerMoved(pos2(5.0, 5.0)), press(pos2(5.0, 5.0), Modifiers::NONE)];
        let mut e = click.clone();
        assert_eq!(t.rewrite(&mut e, 0.0), None);
        assert_eq!(e, click);
    }
}
