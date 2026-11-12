//! Interactive controls (round 13): while a control's evaluation runs, newer positions replace each other (only
//! the newest is evaluated next) and letting go interrupts the stale evaluation; the result shows below.
use mc_nbmodel::{Command, FakeEnv, FakeKernel, KernelRequest, Notebook};

#[test]
fn stale_positions_are_dropped_and_letting_go_interrupts() {
    let text = "#! mathcas 2 notebook\n\n#% cell label=1\nm\n#% output n=1\n#= manipulate(a + 1, [a, 0, 10, 1])\n";
    let mut nb = Notebook::parse(text).unwrap();
    let mut env = FakeEnv::default();
    let mut k = FakeKernel::new(true);
    k.hold_side = true;
    let out = nb.cells()[1].id;
    // a drag: three positions while the first evaluates
    for v in [1.0, 2.0, 3.0] {
        nb.apply(Command::SetControl(out, 0, v, false), &mut env);
        nb.pump(&mut k);
    }
    let sides = |k: &FakeKernel| -> Vec<String> {
        k.log.iter().filter_map(|r| if let KernelRequest::Side { src, .. } = r { Some(src.clone()) } else { None }).collect()
    };
    assert_eq!(sides(&k), ["{ let a = 1; a + 1 }"], "one evaluation at a time");
    // let go at 4: the stale evaluation (1) is interrupted, 2 and 3 were never sent, 4 is evaluated
    nb.apply(Command::SetControl(out, 0, 4.0, true), &mut env);
    nb.pump(&mut k);
    assert!(k.log.iter().any(|r| matches!(r, KernelRequest::Interrupt { .. })));
    assert_eq!(sides(&k), ["{ let a = 1; a + 1 }", "{ let a = 4; a + 1 }"]);
    k.release();
    nb.pump(&mut k);
    let printed = nb.to_text();
    assert!(printed.contains("#% output n=1 controls=4\n"), "{printed}");
    assert!(printed.contains("#% output manipulated\n#= { let a = 4; a + 1 }\n"), "{printed}");
    // the interrupted evaluation showed nothing; nothing is running now
    assert!(!printed.contains("let a = 1;"));
}
