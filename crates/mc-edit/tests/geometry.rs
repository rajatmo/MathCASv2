//! Motions that need geometry, driven by a real layout (`mc_boxes::layout` with its fixed metrics): Home and End
//! on wrapped lines, Up and Down keeping the goal x, into and out of boxes on other lines.
use mc_boxes::layout::{layout, Layout, LayoutOptions, MonoMetrics};
use mc_boxes::{from_text, Step};
use mc_edit::{Command, Context, Editor, Motion, Position, Status};

fn lay(ed: &Editor, width: f32) -> Layout {
    layout(ed.root(), &MonoMetrics, &LayoutOptions { indent_continuations: false, ..LayoutOptions::input(10.0, Some(width)) })
}

fn go(ed: &mut Editor, m: Motion, extend: bool, width: f32) -> Status {
    let l = lay(ed, width);
    let cx = Context { geometry: Some(&l), auto_close: false };
    ed.apply(&Command::Move(m, extend), &cx).status
}

#[test]
fn home_end_and_up_down_follow_the_visual_lines() {
    let text = "alpha + beta + gamma + delta + epsilon + zeta + eta + theta";
    let mut ed = Editor::from_text(text);
    let w = 120.0;
    let l = lay(&ed, w);
    assert!(l.lines.len() >= 3, "{:?}", l.lines);
    let (l0, l1) = (l.lines[0], l.lines[1]);
    // a caret in the middle of the second line: Home to its start, End to its end
    let mid = (l1.start + l1.end) / 2;
    ed.apply(&Command::SetCaret(Position::root(mid)), &Context::default());
    go(&mut ed, Motion::Home, false, w);
    assert_eq!(ed.caret(), &Position::root(l1.start));
    go(&mut ed, Motion::End, false, w);
    assert_eq!(ed.caret(), &Position::root(l1.end));
    // Up from the middle of the second line keeps x on the first; Down twice comes back to the same x
    ed.apply(&Command::SetCaret(Position::root(mid)), &Context::default());
    let x = l.caret(&[], mid).unwrap().x;
    go(&mut ed, Motion::Up, false, w);
    let up = ed.caret().offset;
    assert!(up >= l0.start && up <= l0.end, "{up}");
    assert!((l.caret(&[], up).unwrap().x - x).abs() <= 6.1);
    go(&mut ed, Motion::Down, false, w);
    assert_eq!(ed.caret(), &Position::root(mid), "the goal x is kept");
    // past the first line: the content's start is before the cell
    ed.apply(&Command::SetCaret(Position::root(1)), &Context::default());
    assert_eq!(go(&mut ed, Motion::Up, false, w), Status::Before);
    // Shift+Up there selects to the start
    assert_eq!(go(&mut ed, Motion::Up, true, w), Status::Done);
    assert_eq!(ed.range(), Some((vec![], 0..1)));
}

#[test]
fn up_and_down_go_between_slots_first_then_lines() {
    let mut ed = Editor::new(from_text("1 + ((ab)/(cd))\nx"));
    let num = vec![Step { item: 4, slot: 0 }];
    ed.apply(&Command::SetCaret(Position::new(&num, 1)), &Context::default());
    go(&mut ed, Motion::Down, false, 400.0);
    assert_eq!(ed.notation(), "1 + (ab/c▮d)\nx");
    // from the denominator Down goes to the next line
    go(&mut ed, Motion::Down, false, 400.0);
    assert_eq!(ed.caret().path, Vec::<Step>::new());
    assert!(ed.caret().offset >= 6, "{}", ed.notation());
    // Home in a slot: the slot's start, again: the line's start
    ed.apply(&Command::SetCaret(Position::new(&num, 1)), &Context::default());
    go(&mut ed, Motion::Home, false, 400.0);
    assert_eq!(ed.notation(), "1 + (▮ab/cd)\nx");
    go(&mut ed, Motion::Home, false, 400.0);
    assert_eq!(ed.notation(), "▮1 + (ab/cd)\nx");
    go(&mut ed, Motion::End, false, 400.0);
    assert_eq!(ed.notation(), "1 + (ab/cd)▮\nx");
}
