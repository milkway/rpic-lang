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
/// hint — matched against the CSS keywords, lowercased.
pub fn suggest(s: &str) -> Option<&'static str> {
    crate::diagnostic::closest(&s.to_ascii_lowercase(), CSS_NAMED)
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
