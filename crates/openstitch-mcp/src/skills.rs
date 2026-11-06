//! The bundled embroidery skill (`skills/openstitch-embroidery/` at the repository root),
//! compiled in and served by the `--assistant` tool `skill_reference`.

macro_rules! skill_file {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("../../../skills/openstitch-embroidery/", $name)),
        )
    };
}

/// `(name, text)`, in the order `skill_reference` lists them. `SKILL.md` comes first; the
/// illustrated example designs (#492) are `stitchtool exec` scripts after the guides.
pub const FILES: &[(&str, &str)] = &[
    skill_file!("SKILL.md"),
    skill_file!("reference/answers.md"),
    skill_file!("reference/composition.md"),
    skill_file!("reference/examples.md"),
    skill_file!("reference/recipes.md"),
    skill_file!("reference/sewability.md"),
    skill_file!("reference/examples/applique-heart.json"),
    skill_file!("reference/examples/badge-arc-text.json"),
    skill_file!("reference/examples/cat-face.json"),
    skill_file!("reference/examples/croissant.json"),
    skill_file!("reference/examples/lightbulb.json"),
    skill_file!("reference/examples/line-art-cup.json"),
    skill_file!("reference/examples/monogram.json"),
    skill_file!("reference/examples/mushroom.json"),
    skill_file!("reference/examples/strawberry.json"),
    skill_file!("reference/examples/watermelon.json"),
];

/// A file's title: a Markdown file's first heading, without the `#`s (skipping any front
/// matter); an example script's design name (its `design.new` step), as "Example: name".
pub fn title(text: &str) -> String {
    if text.trim_start().starts_with('[') {
        let name = serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|steps| {
                steps
                    .as_array()?
                    .iter()
                    .find_map(|s| {
                        (s["command"] == "design.new").then(|| s["params"]["name"].as_str())?
                    })
                    .map(str::to_string)
            })
            .unwrap_or_default();
        return format!("Example: {name}");
    }
    text.lines()
        .find_map(|line| line.strip_prefix('#'))
        .map(|rest| rest.trim_start_matches('#').trim())
        .unwrap_or("")
        .to_string()
}

/// A file by its name, also without `.md` / `.json` or the `reference/` and
/// `reference/examples/` folders (`recipes`, `strawberry`).
pub fn find(name: &str) -> Option<(&'static str, &'static str)> {
    let name = name.trim().trim_start_matches("./");
    let bare = |n: &str| -> String {
        n.strip_suffix(".md")
            .or_else(|| n.strip_suffix(".json"))
            .unwrap_or(n)
            .to_ascii_lowercase()
    };
    let wanted = bare(name);
    FILES.iter().copied().find(|(file, _)| {
        let stem = bare(file);
        stem == wanted
            || ["reference/", "reference/examples/"]
                .iter()
                .any(|dir| stem.strip_prefix(dir).is_some_and(|s| s == wanted))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples_are_found_by_name_and_titled_by_their_design() {
        let (file, text) = find("strawberry").expect("strawberry");
        assert_eq!(file, "reference/examples/strawberry.json");
        assert_eq!(title(text), "Example: Strawberry");
        assert_eq!(find("examples").map(|f| f.0), Some("reference/examples.md"));
        assert_eq!(
            find("Composition.md").map(|f| f.0),
            Some("reference/composition.md")
        );
        assert!(find("reference/examples/mushroom.json").is_some());
        // Every example the index names is bundled, and every bundled example is in the index.
        let index = find("examples").expect("index").1;
        let bundled: Vec<&str> = FILES
            .iter()
            .filter_map(|(f, _)| f.strip_prefix("reference/examples/")?.strip_suffix(".json"))
            .collect();
        assert!(bundled.len() >= 8, "{bundled:?}");
        for name in &bundled {
            assert!(
                index.contains(&format!("| `{name}` |")),
                "{name} not in examples.md"
            );
        }
        let listed = index.matches("\n| `").count();
        assert_eq!(
            listed,
            bundled.len(),
            "examples.md lists an example not bundled"
        );
    }
}
