//! Fingers on a touch screen (Android): they scroll, tap, turn and zoom pictures and, after a pause, drag.
//!
//! The window system reports the first finger as a pointer whose button goes down where the finger lands. With a
//! mouse that is right: a drag selects text, moves cells, turns a 3D picture, and the wheel scrolls. A finger has no
//! wheel, so the events of a touch are rewritten before egui sees them ([`Touch::rewrite`], from the app's raw-input
//! hook). The fingers themselves are followed (their `Touch` events); the pointer events the window system made up
//! for the first one are dropped and made anew:
//!
//! - a finger that **moves** scrolls what is under it (the notebook; sideways the menus and the toolbar of a narrow
//!   window): its travel becomes wheel events, and the button never goes down;
//! - a finger that moves **on a picture that turns** (a 3D picture; the app tells where they are,
//!   [`Touch::set_pictures`]) drags as the mouse does: the button goes down where it landed, and the picture turns;
//! - a finger lifted **without moving** is a click where it landed (the button goes down in that frame and up in
//!   the next, as a mouse click takes two);
//! - a finger lifted **while it scrolls** lets the content run on and come to rest (a fling): wheel events follow
//!   in the next frames, each a little smaller, until the content is slow or a finger lands again;
//! - a finger that **rests** for [`HOLD`] seconds is a long press ([`Touch::take_long_press`] tells where), and when
//!   it then moves it drags as the mouse does;
//! - **two fingers** that move apart or together on a picture that turns zoom it ([`Touch::take_pinch`]); two
//!   fingers that move the same way scroll the page, on a picture too (so a picture as large as the screen cannot
//!   hold the page). When one of the two is lifted the gesture is over: the other does nothing until it is lifted.
//!
//! A mouse or a pen is not a touch and is left alone.
use eframe::egui::{Event, Modifiers, MouseWheelSource, MouseWheelUnit, PointerButton, Pos2, Rect, TouchId, TouchPhase, Vec2};

/// How far (points) a finger may move and still tap or rest.
const SLOP: f32 = 8.0;
/// How far (points) two fingers move apart or together before they zoom.
const PINCH_SLOP: f32 = 10.0;
/// How long (seconds) a finger rests before it drags.
pub const HOLD: f64 = 0.45;
/// The slowest (points per second) a lifted finger flings the content, and the speed at which a fling ends.
const FLING_FROM: f32 = 120.0;
const FLING_END: f32 = 24.0;
/// The fastest a fling starts (points per second).
const FLING_MAX: f32 = 7000.0;
/// The time (seconds) in which a fling loses all but 1/e of its speed.
const FLING_DECAY: f64 = 0.38;
/// A finger that stood still this long (seconds) before it was lifted does not fling.
const FLING_REST: f64 = 0.09;

/// What two fingers do.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pair {
    /// they have not moved far enough to tell
    Open,
    /// they zoom the picture
    Zoom,
    /// they scroll the page
    Pan,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    /// no finger down
    Idle,
    /// finger `id` landed at `at` (time `since`) and has not moved yet; the button is not down for egui. `grab`: it
    /// landed on a picture that turns
    Landed { id: TouchId, at: Pos2, since: f64, grab: bool },
    /// the finger scrolls; it was last at `last`. Its speed `v` (points per second) was last measured at time `t`,
    /// when it was at `from`
    Scrolling { id: TouchId, last: Pos2, from: Pos2, t: f64, v: Vec2 },
    /// the finger is lifted and the content runs on at `v` (points per second); `last` is the time of the last step
    Fling { v: Vec2, last: f64 },
    /// the finger drags as a mouse (the button is down): it landed at `from` and is at `at`. `turn`: it turns a
    /// picture
    Dragging { id: TouchId, from: Pos2, at: Pos2, turn: bool },
    /// fingers `a` and `b` are down: `dist` apart with `mid` between them when last looked at; `picture`: a point
    /// of the picture they are on
    Two { a: TouchId, b: TouchId, picture: Option<Pos2>, what: Pair, dist: f32, mid: Pos2 },
    /// a gesture is over and a finger is still down: nothing happens until all are lifted
    Spent,
}

/// Two fingers zoom a picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pinch {
    /// a point of the picture (where the first finger landed)
    pub at: Pos2,
    /// by how much the fingers moved apart since the last call (1: not at all)
    pub factor: f32,
    /// a finger was lifted: the zoom is kept
    pub ended: bool,
}

/// The state of the touch between frames.
#[derive(Debug)]
pub struct Touch {
    state: State,
    /// the fingers that are down and where
    fingers: Vec<(TouchId, Pos2)>,
    /// the finger whose `Touch` events egui sees (it never sees a second one: it has gestures of its own)
    shown: Option<TouchId>,
    /// the modifiers held when the button last went down or up
    modifiers: Modifiers,
    /// events for the next frame (the release of a tap)
    later: Vec<Event>,
    /// the button went down in this frame: what follows waits for the next one (a press and what comes after it in
    /// one frame would be a press where the pointer ends up)
    defer: bool,
    /// where a finger has rested for [`HOLD`] seconds, until it is asked for
    long_press: Option<Pos2>,
    /// the finger of a long press is still down: where it rests, and whether it has moved away since
    resting: Option<(Pos2, bool)>,
    /// where a finger of a long press was lifted without having moved, until it is asked for
    long_tap: Option<Pos2>,
    /// the zoom two fingers made, until it is asked for
    pinch: Option<Pinch>,
    /// where the pictures that turn are (screen rectangles of the last frame)
    pictures: Vec<Rect>,
    /// the last call made wheel events (a finger scrolls: the wheel is not a mouse's)
    scrolled: bool,
}

impl Default for Touch {
    fn default() -> Self {
        Touch {
            state: State::Idle,
            fingers: vec![],
            shown: None,
            modifiers: Modifiers::NONE,
            later: vec![],
            defer: false,
            long_press: None,
            resting: None,
            long_tap: None,
            pinch: None,
            pictures: vec![],
            scrolled: false,
        }
    }
}

impl Touch {
    /// Where a finger came to rest for [`HOLD`] seconds since the last call (a long press), if one did.
    pub fn take_long_press(&mut self) -> Option<Pos2> {
        self.long_press.take()
    }

    /// Where the finger of a long press was lifted since the last call without having moved (a long press that was
    /// not the start of a drag), if one was.
    pub fn take_long_tap(&mut self) -> Option<Pos2> {
        self.long_tap.take()
    }

    /// The zoom two fingers made on a picture since the last call, if they did.
    pub fn take_pinch(&mut self) -> Option<Pinch> {
        self.pinch.take()
    }

    /// Where the pictures that turn are on the screen: a finger that moves on one drags it.
    pub fn set_pictures(&mut self, pictures: Vec<Rect>) {
        self.pictures = pictures;
    }

    /// Whether the last [`Touch::rewrite`] made wheel events (a finger scrolls or the content runs on).
    pub fn scrolled(&self) -> bool {
        self.scrolled
    }

    fn on_picture(&self, at: Pos2) -> bool {
        self.pictures.iter().any(|r| r.contains(at))
    }

    fn finger(&self, id: TouchId) -> Option<Pos2> {
        self.fingers.iter().find(|f| f.0 == id).map(|f| f.1)
    }

    /// A pointer event for egui: in this frame, or in the next one when the button went down in this one.
    fn emit(&mut self, out: &mut Vec<Event>, e: Event) {
        if self.defer {
            self.later.push(e);
        } else {
            out.push(e);
        }
    }

    /// The button goes down at `at`.
    fn press(&mut self, out: &mut Vec<Event>, at: Pos2) {
        self.emit(out, Event::PointerMoved(at));
        self.emit(out, button(at, true, self.modifiers));
        self.defer = true;
    }

    fn wheel(&mut self, out: &mut Vec<Event>, delta: Vec2) {
        self.scrolled = true;
        out.push(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta,
            phase: TouchPhase::Move,
            source: MouseWheelSource::Unknown,
            modifiers: Modifiers::NONE,
        });
    }

    /// The zoom of a picture ends (a finger was lifted).
    fn end_pinch(&mut self) {
        if let State::Two { picture: Some(at), what: Pair::Zoom, .. } = self.state {
            let factor = self.pinch.map_or(1.0, |p| p.factor);
            self.pinch = Some(Pinch { at, factor, ended: true });
        }
    }

    /// Rewrite the frame's events at time `now` (seconds). Returns after how many seconds another frame is needed
    /// although nothing may happen (the release of a tap: `0`; a resting finger: when it starts to drag).
    pub fn rewrite(&mut self, events: &mut Vec<Event>, now: f64) -> Option<f64> {
        let touched = events.iter().any(|e| matches!(e, Event::Touch { .. }));
        let mut out: Vec<Event> = std::mem::take(&mut self.later);
        self.defer = false;
        self.scrolled = false;
        if self.state == State::Idle && !touched {
            // a mouse, a pen, keys: unchanged (after what was kept for this frame)
            out.append(events);
            *events = out;
            return None;
        }
        for e in events.drain(..) {
            match e {
                Event::Touch { id, phase, pos, .. } => {
                    // egui sees one finger; its pointer is made here
                    if phase == TouchPhase::Start && self.shown.is_none() {
                        self.shown = Some(id);
                    }
                    if self.shown == Some(id) {
                        out.push(e);
                        if matches!(phase, TouchPhase::End | TouchPhase::Cancel) {
                            self.shown = None;
                        }
                    }
                    match phase {
                        TouchPhase::Start => self.start(&mut out, id, pos, now),
                        TouchPhase::Move => self.moved(&mut out, id, pos, now),
                        TouchPhase::End => self.end(&mut out, id, pos, now),
                        TouchPhase::Cancel => self.cancel(&mut out),
                    }
                }
                // the pointer the window system made of the first finger
                Event::PointerButton { button: PointerButton::Primary, modifiers, .. } if touched => {
                    self.modifiers = modifiers;
                }
                Event::PointerMoved(_) | Event::PointerGone if touched => {}
                other => out.push(other),
            }
        }
        let mut wake = (!self.later.is_empty()).then_some(0.0);
        if let State::Landed { id, at, since, grab } = self.state {
            if now - since >= HOLD {
                self.defer = false;
                self.press(&mut out, at);
                self.state = State::Dragging { id, from: at, at, turn: grab };
                self.long_press = Some(at);
                self.resting = Some((at, false));
            } else {
                wake = Some(wake.map_or(HOLD - (now - since), |w: f64| w.min(HOLD - (now - since))));
            }
        }
        if let State::Fling { v, last } = self.state {
            // a frame that came late moves the content no further than a slow frame would
            let dt = (now - last).clamp(0.0, 0.05);
            let v = v * (-dt / FLING_DECAY).exp() as f32;
            if dt > 0.0 {
                self.wheel(&mut out, v * dt as f32);
            }
            if v.length() < FLING_END {
                out.push(Event::PointerGone);
                self.state = State::Idle;
            } else {
                self.state = State::Fling { v, last: now };
                wake = Some(0.0);
            }
        }
        *events = out;
        wake
    }

    /// Finger `id` lands at `pos`.
    fn start(&mut self, out: &mut Vec<Event>, id: TouchId, pos: Pos2, now: f64) {
        self.fingers.retain(|f| f.0 != id);
        self.fingers.push((id, pos));
        let two = |a: TouchId, from: Pos2, picture: Option<Pos2>, what: Pair| State::Two {
            a,
            b: id,
            picture,
            what,
            dist: (pos - from).length(),
            mid: from + (pos - from) * 0.5,
        };
        match self.state {
            // (on content that runs on: it stops)
            State::Idle | State::Fling { .. } => {
                self.emit(out, Event::PointerMoved(pos));
                self.state = State::Landed { id, at: pos, since: now, grab: self.on_picture(pos) };
            }
            // a second finger: on a picture the two zoom it, anywhere they scroll
            State::Landed { id: a, at, grab, .. } => {
                let picture = if grab {
                    Some(at)
                } else {
                    self.on_picture(pos).then_some(pos)
                };
                self.state = two(a, at, picture, Pair::Open);
            }
            State::Scrolling { id: a, last, .. } => self.state = two(a, last, None, Pair::Pan),
            // the picture has turned as far as the first finger took it
            State::Dragging { id: a, from, at, turn: true } => {
                self.resting = None;
                let up = button(at, false, self.modifiers);
                self.emit(out, up);
                self.state = two(a, at, Some(from), Pair::Open);
            }
            // a finger that drags, two fingers, a gesture that is over: one more finger changes nothing
            State::Dragging { .. } | State::Two { .. } | State::Spent => {}
        }
    }

    /// Finger `id` is at `pos`.
    fn moved(&mut self, out: &mut Vec<Event>, id: TouchId, pos: Pos2, now: f64) {
        let Some(f) = self.fingers.iter_mut().find(|f| f.0 == id) else { return };
        f.1 = pos;
        match self.state {
            State::Landed { id: a, at, since, grab } if a == id => {
                if (pos - at).length() <= SLOP {
                    self.emit(out, Event::PointerMoved(pos));
                } else if grab {
                    // the button goes down where the finger landed: the picture under that point turns
                    self.press(out, at);
                    self.emit(out, Event::PointerMoved(pos));
                    self.state = State::Dragging { id, from: at, at: pos, turn: true };
                } else {
                    self.wheel(out, pos - at);
                    self.emit(out, Event::PointerMoved(pos));
                    let v = if now > since { (pos - at) / (now - since) as f32 } else { Vec2::ZERO };
                    self.state = State::Scrolling { id, last: pos, from: pos, t: now, v };
                }
            }
            State::Scrolling { id: a, last, from, t, v } if a == id => {
                self.wheel(out, pos - last);
                self.emit(out, Event::PointerMoved(pos));
                // the speed over the last frames: a new measure counts for more than the old ones
                let (from, t, v) = if now - t >= 0.004 {
                    (pos, now, v * 0.35 + (pos - from) / (now - t) as f32 * 0.65)
                } else {
                    (from, t, v)
                };
                self.state = State::Scrolling { id, last: pos, from, t, v };
            }
            State::Dragging { id: a, from, turn, .. } if a == id => {
                if let Some((at, moved)) = &mut self.resting {
                    *moved |= (pos - *at).length() > SLOP;
                }
                self.emit(out, Event::PointerMoved(pos));
                self.state = State::Dragging { id, from, at: pos, turn };
            }
            State::Two { a, b, picture, what, dist, mid } if a == id || b == id => {
                let (Some(pa), Some(pb)) = (self.finger(a), self.finger(b)) else { return };
                let (d, m) = ((pb - pa).length(), pa + (pb - pa) * 0.5);
                let (mut dist, mut mid) = (dist, mid);
                let what = match (what, picture) {
                    // a zoom from where the fingers are now: what they moved before does not count
                    (Pair::Open, Some(_)) if (d - dist).abs() > PINCH_SLOP => {
                        dist = d;
                        Pair::Zoom
                    }
                    (Pair::Open, _) if (m - mid).length() > SLOP => Pair::Pan,
                    (other, _) => other,
                };
                match (what, picture) {
                    (Pair::Zoom, Some(at)) => {
                        if dist > 1.0 && d > 1.0 && d != dist {
                            let before = self.pinch.map_or(1.0, |p| p.factor);
                            self.pinch = Some(Pinch { at, factor: before * d / dist, ended: false });
                        }
                        dist = d;
                    }
                    (Pair::Pan, _) => {
                        self.wheel(out, m - mid);
                        mid = m;
                    }
                    _ => {}
                }
                self.state = State::Two { a, b, picture, what, dist, mid };
            }
            _ => {}
        }
    }

    /// Finger `id` is lifted at `pos`.
    fn end(&mut self, out: &mut Vec<Event>, id: TouchId, pos: Pos2, now: f64) {
        self.fingers.retain(|f| f.0 != id);
        let rest = if self.fingers.is_empty() { State::Idle } else { State::Spent };
        match self.state {
            // a tap: down now, up in the next frame
            State::Landed { id: a, at, .. } if a == id => {
                self.press(out, at);
                let up = button(at, false, self.modifiers);
                self.emit(out, up);
                self.emit(out, Event::PointerGone);
                self.state = rest;
            }
            // lifted while it moved: the content runs on (the pointer stays: the wheel turns what is under it)
            State::Scrolling { id: a, t, v, .. } if a == id => {
                if now - t <= FLING_REST && v.length() >= FLING_FROM {
                    let v = v * (FLING_MAX / v.length()).min(1.0);
                    self.state = State::Fling { v, last: now };
                } else {
                    self.emit(out, Event::PointerGone);
                    self.state = rest;
                }
            }
            State::Dragging { id: a, .. } if a == id => {
                let up = button(pos, false, self.modifiers);
                self.emit(out, up);
                self.emit(out, Event::PointerGone);
                self.state = rest;
                if let Some((at, false)) = self.resting.take() {
                    self.long_tap = Some(at);
                }
            }
            State::Two { a, b, .. } if a == id || b == id => {
                self.end_pinch();
                if rest == State::Idle {
                    self.emit(out, Event::PointerGone);
                }
                self.state = rest;
            }
            State::Spent if rest == State::Idle => {
                self.emit(out, Event::PointerGone);
                self.state = State::Idle;
            }
            _ => {}
        }
    }

    /// The system took the fingers away (another window came up, a gesture of its own began): whatever they did
    /// ends where it is, and the button is not left down.
    fn cancel(&mut self, out: &mut Vec<Event>) {
        if let State::Dragging { at, .. } = self.state {
            let up = button(at, false, self.modifiers);
            self.emit(out, up);
        }
        self.end_pinch();
        self.resting = None;
        if !matches!(self.state, State::Idle | State::Fling { .. }) {
            self.emit(out, Event::PointerGone);
            self.state = State::Idle;
        }
        self.fingers.clear();
    }
}

fn button(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{pos2, vec2, TouchDeviceId};

    fn touch_of(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
        Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos, force: None }
    }

    fn touch(phase: TouchPhase, pos: Pos2) -> Event {
        touch_of(0, phase, pos)
    }

    // the first finger, as the window system reports it: with a pointer made of it
    fn down(pos: Pos2) -> Vec<Event> {
        vec![touch(TouchPhase::Start, pos), Event::PointerMoved(pos), button(pos, true, Modifiers::NONE)]
    }

    fn moved(pos: Pos2) -> Vec<Event> {
        vec![touch(TouchPhase::Move, pos), Event::PointerMoved(pos)]
    }

    fn up(pos: Pos2) -> Vec<Event> {
        vec![touch(TouchPhase::End, pos), button(pos, false, Modifiers::NONE), Event::PointerGone]
    }

    // a second finger: no pointer
    fn second(phase: TouchPhase, pos: Pos2) -> Vec<Event> {
        vec![touch_of(1, phase, pos)]
    }

    fn presses(events: &[Event]) -> Vec<bool> {
        events.iter().filter_map(|e| if let Event::PointerButton { pressed, .. } = e { Some(*pressed) } else { None }).collect()
    }

    fn press_at(events: &[Event]) -> Option<Pos2> {
        events.iter().find_map(|e| if let Event::PointerButton { pressed: true, pos, .. } = e { Some(*pos) } else { None })
    }

    fn wheels(events: &[Event]) -> Vec<eframe::egui::Vec2> {
        events.iter().filter_map(|e| if let Event::MouseWheel { delta, .. } = e { Some(*delta) } else { None }).collect()
    }

    fn pointer(events: &[Event]) -> Vec<Pos2> {
        events.iter().filter_map(|e| if let Event::PointerMoved(p) = e { Some(*p) } else { None }).collect()
    }

    /// Run the frames (16 ms apart, from time `t0`) and return what egui gets, frame by frame.
    fn run(t: &mut Touch, t0: f64, frames: Vec<Vec<Event>>) -> Vec<Vec<Event>> {
        let step = |(k, mut e): (usize, Vec<Event>)| {
            t.rewrite(&mut e, t0 + k as f64 * 0.016);
            e
        };
        frames.into_iter().enumerate().map(step).collect()
    }

    fn with_picture() -> Touch {
        let mut t = Touch::default();
        t.set_pictures(vec![Rect::from_min_max(pos2(20.0, 100.0), pos2(380.0, 400.0))]);
        t
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
        let frames = vec![down(pos2(50.0, 300.0)), moved(pos2(50.0, 280.0)), moved(pos2(53.0, 250.0)), up(pos2(53.0, 250.0))];
        let all = run(&mut t, 0.0, frames).concat();
        assert!(presses(&all).is_empty());
        assert_eq!(&wheels(&all)[..2], &[vec2(0.0, -20.0), vec2(3.0, -30.0)]);
        assert!(!all.contains(&Event::PointerGone), "the pointer stays while the content runs on");
    }

    #[test]
    fn a_finger_lifted_while_it_moves_flings() {
        let mut t = Touch::default();
        run(&mut t, 0.0, vec![down(pos2(50.0, 300.0)), moved(pos2(50.0, 280.0)), moved(pos2(50.0, 250.0)), up(pos2(50.0, 250.0))]);
        // the content runs on upwards, each frame less, and comes to rest
        let (mut now, mut travel, mut frames, mut gone) = (0.048, vec![], 0, false);
        while !gone && frames < 1000 {
            now += 0.016;
            frames += 1;
            let mut e = vec![];
            let wake = t.rewrite(&mut e, now);
            travel.extend(wheels(&e).iter().map(|d| d.y));
            assert!(t.scrolled() || wheels(&e).is_empty());
            gone = e.contains(&Event::PointerGone);
            assert_eq!(wake.is_some(), !gone);
            assert!(presses(&e).is_empty());
        }
        assert!(gone, "the fling ends");
        assert!(travel.len() > 10 && travel.iter().all(|d| *d < 0.0));
        assert!(travel.windows(2).all(|w| w[1] >= w[0]), "it slows down: {travel:?}");
        let mut e = vec![];
        assert_eq!(t.rewrite(&mut e, now + 1.0), None);
        assert!(e.is_empty() && !t.scrolled());
    }

    #[test]
    fn a_finger_that_stops_before_it_lifts_does_not_fling_and_a_new_finger_stops_a_fling() {
        let mut t = Touch::default();
        for (now, mut e) in [(0.0, down(pos2(50.0, 300.0))), (0.016, moved(pos2(50.0, 250.0))), (0.4, up(pos2(50.0, 250.0)))] {
            t.rewrite(&mut e, now);
        }
        let mut e = vec![];
        assert_eq!(t.rewrite(&mut e, 0.42), None);
        assert!(wheels(&e).is_empty());

        let mut t = Touch::default();
        for (now, mut e) in [(0.0, down(pos2(50.0, 300.0))), (0.016, moved(pos2(50.0, 250.0))), (0.032, up(pos2(50.0, 250.0)))] {
            t.rewrite(&mut e, now);
        }
        let mut e = vec![];
        assert_eq!(t.rewrite(&mut e, 0.048), Some(0.0));
        assert_eq!(wheels(&e).len(), 1);
        let mut e = down(pos2(80.0, 200.0));
        t.rewrite(&mut e, 0.064);
        assert!(wheels(&e).is_empty() && presses(&e).is_empty());
        // ... and that finger, lifted in place, is a tap
        let mut e = up(pos2(80.0, 200.0));
        t.rewrite(&mut e, 0.1);
        assert_eq!(presses(&e), vec![true]);
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
        assert_eq!(t.take_long_press(), Some(pos2(10.0, 10.0)));
        assert_eq!(t.take_long_press(), None);
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
        let mut t = with_picture();
        let click = vec![Event::PointerMoved(pos2(50.0, 150.0)), button(pos2(50.0, 150.0), true, Modifiers::NONE)];
        let mut e = click.clone();
        assert_eq!(t.rewrite(&mut e, 0.0), None);
        assert_eq!(e, click);
    }

    #[test]
    fn a_finger_that_moves_on_a_picture_turns_it_and_does_not_scroll() {
        let mut t = with_picture();
        let frames = vec![
            down(pos2(200.0, 250.0)),
            moved(pos2(204.0, 250.0)),
            [moved(pos2(230.0, 240.0)), moved(pos2(250.0, 230.0))].concat(),
            moved(pos2(300.0, 200.0)),
            // it leaves the picture and goes on turning it
            moved(pos2(300.0, 60.0)),
            up(pos2(300.0, 60.0)),
            vec![],
        ];
        let out = run(&mut t, 0.0, frames);
        let all = out.concat();
        assert!(wheels(&all).is_empty() && !t.scrolled(), "the page does not scroll");
        assert_eq!(presses(&all), vec![true, false]);
        // the button goes down where the finger landed, and the pointer is still there when the frame ends: the
        // picture under the landing point is the one that turns. Where the finger went shows a frame later
        assert_eq!(press_at(&out[2]), Some(pos2(200.0, 250.0)));
        assert_eq!(pointer(&out[2]).last(), Some(&pos2(200.0, 250.0)));
        assert_eq!(pointer(&out[3]), vec![pos2(230.0, 240.0), pos2(250.0, 230.0), pos2(300.0, 200.0)]);
        assert_eq!(pointer(&out[4]), vec![pos2(300.0, 60.0)]);
        assert_eq!(presses(&out[5]), vec![false]);
        assert_eq!(all.last(), Some(&Event::PointerGone));
        // no fling after a turn
        let mut e = vec![];
        assert_eq!(t.rewrite(&mut e, 1.0), None);
        assert!(e.is_empty());
    }

    #[test]
    fn a_flick_on_a_picture_within_one_frame_is_a_press_and_then_a_release() {
        let mut t = with_picture();
        let mut e = [down(pos2(200.0, 250.0)), moved(pos2(260.0, 250.0)), up(pos2(300.0, 250.0))].concat();
        assert_eq!(t.rewrite(&mut e, 0.0), Some(0.0));
        assert_eq!(presses(&e), vec![true]);
        assert_eq!(pointer(&e).last(), Some(&pos2(200.0, 250.0)));
        let mut e = vec![];
        t.rewrite(&mut e, 0.016);
        assert_eq!(pointer(&e), vec![pos2(260.0, 250.0)]);
        assert_eq!(presses(&e), vec![false]);
    }

    #[test]
    fn a_tap_on_a_picture_is_a_click_and_a_finger_beside_it_scrolls() {
        let mut t = with_picture();
        let frames = vec![down(pos2(200.0, 250.0)), moved(pos2(203.0, 252.0)), up(pos2(203.0, 252.0)), vec![]];
        let out = run(&mut t, 0.0, frames).concat();
        assert_eq!(presses(&out), vec![true, false]);
        assert_eq!(press_at(&out), Some(pos2(200.0, 250.0)));
        // below the picture the page scrolls, also when the finger then passes over the picture
        let out = run(&mut t, 1.0, vec![down(pos2(200.0, 600.0)), moved(pos2(200.0, 500.0)), moved(pos2(200.0, 300.0))]).concat();
        assert!(presses(&out).is_empty());
        assert_eq!(wheels(&out), vec![vec2(0.0, -100.0), vec2(0.0, -200.0)]);
    }

    #[test]
    fn two_fingers_that_move_apart_on_a_picture_zoom_it() {
        let mut t = with_picture();
        let frames = vec![
            down(pos2(150.0, 250.0)),
            second(TouchPhase::Start, pos2(250.0, 250.0)),
            // 100 apart, then 120: a zoom from here on
            [moved(pos2(140.0, 250.0)), second(TouchPhase::Move, pos2(260.0, 250.0))].concat(),
            [moved(pos2(120.0, 250.0)), second(TouchPhase::Move, pos2(300.0, 250.0))].concat(),
        ];
        let out = run(&mut t, 0.0, frames).concat();
        assert!(presses(&out).is_empty() && wheels(&out).is_empty(), "no click, no scrolling: {out:?}");
        let p = t.take_pinch().expect("a zoom");
        assert_eq!(p.at, pos2(150.0, 250.0));
        assert!((p.factor - 1.5).abs() < 1e-4 && !p.ended, "{p:?}");
        assert_eq!(t.take_pinch(), None);
        // together again
        let out = run(&mut t, 0.1, vec![second(TouchPhase::Move, pos2(210.0, 250.0))]).concat();
        assert!(wheels(&out).is_empty());
        assert!((t.take_pinch().unwrap().factor - 0.5).abs() < 1e-4);
        // one finger lifted: the zoom is kept, and the other finger does nothing more
        let frames = vec![
            second(TouchPhase::End, pos2(210.0, 250.0)),
            moved(pos2(200.0, 400.0)),
            moved(pos2(200.0, 600.0)),
            up(pos2(200.0, 600.0)),
            vec![],
        ];
        let out = run(&mut t, 0.2, frames).concat();
        assert_eq!(t.take_pinch(), Some(Pinch { at: pos2(150.0, 250.0), factor: 1.0, ended: true }));
        assert!(presses(&out).is_empty() && wheels(&out).is_empty() && pointer(&out).is_empty(), "{out:?}");
        assert_eq!(out.last(), Some(&Event::PointerGone));
        // egui saw the first finger only
        assert!(out.iter().all(|e| !matches!(e, Event::Touch { id, .. } if *id == TouchId(1))));
        // the next finger starts anew
        let mut e = down(pos2(200.0, 600.0));
        assert!(t.rewrite(&mut e, 2.0).is_some());
    }

    #[test]
    fn a_second_finger_ends_a_turn_before_the_two_zoom() {
        let mut t = with_picture();
        let frames = vec![
            down(pos2(200.0, 250.0)),
            moved(pos2(240.0, 250.0)),
            moved(pos2(260.0, 250.0)),
            second(TouchPhase::Start, pos2(160.0, 250.0)),
            second(TouchPhase::Move, pos2(100.0, 250.0)),
            second(TouchPhase::Move, pos2(60.0, 250.0)),
        ];
        let out = run(&mut t, 0.0, frames);
        assert_eq!(presses(&out.concat()), vec![true, false]);
        assert_eq!(presses(&out[3]), vec![false], "the turn ends when the second finger lands");
        let p = t.take_pinch().unwrap();
        assert_eq!(p.at, pos2(200.0, 250.0), "the picture the first finger landed on");
        assert!((p.factor - 200.0 / 160.0).abs() < 1e-4, "{p:?}");
    }

    #[test]
    fn two_fingers_that_move_together_scroll_the_page_on_a_picture_too() {
        for mut t in [with_picture(), Touch::default()] {
            let frames = vec![
                down(pos2(150.0, 250.0)),
                second(TouchPhase::Start, pos2(250.0, 255.0)),
                [moved(pos2(150.0, 230.0)), second(TouchPhase::Move, pos2(250.0, 235.0))].concat(),
                [moved(pos2(150.0, 200.0)), second(TouchPhase::Move, pos2(250.0, 205.0))].concat(),
                up(pos2(150.0, 200.0)),
                second(TouchPhase::End, pos2(250.0, 205.0)),
            ];
            let out = run(&mut t, 0.0, frames).concat();
            assert!(presses(&out).is_empty());
            let travel: f32 = wheels(&out).iter().map(|d| d.y).sum();
            assert!((travel + 50.0).abs() < 1e-3, "{:?}", wheels(&out));
            assert_eq!(t.take_pinch(), None, "no zoom");
            let mut e = vec![];
            assert_eq!(t.rewrite(&mut e, 1.0), None, "and nothing runs on");
        }
    }

    #[test]
    fn two_fingers_beside_a_picture_do_not_zoom_and_do_not_tap() {
        let mut t = with_picture();
        let frames = vec![
            down(pos2(150.0, 600.0)),
            second(TouchPhase::Start, pos2(250.0, 600.0)),
            second(TouchPhase::Move, pos2(350.0, 600.0)),
            second(TouchPhase::End, pos2(350.0, 600.0)),
            up(pos2(150.0, 600.0)),
            vec![],
        ];
        let out = run(&mut t, 0.0, frames).concat();
        assert_eq!(t.take_pinch(), None);
        assert!(presses(&out).is_empty(), "{out:?}");
    }

    #[test]
    fn fingers_the_system_takes_away_leave_no_button_down() {
        let mut t = with_picture();
        run(&mut t, 0.0, vec![down(pos2(200.0, 250.0)), moved(pos2(240.0, 250.0)), moved(pos2(260.0, 250.0))]);
        let mut e = vec![touch(TouchPhase::Cancel, pos2(260.0, 250.0)), Event::PointerGone];
        t.rewrite(&mut e, 0.1);
        assert_eq!(presses(&e), vec![false]);
        assert_eq!(e.last(), Some(&Event::PointerGone));
        // a finger that had only landed: no click
        let mut e = down(pos2(200.0, 250.0));
        t.rewrite(&mut e, 1.0);
        let mut e = vec![touch(TouchPhase::Cancel, pos2(200.0, 250.0)), Event::PointerGone];
        t.rewrite(&mut e, 1.1);
        assert!(presses(&e).is_empty());
        let mut e = vec![];
        assert_eq!(t.rewrite(&mut e, 1.2), None);
        assert!(e.is_empty());
    }

    #[test]
    fn a_long_press_on_a_picture_drags_it_too() {
        let mut t = with_picture();
        let mut e = down(pos2(200.0, 250.0));
        t.rewrite(&mut e, 0.0);
        let mut e = vec![];
        t.rewrite(&mut e, HOLD + 0.01);
        assert_eq!(press_at(&e), Some(pos2(200.0, 250.0)));
        assert_eq!(t.take_long_press(), Some(pos2(200.0, 250.0)));
        let mut e = moved(pos2(260.0, 250.0));
        t.rewrite(&mut e, 0.6);
        assert!(wheels(&e).is_empty());
        assert_eq!(pointer(&e), vec![pos2(260.0, 250.0)]);
        // it moved: lifting it is the end of a drag, not of a long press in place
        let mut e = up(pos2(260.0, 250.0));
        t.rewrite(&mut e, 0.7);
        assert_eq!(t.take_long_tap(), None);
    }

    #[test]
    fn a_long_press_lifted_in_place_is_told_apart_from_a_drag() {
        let mut t = Touch::default();
        let mut e = down(pos2(100.0, 300.0));
        t.rewrite(&mut e, 0.0);
        // not yet: the finger may still scroll or tap
        let mut e = vec![];
        t.rewrite(&mut e, HOLD - 0.05);
        assert_eq!(t.take_long_press(), None);
        let mut e = vec![];
        t.rewrite(&mut e, HOLD + 0.01);
        assert_eq!(t.take_long_press(), Some(pos2(100.0, 300.0)));
        assert_eq!(t.take_long_tap(), None, "the finger is still down");
        // a finger trembles: within the slop it has not moved
        let mut e = moved(pos2(103.0, 302.0));
        t.rewrite(&mut e, 0.6);
        let mut e = up(pos2(103.0, 302.0));
        t.rewrite(&mut e, 0.9);
        assert_eq!(presses(&e), vec![false]);
        assert_eq!(t.take_long_tap(), Some(pos2(100.0, 300.0)));
        assert_eq!(t.take_long_tap(), None);
        // a finger that moved before the time was up scrolls: no long press at all
        let mut all = run(&mut t, 2.0, vec![down(pos2(100.0, 300.0)), moved(pos2(100.0, 250.0))]).concat();
        let mut e = vec![];
        t.rewrite(&mut e, 2.0 + HOLD + 0.1);
        all.extend(e);
        let mut e = up(pos2(100.0, 250.0));
        t.rewrite(&mut e, 3.0);
        all.extend(e);
        assert!(presses(&all).is_empty() && !wheels(&all).is_empty());
        assert_eq!((t.take_long_press(), t.take_long_tap()), (None, None));
        // a tap is no long press
        run(&mut t, 4.0, vec![down(pos2(100.0, 300.0)), up(pos2(100.0, 300.0)), vec![]]);
        assert_eq!((t.take_long_press(), t.take_long_tap()), (None, None));
        // fingers the system takes away during a long press: nothing follows
        let mut e = down(pos2(100.0, 300.0));
        t.rewrite(&mut e, 5.0);
        let mut e = vec![];
        t.rewrite(&mut e, 5.0 + HOLD + 0.01);
        let mut e = vec![touch(TouchPhase::Cancel, pos2(100.0, 300.0)), Event::PointerGone];
        t.rewrite(&mut e, 5.6);
        assert_eq!(presses(&e), vec![false]);
        assert_eq!(t.take_long_tap(), None);
    }
}
