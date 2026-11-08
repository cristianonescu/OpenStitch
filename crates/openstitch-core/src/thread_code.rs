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

/// What a swatch shows beside or under it: the thread's code, or its `#rrggbb` when it has none.
pub fn thread_label(t: &Thread) -> String {
    // A PEC entry with no Brother code (62-64) says it is a PEC number.
    if let Some(i) = pec_chart_entry(t)
        && brother_embroidery_code(i).is_none()
    {
        return format!("PEC {i}");
    }
    thread_code(t).unwrap_or_else(|| t.hex())
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

    #[test]
    fn a_thread_shows_its_own_charts_code() {
        // A Brother/PEC thread: its Embroidery code, not its PEC number.
        let prussian = PEC_CHART[1].clone().expect("entry 1");
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
        // A custom colour: its hex; its PEC number is the nearest entry's.
        let custom = Thread::rgb(0x0E1F7D);
        assert_eq!(thread_label(&custom), "#0e1f7d");
        assert_eq!(thread_code(&custom), None);
        assert_eq!(pec_index(&custom), 1);
        // A thread with only a PEC entry's name and colour (an older file) is that entry.
        let mut named = Thread::rgb(prussian.rgb_value());
        named.name = prussian.name.clone();
        assert_eq!(thread_label(&named), "007");
    }
}
