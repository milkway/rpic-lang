//! Colour-name validation. rpic passes a resolved colour string straight to
//! the SVG backend; this module decides whether that string is a colour any
//! SVG renderer will understand, so an unknown name (a typo, a mis-cased
//! keyword) can be flagged with a warning instead of silently producing blank
//! ink. It does **not** map or alter colours — only classifies them.
//!
//! Accepted forms: `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa` hex, the CSS
//! functional notations (`rgb()`/`rgba()`/`hsl()`/`hsla()`/`color()`), the
//! CSS Color Module Level 4 named colours (case-insensitive), the special
//! keywords `none`/`transparent`/`currentColor`, and the dvips/xcolor named
//! colours (`Dandelion`, `Goldenrod`, …) that the dpic corpus carries.

/// The 148 CSS Color Module Level 4 extended colour keywords, lowercase.
static CSS_NAMED: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

/// The 68 dvips/xcolor named colours (as written — CamelCase). The dpic corpus
/// and circuit_macros figures use these (`shaded "Dandelion"`). Kept
/// case-sensitively: they are distinct keywords, not CSS names.
static XCOLOR_NAMED: &[&str] = &[
    "Apricot",
    "Aquamarine",
    "Bittersweet",
    "Black",
    "Blue",
    "BlueGreen",
    "BlueViolet",
    "BrickRed",
    "Brown",
    "BurntOrange",
    "CadetBlue",
    "CarnationPink",
    "Cerulean",
    "CornflowerBlue",
    "Cyan",
    "Dandelion",
    "DarkOrchid",
    "Emerald",
    "ForestGreen",
    "Fuchsia",
    "Goldenrod",
    "Gray",
    "Green",
    "GreenYellow",
    "JungleGreen",
    "Lavender",
    "LimeGreen",
    "Magenta",
    "Mahogany",
    "Maroon",
    "Melon",
    "MidnightBlue",
    "Mulberry",
    "NavyBlue",
    "OliveGreen",
    "Orange",
    "OrangeRed",
    "Orchid",
    "Peach",
    "Periwinkle",
    "PineGreen",
    "Plum",
    "ProcessBlue",
    "Purple",
    "RawSienna",
    "Red",
    "RedOrange",
    "RedViolet",
    "Rhodamine",
    "RoyalBlue",
    "RoyalPurple",
    "RubineRed",
    "Salmon",
    "SeaGreen",
    "Sepia",
    "SkyBlue",
    "SpringGreen",
    "Tan",
    "TealBlue",
    "Thistle",
    "Turquoise",
    "Violet",
    "VioletRed",
    "White",
    "WildStrawberry",
    "Yellow",
    "YellowGreen",
    "YellowOrange",
];

/// The dvips names browsers can't render, mapped to their RGB — derived from
/// `dvipsnam.def`'s cmyk values (channel = 1 − min(1, c + k)); `Dandelion`
/// checks out against man19.pic's own comment (`1, 0.71, 0.16` → `#ffb529`).
/// Deliberately excludes the xcolor names that are *also* CSS keywords
/// (`Goldenrod`, `Plum`, …): those render natively, case-insensitively, and
/// remapping them to the (different!) dvips values would change figures that
/// already display correctly. Sorted for binary search.
static XCOLOR_HEX: &[(&str, &str)] = &[
    ("Apricot", "#ffad7a"),
    ("Bittersweet", "#c20300"),
    ("BlueGreen", "#26ffab"),
    ("BrickRed", "#b80000"),
    ("BurntOrange", "#ff7d00"),
    ("CarnationPink", "#ff5eff"),
    ("Cerulean", "#0fe3ff"),
    ("Dandelion", "#ffb529"),
    ("Emerald", "#00ff80"),
    ("JungleGreen", "#03ff7a"),
    ("Mahogany", "#a60000"),
    ("Melon", "#ff8a80"),
    ("Mulberry", "#a314fa"),
    ("NavyBlue", "#0f75ff"),
    ("OliveGreen", "#009900"),
    ("Peach", "#ff804d"),
    ("Periwinkle", "#6e73ff"),
    ("PineGreen", "#00bf29"),
    ("ProcessBlue", "#0affff"),
    ("RawSienna", "#8c0000"),
    ("RedOrange", "#ff3b21"),
    ("RedViolet", "#9600a8"),
    ("Rhodamine", "#ff2eff"),
    ("RoyalPurple", "#4019ff"),
    ("RubineRed", "#ff00de"),
    ("Sepia", "#4d0000"),
    ("TealBlue", "#1ffaa3"),
    ("VioletRed", "#ff30ff"),
    ("WildStrawberry", "#ff0a9c"),
    ("YellowOrange", "#ff9400"),
];

/// The hex for a dvips/xcolor name **no browser understands** (`Dandelion` →
/// `#ffb529`), or `None` for everything else — including the xcolor names that
/// are also CSS keywords, which must stay untouched.
pub fn xcolor_hex(name: &str) -> Option<&'static str> {
    XCOLOR_HEX
        .binary_search_by_key(&name, |(n, _)| n)
        .ok()
        .map(|i| XCOLOR_HEX[i].1)
}

/// Is `s` a colour an SVG renderer will understand? Used to warn on an unknown
/// colour name; a `false` result never blocks rendering, it only flags.
pub fn is_valid_color(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    if is_hex(t) || is_functional(t) {
        return true;
    }
    // `none`/`transparent`/`currentColor` are valid SVG paint keywords.
    if t.eq_ignore_ascii_case("none")
        || t.eq_ignore_ascii_case("transparent")
        || t.eq_ignore_ascii_case("currentcolor")
    {
        return true;
    }
    let lower = t.to_ascii_lowercase();
    CSS_NAMED.binary_search(&lower.as_str()).is_ok() || XCOLOR_NAMED.contains(&t)
}

/// Nearest known colour name to `s` (edit distance ≤ 2) for a "did you mean"
/// hint. Tries the CSS keywords (case-insensitively) then the dvips/xcolor
/// names (case-sensitive CamelCase) — so `"Dandelio"` suggests `Dandelion`.
pub fn suggest(s: &str) -> Option<&'static str> {
    crate::diagnostic::closest(&s.to_ascii_lowercase(), CSS_NAMED)
        .or_else(|| crate::diagnostic::closest(s, XCOLOR_NAMED))
}

/// `#` followed by exactly 3, 4, 6, or 8 hex digits.
fn is_hex(s: &str) -> bool {
    let Some(hex) = s.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A CSS functional colour: `rgb(`/`rgba(`/`hsl(`/`hsla(`/`color(` … `)`. The
/// components aren't validated — rpic's own `rgb()` literal already resolves to
/// hex, so this only accepts strings a user passed through verbatim.
fn is_functional(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    ["rgb(", "rgba(", "hsl(", "hsla(", "color("]
        .iter()
        .any(|p| lower.starts_with(p))
        && lower.ends_with(')')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_list_is_sorted_for_binary_search() {
        let mut sorted = CSS_NAMED.to_vec();
        sorted.sort_unstable();
        assert_eq!(CSS_NAMED, sorted.as_slice(), "CSS_NAMED must stay sorted");
    }

    #[test]
    fn xcolor_hex_table_is_sorted_and_consistent() {
        let mut sorted = XCOLOR_HEX.to_vec();
        sorted.sort_unstable_by_key(|(n, _)| *n);
        assert_eq!(XCOLOR_HEX, sorted.as_slice(), "XCOLOR_HEX must stay sorted");
        for (name, hex) in XCOLOR_HEX {
            // every mapped name is a recognised xcolor name …
            assert!(XCOLOR_NAMED.contains(name), "{name} not in XCOLOR_NAMED");
            // … that is NOT also a CSS keyword (those must stay unmapped) …
            assert!(
                CSS_NAMED
                    .binary_search(&name.to_ascii_lowercase().as_str())
                    .is_err(),
                "{name} is a CSS keyword and must not be remapped"
            );
            // … and maps to well-formed hex
            assert!(is_hex(hex), "bad hex for {name}: {hex}");
        }
    }

    #[test]
    fn every_valid_xcolor_name_actually_renders() {
        // `is_valid_color` accepts every XCOLOR_NAMED entry (so it doesn't
        // warn), so each MUST render to real paint — either it's also a CSS
        // keyword the browser knows, or we remap it to hex. A future name that
        // is neither would validate yet paint nothing (the silent-blank-ink
        // failure this module exists to prevent).
        for name in XCOLOR_NAMED {
            let is_css = CSS_NAMED
                .binary_search(&name.to_ascii_lowercase().as_str())
                .is_ok();
            assert!(
                is_css || xcolor_hex(name).is_some(),
                "{name} is a valid xcolor name but neither a CSS keyword nor remapped to hex — it would render blank"
            );
        }
    }

    #[test]
    fn xcolor_hex_maps_non_css_names_only() {
        // dvipsnam.def: Dandelion = cmyk(0,.29,.84,0) -> rgb(1,.71,.16)
        assert_eq!(xcolor_hex("Dandelion"), Some("#ffb529"));
        // Goldenrod IS a CSS keyword — browsers render it; stays untouched
        assert_eq!(xcolor_hex("Goldenrod"), None);
        // case matters: the CSS name `dandelion` doesn't exist, and the
        // xcolor spelling is CamelCase — lowercase input is not remapped
        assert_eq!(xcolor_hex("dandelion"), None);
        assert_eq!(xcolor_hex("notacolor"), None);
    }

    #[test]
    fn suggest_covers_css_and_xcolor() {
        assert_eq!(suggest("crimsom"), Some("crimson")); // CSS typo
        assert_eq!(suggest("Dandelio"), Some("Dandelion")); // xcolor typo (#291)
        assert_eq!(suggest("zzzzzzzz"), None); // nothing close
    }

    #[test]
    fn accepts_valid_forms() {
        for c in [
            "red",
            "Red",
            "REBECCAPURPLE",
            "cornflowerblue",
            "#1b5e20",
            "#abc",
            "#12345678",
            "#abcd",
            "rgb(1,2,3)",
            "rgba(1,2,3,0.5)",
            "hsl(120, 50%, 50%)",
            "none",
            "transparent",
            "currentColor",
            "Dandelion",
            "Goldenrod",
        ] {
            assert!(is_valid_color(c), "should be valid: {c}");
        }
    }

    #[test]
    fn rejects_invalid_forms() {
        for c in [
            "notacolor",
            "crimsom",
            "#12g456",
            "#ab",
            "#abcde",
            "0xff0000",
            "dandelion",
            "",
            "rgb(1,2,3",
        ] {
            assert!(!is_valid_color(c), "should be invalid: {c}");
        }
    }
}
