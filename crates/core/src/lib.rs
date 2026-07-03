//! rpic-core — engine for the `pic` picture-drawing language.
//!
//! Pipeline: source → [`lexer`] → [`parser`] → [`ast`] → [`eval`] → placed
//! primitives ([`ir`]) → render backends ([`svg`], later PNG/PDF). This crate
//! is pure (no file I/O); the CLI and WASM wrappers drive it.
//!
//! Note: [`eval`] is the pic-language *interpreter* — a safe walker over a typed
//! expression/geometry tree. It executes no arbitrary code.

pub mod ast;
pub mod diagnostic;
pub mod eval;
pub mod geom;
pub mod ir;
pub mod lexer;
mod math;
pub mod parser;
pub mod svg;
pub mod token;

pub use diagnostic::{Diagnostic, Span};
pub use eval::{EvalError, eval};
pub use ir::Drawing;
pub use lexer::{LexError, lex};
pub use math::{MathSpan, set_math_renderer};
pub use parser::{ParseError, parse, parse_in_dir};
pub use svg::to_svg;
pub use token::Token;

/// Bundled native circuit-element library (the `define` dialect). Prepend it to
/// source to use `resistor`, `capacitor`, … See `std/circuits.pic`.
pub const CIRCUITS: &str = include_str!("std/circuits.pic");

/// Compile pic source into a placed-primitive [`Drawing`].
pub fn compile(src: &str) -> Result<Drawing, String> {
    compile_in_dir(src, None)
}

/// Compile pic source, resolving `copy "file"` includes relative to `base`.
pub fn compile_in_dir(src: &str, base: Option<&std::path::Path>) -> Result<Drawing, String> {
    let picture = parser::parse_in_dir(src, base).map_err(|e| e.to_string())?;
    eval(&picture).map_err(|e| e.to_string())
}

/// Compile pic source directly to an SVG string.
pub fn render_svg(src: &str) -> Result<String, String> {
    Ok(to_svg(&compile(src)?))
}

/// Render pic source to SVG, resolving `copy "file"` includes relative to `base`.
pub fn render_svg_in_dir(src: &str, base: Option<&std::path::Path>) -> Result<String, String> {
    Ok(to_svg(&compile_in_dir(src, base)?))
}

/// Build the JSON animation manifest array (`[{id,effect,start,duration},…]`)
/// for an already-compiled drawing.
pub fn animations_json(d: &Drawing) -> String {
    let mut s = String::from("[");
    for (i, a) in d.anims.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"id\":\"s{}\",\"effect\":\"{}\",\"start\":{},\"duration\":{}}}",
            a.shape,
            json_str(&a.effect),
            a.start,
            a.duration
        ));
    }
    s.push(']');
    s
}

/// Build the JSON diagnostic array emitted by pic `print` statements.
pub fn diagnostics_json(d: &Drawing) -> String {
    let mut s = String::from("[");
    for (i, line) in d.diagnostics.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push('"');
        s.push_str(&json_str(line));
        s.push('"');
    }
    s.push(']');
    s
}

/// Compile to a single JSON object `{ "svg": "...", "animations": [...],
/// "diagnostics": [...], "warnings": [...] }`, or `{ "error": "...",
/// "error_info": { ... } }` on failure. The flat `error` string is kept for
/// backward compatibility with older bindings.
pub fn compile_json(src: &str) -> String {
    compile_json_impl(src, None)
}

/// Compile to a JSON bundle, resolving `copy "file"` includes relative to
/// `base`.
pub fn compile_json_in_dir(src: &str, base: Option<&std::path::Path>) -> String {
    compile_json_impl(src, base)
}

fn compile_json_impl(src: &str, base: Option<&std::path::Path>) -> String {
    let picture = match parser::parse_in_dir(src, base) {
        Ok(p) => p,
        Err(e) => return error_json(&e.to_string(), &e.diagnostic()),
    };
    match eval(&picture) {
        Ok(d) => drawing_json(&d),
        Err(e) => {
            let msg = e.to_string();
            let diagnostic = eval_error_diagnostic(src, &msg);
            error_json(&msg, &diagnostic)
        }
    }
}

fn drawing_json(d: &Drawing) -> String {
    format!(
        "{{\"svg\":\"{}\",\"animations\":{},\"diagnostics\":{},\"warnings\":{}}}",
        json_str(&to_svg(d)),
        animations_json(d),
        diagnostics_json(d),
        diagnostics_json_structured(&d.warnings)
    )
}

fn error_json(message: &str, diagnostic: &Diagnostic) -> String {
    format!(
        "{{\"error\":\"{}\",\"error_info\":{},\"warnings\":[]}}",
        json_str(message),
        diagnostic_json(diagnostic)
    )
}

fn diagnostics_json_structured(items: &[Diagnostic]) -> String {
    let mut s = String::from("[");
    for (i, d) in items.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&diagnostic_json(d));
    }
    s.push(']');
    s
}

fn diagnostic_json(d: &Diagnostic) -> String {
    let mut s = String::from("{");
    s.push_str(&format!("\"message\":\"{}\"", json_str(&d.message)));
    s.push_str(&format!(",\"line\":{}", json_opt_u32(d.line)));
    s.push_str(&format!(",\"col\":{}", json_opt_u32(d.col)));
    s.push_str(&format!(",\"end_col\":{}", json_opt_u32(d.end_col)));
    s.push_str(&format!(",\"kind\":\"{}\"", json_str(&d.kind)));
    s.push_str(&format!(",\"found\":{}", json_opt_str(d.found.as_deref())));
    s.push_str(&format!(
        ",\"expected\":{}",
        json_opt_str(d.expected.as_deref())
    ));
    s.push_str(&format!(",\"hint\":{}", json_opt_str(d.hint.as_deref())));
    s.push('}');
    s
}

fn json_opt_u32(v: Option<u32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "null".into())
}

fn json_opt_str(v: Option<&str>) -> String {
    v.map(|s| format!("\"{}\"", json_str(s)))
        .unwrap_or_else(|| "null".into())
}

fn eval_error_diagnostic(src: &str, msg: &str) -> Diagnostic {
    if let Some(name) = msg
        .strip_prefix("unknown label `")
        .and_then(|rest| rest.strip_suffix('`'))
    {
        let mut d = Diagnostic::new("unknown_label", msg.to_string()).found(name);
        if let Some(span) = find_name_ref(src, name) {
            d = d.at(span);
        }
        return d;
    }
    if msg.starts_with("ordinal ") && msg.contains(" out of range") {
        let mut d = Diagnostic::new("ordinal_out_of_range", msg.to_string());
        if let Some(found) = msg
            .strip_prefix("ordinal ")
            .and_then(|rest| rest.split_once(" out of range"))
            .map(|(found, _)| found)
        {
            d = d.found(found);
        }
        if let Some(available) = msg
            .split_once("(available ")
            .and_then(|(_, rest)| rest.strip_suffix(')'))
        {
            d = d.expected(format!("1..{available}"));
        }
        if let Some(span) = find_ordinal_ref(src) {
            d = d.at(span);
        }
        return d;
    }
    Diagnostic::new("eval", msg.to_string())
}

fn find_name_ref(src: &str, name: &str) -> Option<Span> {
    let toks = lexer::lex(src).ok()?;
    for (i, s) in toks.iter().enumerate() {
        let matches_name = match &s.tok {
            token::Token::Name(n) | token::Token::Label(n) => n == name,
            _ => false,
        };
        if matches_name && !matches!(toks.get(i + 1).map(|n| &n.tok), Some(token::Token::Colon)) {
            return Some(s.span());
        }
    }
    None
}

fn find_ordinal_ref(src: &str) -> Option<Span> {
    let toks = lexer::lex(src).ok()?;
    for w in toks.windows(2) {
        if matches!(w[0].tok, token::Token::Float(_))
            && matches!(w[1].tok, token::Token::Kw(token::Kw::Nth))
        {
            return Some(Span::new(w[0].line, w[0].col, w[1].end_col));
        }
    }
    toks.iter()
        .find(|s| matches!(s.tok, token::Token::Kw(token::Kw::Last)))
        .map(|s| s.span())
}

/// Escape a string for embedding inside a JSON string literal.
fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_bundles_svg_and_animations() {
        let j = compile_json("box\nanimate last box with \"fade\"");
        assert!(j.starts_with("{\"svg\":\"<svg"));
        assert!(j.contains("<g id=\\\"s0\\\">")); // stable id, JSON-escaped
        assert!(j.contains("\"animations\":[{\"id\":\"s0\",\"effect\":\"fade\""));
        assert!(j.contains("\"diagnostics\":[]"));
    }

    #[test]
    fn json_reports_errors() {
        let j = compile_json("copy \"oops\"");
        assert!(j.contains("\"error\""));
        assert!(j.contains("\"error_info\""));
    }

    #[test]
    fn json_reports_print_diagnostics() {
        let j = compile_json("print \"hi\"\nprint 2+3");
        assert!(j.contains("\"diagnostics\":[\"hi\",\"5\"]"));
    }

    #[test]
    fn json_in_dir_resolves_copy_includes() {
        let dir = std::env::temp_dir().join(format!("rpic_json_copy_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("inc.pic"), "box wid 0.5 ht 0.5\n").unwrap();

        let j = compile_json_in_dir("copy \"inc.pic\"\ncircle", Some(dir.as_path()));
        let _ = std::fs::remove_dir_all(&dir);

        assert!(j.starts_with("{\"svg\":\"<svg"), "{j}");
        assert!(j.contains("<rect"), "{j}");
        assert!(j.contains("<circle"), "{j}");
        assert!(j.contains("\"diagnostics\":[]"), "{j}");
        assert!(!j.contains("\"error\""), "{j}");
    }

    #[test]
    fn json_error_info_uses_user_facing_tokens_and_spans() {
        let j = compile_json("bxo\n");

        assert!(
            j.contains("\"error\":\"1:1: expected an object, found `bxo`\""),
            "{j}"
        );
        assert!(j.contains("\"kind\":\"expected_token\""), "{j}");
        assert!(j.contains("\"line\":1"), "{j}");
        assert!(j.contains("\"col\":1"), "{j}");
        assert!(j.contains("\"end_col\":4"), "{j}");
        assert!(j.contains("\"found\":\"`bxo`\""), "{j}");
        assert!(j.contains("\"expected\":\"an object\""), "{j}");
        assert!(j.contains("\"hint\":\"did you mean `box`?\""), "{j}");
    }

    #[test]
    fn json_unterminated_string_points_at_opening_quote() {
        let j = compile_json("box wid 1\n  \"oops\n");

        assert!(j.contains("\"kind\":\"unterminated_string\""), "{j}");
        assert!(j.contains("\"line\":2"), "{j}");
        assert!(j.contains("\"col\":3"), "{j}");
    }

    #[test]
    fn json_eval_errors_get_structured_locations_when_possible() {
        let label = compile_json("box at A\n");
        assert!(label.contains("\"kind\":\"unknown_label\""), "{label}");
        assert!(label.contains("\"line\":1"), "{label}");
        assert!(label.contains("\"col\":8"), "{label}");
        assert!(label.contains("\"found\":\"A\""), "{label}");

        let ordinal = compile_json("box\nbox at 3rd box\n");
        assert!(
            ordinal.contains("\"kind\":\"ordinal_out_of_range\""),
            "{ordinal}"
        );
        assert!(ordinal.contains("\"line\":2"), "{ordinal}");
        assert!(ordinal.contains("\"col\":8"), "{ordinal}");
        assert!(ordinal.contains("\"found\":\"3\""), "{ordinal}");
        assert!(ordinal.contains("\"expected\":\"1..1\""), "{ordinal}");
    }

    #[test]
    fn json_success_bundle_reports_warnings() {
        let attr = compile_json("box \"a\" dashd\n");
        assert!(attr.contains("\"warnings\":[{"), "{attr}");
        assert!(attr.contains("\"kind\":\"ignored_attribute\""), "{attr}");
        assert!(attr.contains("\"found\":\"dashd\""), "{attr}");
        assert!(
            attr.contains("\"hint\":\"did you mean `dashed`?\""),
            "{attr}"
        );

        let anim = compile_json("box\nanimate 1st box with \"zoom\"\n");
        assert!(
            anim.contains("\"kind\":\"unknown_animation_effect\""),
            "{anim}"
        );
        assert!(anim.contains("\"found\":\"zoom\""), "{anim}");
        assert!(anim.contains("\"line\":2"), "{anim}");
    }

    #[test]
    fn circuits_library_compiles_and_draws() {
        let src = format!(
            "{}\nA:(0,0); B:(1.6,0); C:(3,0)\nresistor(A,B)\ncapacitor(A,B)\ninductor(A,B)\n\
             diode(A,B)\nbattery(A,B)\nground(A)\ndot(A)\nwire(A,B)\n\
             and_gate(C)\nor_gate(C)\nnand_gate(C)\nnor_gate(C)\nxor_gate(C)\n\
             xnor_gate(C)\nbuffer(C)\nnot_gate(C)\n\
             opamp(C)\nnpn(C)\npnp(C)\nac_source(A,B)\nswitch(A,B)\n\
             potentiometer(A,B)\ntransformer(C)\n\
             nmos(C)\npmos(C)\ncurrent_source(A,B)\nvsource_ctrl(A,B)\n\
             isource_ctrl(A,B)\nled(A,B)\nphotodiode(A,B)\nzener(A,B)\n\
             clabel(A,\"x\")\nterminal(A)\nvdd(A)\nantenna(A)\ncurrent(A,B)\n\
             voltage(A,B)\nlamp(A,B)\nfuse(A,B)\n\
             voltmeter(A,B)\nammeter(A,B)\nohmmeter(A,B)\nmeter(A,B,\"X\")\n\
             crystal(A,B)\nspeaker(A,B)\nhop(A,B)\nspdt(A,B,C)\n\
             iec_resistor(A,B)\nvoltage_source(A,B)\npushbutton(A,B)\nrelay(A,B)\n\
             thermistor(A,B)\nmotor(A,B)\ngenerator(A,B)\nbell(A,B)\n\
             ieee_and(C)\nieee_or(C)\nieee_xor(C)\nieee_buf(C)\niecgate(C,\"x\")\n\
             varistor(A,B)\ntline(A,B)\nbus(A,B)\n\
             polcap(A,B)\nvarcap(A,B)\nvarind(A,B)\nschottky(A,B)\nldr(A,B)\n\
             spark_gap(A,B)\nchassis_ground(A)\nsignal_ground(A)\n\
             njfet(C)\npjfet(C)\nphototransistor(C)\nic_block(C,\"x\")\nmux(C)\n\
             solar_cell(A,B)\nmicrophone(A,B)\nthermocouple(A,B)",
            CIRCUITS
        );
        let d = compile(&src).expect("circuit library should compile");
        assert!(!d.shapes.is_empty());
    }
}
