//! The code printed on a thread's spool, as every colour list shows it (#798).
//!
//! A thread shows the catalogue code of its own chart: Brother's Embroidery code for a
//! Brother/PEC chart thread (PEC entry 1 is "007" Prussian Blue), the maker's code for Madeira,
//! Isacord, Sulky, Robison-Anton or New Brothread, the user's code for one of their charts. A
//! colour with no thread behind it shows its `#rrggbb`. The PEC chart number is a separate
//! field ([`pec_index`]), shown only where the user works in PEC numbers.

use crate::{PEC_CHART, Thread, brother_embroidery_code, nearest_index, pec_basic_chart};

/// The PEC chart entry `t` is, when it is one: brand `Brother`, the entry's number as its
/// catalogue number and the entry's RGB (what `PEC_CHART` holds), or no code but the entry's
/// name and RGB.
pub fn pec_chart_entry(t: &Thread) -> Option<usize> {
    let same = |i: usize| {
        PEC_CHART
            .get(i)
            .and_then(Option::as_ref)
            .filter(|c| c.rgb_value() == t.rgb_value())
            .map(|_| i)
    };
    match (t.brand.as_deref(), t.catalog_number.as_deref()) {
        (Some("Brother"), Some(code)) => code.trim().parse().ok().and_then(same),
        (None, None) => {
            let name = t.name.as_deref()?;
            (1..PEC_CHART.len())
                .find(|&i| PEC_CHART[i].as_ref().and_then(|c| c.name.as_deref()) == Some(name))
                .and_then(same)
        }
        _ => None,
    }
}

/// The PEC chart number a PES/PEC file gives `t`: its own entry when it is one, else the
/// nearest of entries 1–61 (as the PEC writer picks it).
pub fn pec_index(t: &Thread) -> usize {
    pec_chart_entry(t)
        .or_else(|| nearest_index(t, pec_basic_chart()))
        .unwrap_or(20)
}

/// The code on the thread's spool: Brother's Embroidery code for a PEC chart thread ("007";
/// the chart number for the three entries Brother's catalogue has no code for), else the thread's own
/// catalogue number. `None` for a colour with no code.
pub fn thread_code(t: &Thread) -> Option<String> {
    if let Some(i) = pec_chart_entry(t) {
        // 62-64 are not in Brother's catalogue: their chart number is their code.
        return Some(brother_embroidery_code(i).map_or_else(|| i.to_string(), str::to_string));
    }
    t.catalog_number
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_string)
}

/// The thread's own code as a swatch shows it ("PEC 62" for a PEC entry Brother's catalogue
/// has no code for), or `None` for a custom colour.
fn own_label(t: &Thread) -> Option<String> {
    // A PEC entry with no Brother code (62-64) says it is a PEC number.
    if let Some(i) = pec_chart_entry(t)
        && brother_embroidery_code(i).is_none()
    {
        return Some(format!("PEC {i}"));
    }
    thread_code(t)
}

/// Whether `t` is a custom colour: a colour with no code on a spool (an imported SVG's, a stitch
/// file's without thread data, one picked by value). It is named by its nearest thread (#1184).
pub fn is_custom(t: &Thread) -> bool {
    own_label(t).is_none()
}

/// The thread of `chart` nearest `t` by CIEDE2000, with the ΔE: what a custom colour is named
/// by (#1184). Ties go to the earlier thread. `None` for an empty chart.
pub fn nearest_thread<'a>(t: &Thread, chart: &'a [Thread]) -> Option<(&'a Thread, f64)> {
    chart
        .iter()
        .map(|c| (c, crate::colour::delta_e(t, c)))
        .fold(None, |best: Option<(&Thread, f64)>, (c, d)| match best {
            Some((_, bd)) if bd <= d => best,
            _ => Some((c, d)),
        })
}

/// The mark before a custom colour's nearest code in a swatch caption ("≈205", #1184).
pub const NEAREST_MARK: char = '≈';

/// What a swatch shows beside or under it (#798, #1184): the thread's code ("007", "PEC 62"),
/// or for a custom colour the code of the nearest thread of `chart` marked as nearest
/// ("≈205"); "custom" when there is no chart to name it by. Never a hex value.
pub fn thread_label(t: &Thread, chart: &[Thread]) -> String {
    if let Some(own) = own_label(t) {
        return own;
    }
    match nearest_thread(t, chart).and_then(|(n, _)| own_label(n)) {
        Some(code) => format!("{NEAREST_MARK}{code}"),
        None => "custom".into(),
    }
}

/// A thread as a person names it: brand, code and name ("Brother 205 Yellow"), the parts it
/// has. Empty for a colour with none of them.
pub fn thread_title(t: &Thread) -> String {
    let code = own_label(t);
    [thread_brand(t), code.as_deref(), t.name.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A colour as text for a person (#1184): a thread's brand, code and name, or for a custom
/// colour "Custom colour, nearest Brother 205 Yellow" (its own name first when it has one,
/// "Sky: custom colour, nearest …"). Never a hex value; the ΔE is for replies, not people.
pub fn thread_text(t: &Thread, chart: &[Thread]) -> String {
    if !is_custom(t) {
        return thread_title(t);
    }
    let custom = match nearest_thread(t, chart) {
        Some((n, _)) => format!("custom colour, nearest {}", thread_title(n)),
        None => "custom colour".into(),
    };
    match t.name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => format!("{name}: {custom}"),
        None => capitalised(&custom),
    }
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// The Brother chart entry `t` is, in either of the two forms a design stores it in: the PEC
/// chart's (brand "Brother", the PEC index as its number) or Brother's catalogue (brand
/// [`crate::BROTHER_EMBROIDERY_BRAND`], its catalogue code), with the chart's colour (#1197).
fn brother_entry(t: &Thread) -> Option<usize> {
    pec_chart_entry(t).or_else(|| {
        if t.brand.as_deref() != Some(crate::BROTHER_EMBROIDERY_BRAND) {
            return None;
        }
        let i = crate::pec_index_of_brother_code(t.catalog_number.as_deref()?.trim())?;
        PEC_CHART
            .get(i)
            .and_then(Option::as_ref)
            .filter(|c| c.rgb_value() == t.rgb_value())
            .map(|_| i)
    })
}

/// Whether `a` and `b` are the same spool of thread: equal, or the same Brother chart entry in
/// its two stored forms (#1197: "Red" and "Brother 800" sewed as two colour blocks with a colour
/// change between them).
pub fn same_thread(a: &Thread, b: &Thread) -> bool {
    a == b || matches!((brother_entry(a), brother_entry(b)), (Some(x), Some(y)) if x == y)
}

/// The thread's brand as a person reads it: "Brother" for a PEC chart thread and for Brother's
/// Embroidery catalogue, else the brand it carries.
pub fn thread_brand(t: &Thread) -> Option<&str> {
    if pec_chart_entry(t).is_some() || t.brand.as_deref() == Some(crate::BROTHER_EMBROIDERY_BRAND) {
        return Some("Brother");
    }
    t.brand.as_deref().filter(|b| !b.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1197: the PEC chart's form and Brother's catalogue form of one thread are the same
    /// thread; another colour, or another maker's code, is not.
    #[test]
    fn a_brother_thread_in_both_forms_is_one_thread() {
        let pec = PEC_CHART[5].clone().expect("entry 5, red");
        let mut catalogue = pec.clone();
        catalogue.brand = Some(crate::BROTHER_EMBROIDERY_BRAND.into());
        catalogue.catalog_number = Some("800".into());
        assert_ne!(pec, catalogue);
        assert!(same_thread(&pec, &catalogue) && same_thread(&catalogue, &pec));
        // The code shown (the Sewing Order list's label) is the chart code for both (#1184).
        assert_eq!(thread_label(&pec, &[]), "800");
        assert_eq!(thread_label(&catalogue, &[]), "800");
        assert_eq!(thread_brand(&pec), thread_brand(&catalogue));
        let mut other_code = catalogue.clone();
        other_code.catalog_number = Some("405".into());
        assert!(!same_thread(&pec, &other_code));
        let mut other_colour = catalogue.clone();
        other_colour.red = other_colour.red.wrapping_add(1);
        assert!(!same_thread(&pec, &other_colour));
        let mut maker = catalogue.clone();
        maker.brand = Some("New Brothread".into());
        assert!(!same_thread(&pec, &maker));
    }

    #[test]
    fn a_thread_shows_its_own_charts_code() {
        // A Brother/PEC thread: its Embroidery code, not its PEC number.
        let prussian = PEC_CHART[1].clone().expect("entry 1");
        let thread_label = |t: &Thread| super::thread_label(t, &[]);
        assert_eq!(thread_label(&prussian), "007");
        assert_eq!(pec_index(&prussian), 1);
        assert_eq!(thread_brand(&prussian), Some("Brother"));
        // The same thread as Brother's catalogue gives it (brand EMBROIDERY, code 007).
        let mut catalogue = prussian.clone();
        catalogue.catalog_number = Some("007".into());
        catalogue.brand = Some(crate::BROTHER_EMBROIDERY_BRAND.into());
        assert_eq!(thread_label(&catalogue), "007");
        assert_eq!(thread_brand(&catalogue), Some("Brother"));
        // Entries 62-64 have no Brother code.
        let p62 = PEC_CHART[62].clone().expect("entry 62");
        assert_eq!(thread_label(&p62), "PEC 62");
        assert_eq!(thread_code(&p62).as_deref(), Some("62"));
        // Another maker's thread: its own code.
        let madeira = crate::brands::brand_charts()
            .into_iter()
            .find(|c| c.0 == "madeiraPolyneon")
            .expect("Madeira")
            .2[0]
            .clone();
        assert_eq!(
            thread_label(&madeira),
            madeira.catalog_number.as_deref().expect("a code")
        );
        assert_eq!(thread_brand(&madeira), Some("Madeira Polyneon"));
        let nb = &crate::brands::NEW_BROTHREAD[0];
        assert_eq!(thread_label(nb), "001");
        // A user chart's thread: the user's code.
        let mut mine = Thread::rgb(0x123456);
        mine.catalog_number = Some("A7".into());
        mine.brand = Some("Mine".into());
        assert_eq!(thread_label(&mine), "A7");
        // A custom colour with no chart to name it by: "custom", never its hex; its PEC number
        // is the nearest entry's.
        let custom = Thread::rgb(0x0E1F7D);
        assert_eq!(thread_label(&custom), "custom");
        assert_eq!(thread_code(&custom), None);
        assert_eq!(pec_index(&custom), 1);
        // A thread with only a PEC entry's name and colour (an older file) is that entry.
        let mut named = Thread::rgb(prussian.rgb_value());
        named.name = prussian.name.clone();
        assert_eq!(thread_label(&named), "007");
    }

    #[test]
    fn a_custom_colour_is_named_by_its_nearest_thread_never_by_hex() {
        // #1184: New Brothread's 205 is the nearest of the kit to a yellow a little off it.
        let kit: &[Thread] = &crate::brands::NEW_BROTHREAD;
        let yellow = kit
            .iter()
            .find(|t| t.catalog_number.as_deref() == Some("205"))
            .expect("205 in the kit");
        let off = Thread::rgb(yellow.rgb_value() ^ 0x000203);
        assert!(is_custom(&off));
        let (near, de) = nearest_thread(&off, kit).expect("a nearest thread");
        assert_eq!(near.catalog_number.as_deref(), Some("205"));
        assert!(de > 0.0 && de < 2.0, "ΔE {de}");
        assert_eq!(thread_label(&off, kit), "≈205");
        let title = thread_title(yellow);
        assert_eq!(title, "New Brothread 205 Yellow");
        assert_eq!(thread_text(yellow, kit), title);
        assert_eq!(
            thread_text(&off, kit),
            format!("Custom colour, nearest {title}")
        );
        // Its own name first when it has one; no chart leaves only "custom colour".
        let sky = off.clone().with_name("Sky");
        assert_eq!(
            thread_text(&sky, kit),
            format!("Sky: custom colour, nearest {title}")
        );
        assert_eq!(thread_text(&off, &[]), "Custom colour");
        for s in [
            thread_label(&off, kit),
            thread_text(&off, kit),
            thread_text(&off, &[]),
        ] {
            assert!(!s.contains('#'), "{s}");
        }
    }
}
