//! Requests from the loopback automation socket (`--listen`), run on the UI thread between
//! frames so they share the editor's undo history with the user's own edits.

use crate::app::EditorApp;
use openstitch_engine::Effect;
#[cfg(not(target_arch = "wasm32"))]
use openstitch_remote::Policy;
use serde_json::{Map, Value, json};

/// The socket's settings and server: `openstitch-remote` on the desktop; a web page cannot
/// listen on a socket, so the web build has empty stand-ins and never serves (#693).
#[cfg(not(target_arch = "wasm32"))]
pub use openstitch_remote::{Server, ServerConfig};

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Default)]
pub struct ServerConfig;

#[cfg(target_arch = "wasm32")]
pub struct Server;

impl EditorApp {
    /// The web build has no socket to serve (#693).
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn serve_remote(&mut self) {}

    /// The web page's automation API (`openstitch.run`, #693): one request, run as the socket's
    /// are, between frames on the editor's own history.
    #[cfg(target_arch = "wasm32")]
    pub fn web_request(&mut self, request: &Value) -> Value {
        self.remote_request(request)
    }

    /// Runs every request socket clients have queued, with their capabilities and file roots.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn serve_remote(&mut self) {
        let Some(server) = self.remote.take() else {
            return;
        };
        while let Some(job) = server.try_next() {
            let saved = server.policy().apply(&mut self.session);
            let reply = self.remote_request(job.request());
            Policy::restore(&mut self.session, saved);
            job.respond(reply);
        }
        self.remote = Some(server);
    }

    /// Edits are one Undo step each; undo/redo act like the Edit menu, so a client's undo also
    /// undoes a grouped user gesture as a whole. `project.open`, `design.new` and `project.save`
    /// then do what File ▸ Open, New and Save do ([`EditorApp::after_remote_file_command`]).
    pub(crate) fn remote_request(&mut self, request: &Value) -> Value {
        let command = request
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if command == "commands.list" || command.starts_with("ui.") {
            return self.remote_ui_request(command, request);
        }
        let mut request = std::borrow::Cow::Borrowed(request);
        if command == "project.save" {
            // The Assistant's own saves and a socket client's write the document's conversation
            // as File ▸ Save does (#1176).
            crate::assistant_panel::chat_to_session(self);
        }
        // A relative path of a file-writing command is taken from the design's own folder, never
        // from the process's working folder (#1187, #1199).
        match self.beside_the_design(command, &request) {
            Ok(Some(r)) => request = std::borrow::Cow::Owned(r),
            Ok(None) => {}
            Err(e) => return failed(&request, &e),
        }
        let request = request.as_ref();
        let unsaved_before = self.is_dirty();
        let reply = self.remote_engine_request(command, request);
        if reply["ok"] == json!(true) {
            self.after_remote_file_command(command, &reply["result"], unsaved_before);
        }
        reply
    }

    /// `request` with its relative `path` joined to the folder of the design's file (#1187);
    /// `None` when the path is absolute or missing (the engine says what is wrong), an error
    /// when it is relative and the design has no file.
    ///
    /// Every file-writing command (Effect::File) is handled so, on its `path`, `output` and `dir`
    /// params (#1199); `stitchtool` and `openstitch-mcp` on their own keep their working folder.
    pub(crate) fn beside_the_design(
        &self,
        command: &str,
        request: &Value,
    ) -> Result<Option<Value>, String> {
        if self.session.permits(command).map(|c| c.effect) != Ok(Effect::File) {
            return Ok(None);
        }
        let mut r = request.clone();
        let mut changed = false;
        for key in openstitch_engine::commands::BESIDE_DESIGN_KEYS {
            let Some(path) = request.pointer(&format!("/params/{key}")).and_then(Value::as_str)
            else {
                continue;
            };
            // A rooted path ("/x" on Windows too) is not a bare name: it stays as given.
            let p = std::path::Path::new(path);
            if path.trim().is_empty() || p.is_absolute() || p.has_root() {
                continue;
            }
            let Some(folder) = self.path.as_ref().and_then(|p| p.parent()) else {
                return Err(format!(
                    "the design has no file yet, so '{path}' has no folder to be relative to: ask the user where to write it, then give a full path"
                ));
            };
            r["params"][key] = json!(folder.join(path).display().to_string());
            changed = true;
        }
        Ok(changed.then_some(r))
    }

    /// What the editor does after the menu's own Open, New and Save, for the same commands from
    /// the Assistant or a socket client (#1178): the opened file becomes the document's file,
    /// with a new conversation identity and the chat it kept (`after_open`; during the
    /// Assistant's turn its conversation goes on and the file keeps its own); a new design is a
    /// new document (`after_load`); a save makes that file the document's file, as Save As does,
    /// and the conversation stays with it.
    ///
    /// A design that had unsaved changes stays in Undo after an open or a new design, and the
    /// new one is marked unsaved, so closing still asks (#1186): it must not look saved.
    fn after_remote_file_command(&mut self, command: &str, result: &Value, unsaved_before: bool) {
        let path = || result["path"].as_str().map(std::path::PathBuf::from);
        match command {
            "project.open" => {
                if let Some(p) = path() {
                    self.after_open(&p);
                    if unsaved_before {
                        self.mark_unsaved();
                    }
                }
            }
            "design.new" => {
                self.after_load(None);
                if unsaved_before {
                    self.mark_unsaved();
                }
            }
            "project.save" => {
                if let Some(p) = path() {
                    self.status = openstitch_app::tf!("ui-status-saved", path = p.display());
                    self.ui.add_recent(&p);
                    self.path = Some(p);
                    self.mark_saved();
                }
            }
            _ => {}
        }
    }

    fn remote_engine_request(&mut self, command: &str, request: &Value) -> Value {
        match self.session.permits(command).map(|c| c.effect) {
            Ok(Effect::History) => {
                let undo = command == "history.undo";
                let possible = if undo {
                    self.session.can_undo()
                } else {
                    self.session.can_redo()
                };
                if !possible {
                    // The engine's "nothing to undo/redo" error.
                    return self.session.handle(request);
                }
                if undo {
                    self.undo();
                } else {
                    self.redo();
                }
                let mut reply = Map::new();
                if let Some(id) = request.get("id") {
                    reply.insert("id".into(), id.clone());
                }
                reply.insert("ok".into(), json!(true));
                reply.insert(
                    "result".into(),
                    json!({ "canUndo": self.session.can_undo(), "canRedo": self.session.can_redo() }),
                );
                Value::Object(reply)
            }
            Ok(Effect::Edit) => {
                let before = self.session.design().clone();
                let reply = self.session.handle(request);
                if *self.session.design() != before {
                    self.undo_groups.push(1);
                    self.redo_groups.clear();
                    self.refresh();
                }
                reply
            }
            _ => self.session.handle(request),
        }
    }

    /// `commands.list` with the editor's `ui.*` commands added, and the `ui.*` commands
    /// themselves (B17), in the engine's reply shape.
    fn remote_ui_request(&mut self, command: &str, request: &Value) -> Value {
        let outcome = if command == "commands.list" {
            let engine = self.session.handle(request);
            if engine["ok"] != json!(true) {
                return engine;
            }
            Ok(crate::assistant_ui::commands_list(engine["result"].clone()))
        } else {
            self.ui_command(command, request.get("params"))
        };
        let mut reply = Map::new();
        if let Some(id) = request.get("id") {
            reply.insert("id".into(), id.clone());
        }
        match outcome {
            Ok(result) => {
                reply.insert("ok".into(), json!(true));
                reply.insert("result".into(), result);
            }
            Err(e) => {
                reply.insert("ok".into(), json!(false));
                reply.insert("error".into(), json!(e));
            }
        }
        Value::Object(reply)
    }
}

/// The engine's failure reply for `request`, with `error`.
fn failed(request: &Value, error: &str) -> Value {
    let mut reply = Map::new();
    if let Some(id) = request.get("id") {
        reply.insert("id".into(), id.clone());
    }
    reply.insert("ok".into(), json!(false));
    reply.insert("error".into(), json!(error));
    Value::Object(reply)
}

#[cfg(test)]
mod tests {
    use crate::app::{EditorApp, Launch};
    use openstitch_engine::Capabilities;
    use openstitch_remote::{Client, ServerConfig};
    use serde_json::{Value, json};

    /// Sends `request` from a client thread and drains it on this ("UI") thread.
    fn send(app: &mut EditorApp, client: &mut Option<Client>, request: Value) -> Value {
        let mut c = client.take().expect("client");
        let t = std::thread::spawn(move || (c.request(&request), c));
        while !t.is_finished() {
            app.serve_remote();
            std::thread::yield_now();
        }
        let (reply, c) = t.join().expect("join");
        *client = Some(c);
        reply.expect("reply")
    }

    #[test]
    fn socket_requests_share_the_editor_history() {
        let mut app = EditorApp::new(Launch::default());
        let server = openstitch_remote::Server::start(ServerConfig::default()).expect("listen");
        let mut client = Some(Client::new(server.endpoint().clone()).expect("client"));
        app.remote = Some(server);
        let rect = json!({"command": "shape.rectangle",
            "params": {"x": 0, "y": 0, "width": 20, "height": 10}});
        let added = send(&mut app, &mut client, rect.clone());
        assert_eq!(added["ok"], json!(true), "{added}");
        assert_eq!(app.session.design().objects.len(), 1);
        assert!(app.is_dirty(), "the editor recompiled");
        // The user's Undo undoes the client's edit, and the client's undo the user's.
        app.undo();
        assert!(app.session.design().objects.is_empty());
        app.redo();
        let undone = send(
            &mut app,
            &mut client,
            json!({"id": 3, "command": "history.undo"}),
        );
        assert_eq!(undone["id"], json!(3));
        assert_eq!(undone["result"]["canRedo"], json!(true));
        assert!(app.session.design().objects.is_empty());
        let nothing = send(&mut app, &mut client, json!({"command": "history.undo"}));
        assert_eq!(nothing["ok"], json!(false));
        // The editor's own session keeps its capabilities and no sandbox.
        assert_eq!(app.session.capabilities(), Capabilities::FULL);
        assert!(app.session.sandbox().is_none());
    }

    #[test]
    fn read_only_socket_clients_cannot_edit_or_undo() {
        let mut app = EditorApp::new(Launch::default());
        let server = openstitch_remote::Server::start(ServerConfig {
            capabilities: Capabilities::FULL.read_only(),
            ..Default::default()
        })
        .expect("listen");
        let mut client = Some(Client::new(server.endpoint().clone()).expect("client"));
        app.remote = Some(server);
        let refused = send(
            &mut app,
            &mut client,
            json!({"command": "shape.rectangle",
                "params": {"x": 0, "y": 0, "width": 5, "height": 5}}),
        );
        assert_eq!(refused["ok"], json!(false));
        let undo = send(&mut app, &mut client, json!({"command": "history.undo"}));
        assert_eq!(undo["ok"], json!(false));
        assert!(app.session.design().objects.is_empty());
    }
}
