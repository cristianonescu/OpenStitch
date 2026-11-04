# Test fonts

Small subsets of Noto fonts for the lettering tests that need scripts the bundled lettering fonts
(Lato, Pacifico) do not have (#345): `shaping.rs`
`right_to_left_scripts_with_the_bundled_test_fonts` and
`devanagari_vowel_sign_i_is_drawn_before_its_consonant`, `fonts.rs`
`cjk_text_with_the_bundled_test_font`. They are test fixtures only: never embedded in OpenStitch
binaries or offered as lettering fonts.

## Source

Downloaded 2026-11-04 from [google/fonts](https://github.com/google/fonts) at commit
`023eedd8780dd254e5905901473ed55e5475ee73`:

| Source file | SHA-256 of the downloaded file | Upstream |
|---|---|---|
| `ofl/notosanshebrew/NotoSansHebrew[wdth,wght].ttf` | `7ef36a2c3593758cdb622e1bdef4f84523e92fbc3ccc667438dd80ff54c2de88` | github.com/notofonts/hebrew |
| `ofl/notosansarabic/NotoSansArabic[wdth,wght].ttf` | `63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e` | github.com/notofonts/arabic |
| `ofl/notosansdevanagari/NotoSansDevanagari[wdth,wght].ttf` | `14ec4af41f27482216d1c2229f417ff9b1425e1babb014e57d1d40d03229853e` | github.com/notofonts/devanagari |
| `ofl/notosansjp/NotoSansJP[wght].ttf` | `c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f` | github.com/notofonts/noto-cjk |

## How they were made

`tools/test_font_subsets.py` with fontTools 4.60.1: the Regular instance (wght 400, wdth 100) of
each variable font, cut down to the characters the tests set, every OpenType layout feature kept
(Arabic joining, Indic reordering), hinting dropped, names updated to the Regular instance. The script gives the same bytes on every run.

| File | Characters kept | Bytes | SHA-256 |
|---|---|---|---|
| `NotoSansArabic-TestSubset.ttf` | a, b, space, U+0627 U+0628 U+0633 U+0644 U+0645 | 6932 | `2cb1fea9b7fc9a7603ca899c16dc456caea8ccc37252a53f435c4c9899d1e21b` |
| `NotoSansDevanagari-TestSubset.ttf` | a, b, space, U+0915 U+093F U+0937 U+094D | 5408 | `b6862c786ac9bede9f47125ae6e2880ab47c32e0bfb8ca104794948c1f4da19a` |
| `NotoSansHebrew-TestSubset.ttf` | a, b, space, U+05D5 U+05DC U+05DD U+05E9 | 3084 | `d928eae4490990af47417410b9d3d3f060e45dd2ee5722986a7c37daf35e8c6a` |
| `NotoSansJP-TestSubset.ttf` | a, b, space, U+4E2D U+6587 U+65E5 U+672C | 4768 | `25d997c6060c5bb9f6e2e2c0878efc97d75bb0f0a7cd1ac8d2e5b5ce1c7b4cce` |

## Licence

SIL Open Font License 1.1, each font's licence beside it: `NotoSansHebrew-OFL.txt`,
`NotoSansArabic-OFL.txt`, `NotoSansDevanagari-OFL.txt`, `NotoSansJP-OFL.txt` (copied unchanged
from the same google/fonts folders). A subset is a Modified Version under the OFL: it is
distributed under the same licence, with the copyright notices and licence kept (the fonts' own
`name` tables are kept whole), and never sold by itself. The Noto Sans Hebrew, Arabic and
Devanagari licences reserve no font name; Noto Sans JP's reserves "Source" (from Adobe's Source
Han Sans), which the subset's name, Noto Sans JP, does not use.
