//! Copy and paste of objects through the system clipboard (#500), so they reach another
//! editor window or survive their originals. Copy writes the selection as `object.get` data in
//! a small envelope, as text; Paste reads it back and runs one `object.paste`. Text that is
//! not an envelope is left alone (the background's Paste Image reads images, not text).
//!
//! Nothing here touches the clipboard itself or depends on egui.

use super::request::CommandRequest;
use openstitch_design::text::{self, FontRef};
use openstitch_design::{Design, DesignObject, Rect};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// The envelope's `openstitch` field.
pub const KIND: &str = "objects";
pub const VERSION: u32 = 1;
/// How every envelope's text starts (the fields are written in this order), so the clipboard
/// can be checked cheaply.
pub const PREFIX: &str = "{\"openstitch\":\"objects\"";

/// Objects on the clipboard, with the design fonts their texts use.
#[derive(Clone, Debug, PartialEq)]
pub struct Envelope {
    pub openstitch: String,
    pub version: u32,
    pub objects: Vec<Value>,
    pub fonts: Vec<Value>,
}

impl Envelope {
    /// The selected objects of `design` in sewing order, with the design fonts their texts
    /// use; `None` when nothing is selected.
    pub fn copy(design: &Design, ids: &BTreeSet<u64>) -> Option<Envelope> {
        let picked: Vec<&DesignObject> = design
            .objects
            .iter()
            .filter(|o| ids.contains(&o.id))
            .collect();
        if picked.is_empty() {
            return None;
        }
        let mut fonts: Vec<Value> = Vec::new();
        let mut font_ids: Vec<&str> = Vec::new();
        for t in picked.iter().filter_map(|o| o.text.as_ref()) {
            if let Some(FontRef::Design(f)) = text::resolve(&t.font, &design.fonts)
                && !font_ids.contains(&f.id.as_str())
            {
                font_ids.push(&f.id);
                fonts.extend(serde_json::to_value(f).ok());
            }
        }
        Some(Envelope {
            openstitch: KIND.into(),
            version: VERSION,
            objects: picked
                .iter()
                .filter_map(|o| serde_json::to_value(o).ok())
                .collect(),
            fonts,
        })
    }

    pub fn to_text(&self) -> String {
        // serde_json keeps keys sorted unless told otherwise, so the fields are written by
        // hand in PREFIX's order.
        let mut out = format!("{PREFIX},\"version\":{}", self.version);
        out.push_str(",\"objects\":");
        out.push_str(&Value::Array(self.objects.clone()).to_string());
        if !self.fonts.is_empty() {
            out.push_str(",\"fonts\":");
            out.push_str(&Value::Array(self.fonts.clone()).to_string());
        }
        out.push('}');
        out
    }

    /// The envelope in `text`, `None` for any other text (or a newer version).
    pub fn parse(text: &str) -> Option<Envelope> {
        if !looks_like(text) {
            return None;
        }
        let v: Value = serde_json::from_str(text).ok()?;
        let list = |k: &str| v.get(k).and_then(Value::as_array).cloned();
        let e = Envelope {
            openstitch: v.get("openstitch")?.as_str()?.to_string(),
            version: u32::try_from(v.get("version")?.as_u64()?).ok()?,
            objects: list("objects")?,
            fonts: list("fonts").unwrap_or_default(),
        };
        (e.openstitch == KIND && e.version == VERSION && !e.objects.is_empty()).then_some(e)
    }

    /// The `object.paste` adding these objects to `design`: where they were (PE-DESIGN pastes
    /// at the same position), or centred on the Design Page when they would fall off it.
    pub fn paste_request(&self, design: &Design) -> CommandRequest {
        let mut params = json!({ "objects": self.objects });
        if !self.fonts.is_empty() {
            params["fonts"] = json!(self.fonts);
        }
        if let Some(b) = self.bounds() {
            let page = Rect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: design.page.width,
                max_y: design.page.height,
            };
            let inside = b.min_x >= page.min_x
                && b.min_y >= page.min_y
                && b.max_x <= page.max_x
                && b.max_y <= page.max_y;
            if !inside {
                let to = design.page.center();
                let from = b.center();
                params["dx"] = json!(round_mm(to.x - from.x));
                params["dy"] = json!(round_mm(to.y - from.y));
            }
        }
        CommandRequest::new("object.paste", params)
    }

    fn bounds(&self) -> Option<Rect> {
        let objects: Vec<DesignObject> = self
            .objects
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();
        Rect::enclosing(objects.iter().flat_map(DesignObject::points))
    }
}

/// Whether `text` starts like an envelope (no parsing).
pub fn looks_like(text: &str) -> bool {
    text.trim_start().starts_with(PREFIX)
}

/// 0.1 mm units to mm, to the hundredth.
fn round_mm(units: f64) -> f64 {
    (units / 10.0 * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_engine::Session;

    fn session() -> Session {
        let mut s = Session::new(std::env::temp_dir());
        for (x, fill) in [(10, "Red"), (40, "Blue")] {
            s.execute(
                "shape.rectangle",
                Some(&json!({"x": x, "y": 10, "width": 20, "height": 10, "fill": fill})),
            )
            .expect("rectangle");
        }
        s
    }

    #[test]
    fn copy_then_paste_into_another_design_gives_the_same_objects() {
        let a = session();
        let env = Envelope::copy(a.design(), &BTreeSet::from([2])).expect("copied");
        let text = env.to_text();
        assert!(text.starts_with(PREFIX), "{}", &text[..40]);
        let back = Envelope::parse(&text).expect("parsed");
        assert_eq!(back, env);
        // Window B: another session, one undo step.
        let mut b = Session::new(std::env::temp_dir());
        let r = back.paste_request(b.design());
        assert!(r.params.get("dx").is_none(), "same place");
        b.execute(r.command, Some(&r.params)).expect("paste");
        let (src, dst) = (&a.design().objects[1], &b.design().objects[0]);
        assert_eq!(dst.contours, src.contours);
        assert_eq!(dst.region, src.region);
        b.execute("history.undo", Some(&json!({}))).expect("undo");
        assert!(b.design().objects.is_empty());
    }

    #[test]
    fn copy_survives_deleting_the_originals() {
        let mut a = session();
        let env = Envelope::copy(a.design(), &BTreeSet::from([1, 2])).expect("copied");
        a.execute("object.delete", Some(&json!({"ids": [1, 2]})))
            .expect("delete");
        let r = env.paste_request(a.design());
        let out = a.execute(r.command, Some(&r.params)).expect("paste");
        assert_eq!(out["ids"].as_array().map(Vec::len), Some(2));
        assert_eq!(a.design().objects.len(), 2);
    }

    #[test]
    fn objects_off_the_page_are_centred() {
        let a = session();
        let env = Envelope::copy(a.design(), &BTreeSet::from([2])).expect("copied");
        let mut small = Session::new(std::env::temp_dir());
        small
            .execute("design.new", Some(&json!({"width": 40, "height": 40})))
            .expect("page");
        let r = env.paste_request(small.design());
        // The 20 x 10 mm box at (40, 10) mm moves to the 40 mm page's centre.
        assert_eq!(r.params["dx"], json!(-30.0));
        assert_eq!(r.params["dy"], json!(5.0));
    }

    #[test]
    fn other_text_is_not_an_envelope() {
        for t in [
            "",
            "hello",
            "{\"openstitch\":\"font\"}",
            "{\"openstitch\":\"objects\",\"version\":2,\"objects\":[{}]}",
            "{\"openstitch\":\"objects\",\"version\":1,\"objects\":[]}",
            "{\"openstitch\":\"objects\",\"version\":1,\"objects\":",
        ] {
            assert!(Envelope::parse(t).is_none(), "{t}");
        }
    }
}
