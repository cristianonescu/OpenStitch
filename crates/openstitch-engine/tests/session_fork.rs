//! `Session::fork` and `Session::adopt_design` (#558): a worker session runs a slow open on a
//! copy; adopting its design is one undo step, and dropping it leaves the session untouched.

use openstitch_engine::Session;
use serde_json::json;

fn rectangle(s: &mut Session, x: f64) {
    s.execute(
        "shape.rectangle",
        Some(&json!({ "x": x, "y": 0, "width": 10, "height": 10 })),
    )
    .expect("rectangle");
}

#[test]
fn a_fork_works_on_a_copy_and_adopting_it_is_one_undo_step() {
    let dir = openstitch_testutil::TempDir::new("engine-fork");
    let mut s = Session::new(dir.to_path_buf());
    rectangle(&mut s, 0.0);
    let before = s.design().clone();

    let mut worker = s.fork();
    assert_eq!(worker.design(), &before, "the fork starts from the design");
    assert!(!worker.can_undo(), "with no history");
    rectangle(&mut worker, 20.0);
    rectangle(&mut worker, 40.0);
    assert_eq!(
        s.design(),
        &before,
        "the session is untouched while the worker runs"
    );

    assert!(s.adopt_design(worker.design().clone()));
    assert_eq!(s.design().objects.len(), 3);
    s.execute("history.undo", None).expect("undo");
    assert_eq!(
        s.design(),
        &before,
        "one Undo takes the whole worker's result back"
    );
    s.execute("history.redo", None).expect("redo");
    assert_eq!(s.design().objects.len(), 3);

    let same = s.design().clone();
    assert!(!s.adopt_design(same), "an unchanged design records nothing");
}
