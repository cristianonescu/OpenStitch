//! Commands that replace or write a design never leave the open project's file name on other
//! content (#1179): a template opens as File ▸ New does (untitled, unsaved changes asked about
//! first, its own tab with Open designs in tabs), through the menu and through the Assistant or
//! a socket client; the test swatches write their file and leave the open design and its file
//! alone; a saved swatch opens as File ▸ Open does.

use super::*;
use openstitch_app::editor::close_prompt::{Choice, Pending};

const TEMPLATE: &str = "builtin:team-badge";

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("openstitch-1179-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("dir");
    d
}

/// An editor with a saved project open (`project.osd`, one rectangle): the app and its bytes.
fn with_project(dir: &Path) -> (EditorApp, PathBuf, Vec<u8>) {
    let mut app = EditorApp::new(Launch::default());
    app.splash = None;
    app.ui.prefs.open_in_tabs = false;
    app.execute(&CommandRequest::new(
        "design.new",
        json!({ "name": "My project", "width": 100, "height": 100 }),
    ))
    .expect("new");
    app.execute(&CommandRequest::new(
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 30, "height": 20, "fill": "Red" }),
    ))
    .expect("rectangle");
    let project = dir.join("project.osd");
    app.path = Some(project.clone());
    app.save(false);
    assert!(!app.is_dirty());
    let bytes = std::fs::read(&project).expect("saved");
    (app, project, bytes)
}

/// File ▸ Save as Ctrl+S runs it: with a file name it writes there at once; with none it would
/// ask for a name (a dialog a test cannot answer, so it is not opened here).
fn ctrl_s(app: &mut EditorApp) {
    if app.path.is_some() {
        app.save(false);
    }
}

fn unchanged(project: &Path, before: &[u8]) {
    assert!(
        std::fs::read(project).expect("project") == before,
        "the project file was written over"
    );
}

#[test]
fn a_template_from_the_wizard_opens_untitled_and_save_never_writes_over_the_project() {
    let dir = temp_dir("menu");
    let (mut app, project, before) = with_project(&dir);
    // The Template Wizard's Next on step 1.
    app.lettering.templates.chosen = Some(TEMPLATE.into());
    app.guarded(Pending::Template(TEMPLATE.into()));
    assert!(!app.close_prompt.is_open(), "nothing unsaved: no question");
    assert_ne!(app.session.design().name, "My project", "the template is the design");
    assert_eq!(app.path, None, "untitled, as File ▸ New");
    assert_eq!(app.lettering.templates.step, 2, "the wizard goes on to its slots");
    assert!(!app.lettering.templates.slots.is_empty());
    ctrl_s(&mut app);
    unchanged(&project, &before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_template_asks_about_unsaved_changes_first() {
    let dir = temp_dir("ask");
    let (mut app, project, before) = with_project(&dir);
    app.execute(&CommandRequest::new(
        "shape.ellipse",
        json!({ "cx": 70, "cy": 60, "rx": 10, "fill": "Blue" }),
    ))
    .expect("an unsaved edit");
    let edited = app.session.design().clone();
    app.guarded(Pending::Template(TEMPLATE.into()));
    assert!(app.close_prompt.is_open(), "unsaved changes: asked first");
    assert_eq!(app.session.design(), &edited, "nothing replaced yet");
    // Cancel keeps the design and its file.
    app.answer_close_prompt(Choice::Cancel);
    assert_eq!(app.session.design(), &edited);
    assert_eq!(app.path.as_deref(), Some(project.as_path()));
    // Don't Save opens the template untitled; the project file is as it was saved.
    app.guarded(Pending::Template(TEMPLATE.into()));
    app.answer_close_prompt(Choice::DontSave);
    assert_ne!(app.session.design(), &edited);
    assert_eq!(app.path, None);
    ctrl_s(&mut app);
    unchanged(&project, &before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn with_open_designs_in_tabs_a_template_gets_its_own_tab() {
    let dir = temp_dir("tabs");
    let (mut app, project, before) = with_project(&dir);
    app.ui.prefs.open_in_tabs = true;
    let tabs = app.tab_count();
    app.guarded(Pending::Template(TEMPLATE.into()));
    assert_eq!(app.tab_count(), tabs + 1, "a new tab, as File ▸ New");
    assert_eq!(app.path, None, "the template's tab is untitled");
    ctrl_s(&mut app);
    unchanged(&project, &before);
    // A template that does not open closes the tab it added.
    let tabs = app.tab_count();
    app.guarded(Pending::Template("builtin:no-such-template".into()));
    assert_eq!(app.tab_count(), tabs);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_template_the_assistant_or_a_socket_client_opens_is_untitled_too() {
    let dir = temp_dir("remote");
    let (mut app, project, before) = with_project(&dir);
    let r = app.remote_request(&json!({ "command": "template.open",
        "params": { "template": TEMPLATE } }));
    assert_eq!(r["ok"], json!(true), "{r}");
    assert_eq!(app.path, None, "as design.new from the Assistant (#1178)");
    ctrl_s(&mut app);
    unchanged(&project, &before);
    // Over unsaved changes: they stay in Undo and the new design is marked unsaved (#1186).
    let (mut app, project, before) = with_project(&dir);
    app.execute(&CommandRequest::new(
        "shape.ellipse",
        json!({ "cx": 70, "cy": 60, "rx": 10, "fill": "Blue" }),
    ))
    .expect("an unsaved edit");
    let r = app.remote_request(&json!({ "command": "template.open",
        "params": { "template": TEMPLATE } }));
    assert_eq!(r["ok"], json!(true), "{r}");
    assert_eq!(app.path, None);
    assert!(app.is_dirty(), "closing still asks");
    assert!(app.session.can_undo(), "the edited design is in Undo");
    ctrl_s(&mut app);
    unchanged(&project, &before);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_test_swatches_leave_the_open_design_and_its_file_alone() {
    let dir = temp_dir("swatches");
    let swatches = [
        ("design.testSwatch", json!({ "ids": [1] })),
        (
            "design.testSwatches",
            json!({ "id": 1, "attribute": "density", "start": 3.0, "step": 1.0, "count": 2 }),
        ),
        ("fabric.calibrationSwatch", json!({ "fabric": "standard" })),
    ];
    for (k, (command, params)) in swatches.iter().enumerate() {
        for remote in [false, true] {
            let (mut app, project, before) = with_project(&dir);
            let design = app.session.design().clone();
            let out = dir.join(format!("swatch-{k}-{remote}.pes"));
            let mut params = params.clone();
            params["path"] = json!(out.display().to_string());
            if remote {
                let r = app.remote_request(&json!({ "command": command, "params": params }));
                assert_eq!(r["ok"], json!(true), "{command}: {r}");
            } else {
                app.execute(&CommandRequest::new(*command, params))
                    .unwrap_or_else(|| panic!("{command}: {}", app.status));
            }
            assert!(out.is_file(), "{command} wrote its file");
            assert_eq!(app.session.design(), &design, "{command}: the open design is unchanged");
            assert_eq!(app.path.as_deref(), Some(project.as_path()), "{command}");
            assert!(!app.is_dirty(), "{command}");
            ctrl_s(&mut app);
            unchanged(&project, &before);
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_a_saved_swatch_asks_first_and_never_takes_the_projects_name() {
    let dir = temp_dir("open-swatch");
    let (mut app, project, before) = with_project(&dir);
    let out = dir.join("swatch.osd");
    app.execute(&CommandRequest::new(
        "design.testSwatch",
        json!({ "ids": [1], "path": out.display().to_string() }),
    ))
    .expect("swatch");
    app.execute(&CommandRequest::new(
        "shape.ellipse",
        json!({ "cx": 70, "cy": 60, "rx": 10, "fill": "Blue" }),
    ))
    .expect("an unsaved edit");
    let edited = app.session.design().clone();
    crate::swatch_ui::open_saved_swatch(&mut app, out.clone());
    assert!(app.close_prompt.is_open(), "unsaved changes: asked first, as File ▸ Open");
    assert_eq!(app.session.design(), &edited);
    app.answer_close_prompt(Choice::Cancel);
    assert_eq!(app.path.as_deref(), Some(project.as_path()));
    unchanged(&project, &before);
    let _ = std::fs::remove_dir_all(&dir);
}
