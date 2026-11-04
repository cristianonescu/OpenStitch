//! The bundled embroidery skill (`skills/openstitch-embroidery/` at the repository root),
//! compiled in and served by the `--assistant` tool `skill_reference`.

/// `(name, text)`, in the order `skill_reference` lists them. `SKILL.md` comes first.
pub const FILES: &[(&str, &str)] = &[
    (
        "SKILL.md",
        include_str!("../../../skills/openstitch-embroidery/SKILL.md"),
    ),
    (
        "reference/answers.md",
        include_str!("../../../skills/openstitch-embroidery/reference/answers.md"),
    ),
    (
        "reference/recipes.md",
        include_str!("../../../skills/openstitch-embroidery/reference/recipes.md"),
    ),
    (
        "reference/sewability.md",
        include_str!("../../../skills/openstitch-embroidery/reference/sewability.md"),
    ),
];

/// A file's first Markdown heading, without the `#`s (skipping any front matter).
pub fn title(text: &str) -> &str {
    text.lines()
        .find_map(|line| line.strip_prefix('#'))
        .map(|rest| rest.trim_start_matches('#').trim())
        .unwrap_or("")
}

/// A file by its name, also without `.md` or the `reference/` folder (`recipes`).
pub fn find(name: &str) -> Option<(&'static str, &'static str)> {
    let name = name.trim().trim_start_matches("./");
    let stem = name.strip_suffix(".md").unwrap_or(name);
    FILES.iter().copied().find(|(file, _)| {
        let file_stem = file.strip_suffix(".md").unwrap_or(file);
        file_stem.eq_ignore_ascii_case(stem)
            || file_stem
                .strip_prefix("reference/")
                .is_some_and(|s| s.eq_ignore_ascii_case(stem))
    })
}
