//! `Session::sibling` and `Session::copy_user_settings` (#552): a second document (a tab)
//! has its own design and history and shares the user settings.

use openstitch_engine::Session;
use serde_json::json;

#[test]
fn a_sibling_is_an_empty_document_with_its_own_history() {
    let dir = openstitch_testutil::TempDir::new("engine-sibling");
    let mut a = Session::new(dir.to_path_buf());
    a.execute(
        "shape.rectangle",
        Some(&json!({ "x": 0, "y": 0, "width": 10, "height": 10 })),
    )
    .expect("rectangle");
    let mut b = a.sibling();
    assert!(b.design().objects.is_empty(), "an empty design");
    assert!(!b.can_undo(), "its own history");
    b.execute(
        "shape.ellipse",
        Some(&json!({ "cx": 20, "cy": 20, "rx": 5, "ry": 5 })),
    )
    .expect("ellipse");
    b.execute("history.undo", None).expect("undo");
    assert!(b.design().objects.is_empty());
    assert_eq!(
        a.design().objects.len(),
        1,
        "the first document is untouched"
    );
    assert!(a.can_undo());
    assert_eq!(a.capabilities(), b.capabilities());
    b.copy_user_settings(&a);
    assert_eq!(b.user_settings(), a.user_settings());
}
