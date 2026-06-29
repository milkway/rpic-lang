//! rpic-core — engine for the `pic` picture-drawing language.
//!
//! Pipeline: source → [`lexer`] → [`parser`] → [`ast`] → [`eval`] → placed
//! primitives ([`ir`]) → render backends ([`svg`], later PNG/PDF). This crate
//! is pure (no file I/O); the CLI and WASM wrappers drive it.
//!
//! Note: [`eval`] is the pic-language *interpreter* — a safe walker over a typed
//! expression/geometry tree. It executes no arbitrary code.

pub mod ast;
pub mod eval;
pub mod geom;
pub mod ir;
pub mod lexer;
pub mod parser;
pub mod svg;
pub mod token;

pub use eval::{EvalError, eval};
pub use ir::Drawing;
pub use lexer::{LexError, lex};
pub use parser::{ParseError, parse};
pub use svg::to_svg;
pub use token::Token;

/// Bundled native circuit-element library (the `define` dialect). Prepend it to
/// source to use `resistor`, `capacitor`, … See `std/circuits.pic`.
pub const CIRCUITS: &str = include_str!("std/circuits.pic");

/// Compile pic source into a placed-primitive [`Drawing`].
pub fn compile(src: &str) -> Result<Drawing, String> {
    let picture = parse(src).map_err(|e| e.to_string())?;
    eval(&picture).map_err(|e| e.to_string())
}

/// Compile pic source directly to an SVG string.
pub fn render_svg(src: &str) -> Result<String, String> {
    Ok(to_svg(&compile(src)?))
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

/// Compile to a single JSON object `{ "svg": "...", "animations": [...] }`, or
/// `{ "error": "..." }` on failure. This is the entry point the WASM wrapper and
/// browser playground consume.
pub fn compile_json(src: &str) -> String {
    match compile(src) {
        Ok(d) => format!(
            "{{\"svg\":\"{}\",\"animations\":{}}}",
            json_str(&to_svg(&d)),
            animations_json(&d)
        ),
        Err(e) => format!("{{\"error\":\"{}\"}}", json_str(&e)),
    }
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
    }

    #[test]
    fn json_reports_errors() {
        let j = compile_json("copy \"oops\"");
        assert!(j.contains("\"error\""));
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
