//! A tiny JSON writer that matches Apple's JSONSerialization output (R-API-6): 2-space pretty
//! printing with `"key" : value`, `/` escaped as `\/`, and integral numbers without a decimal point.

/// A JSON value for the API and CLI formatters.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number; integral values print without a decimal point.
    Num(f64),
    /// A string, escaped on output.
    Str(String),
    /// An array, one element per line when pretty.
    Arr(Vec<JsonValue>),
    /// An object as ordered pairs; sorted by key on output when asked.
    Obj(Vec<(String, JsonValue)>),
}

/// Serialises `v` like Apple's JSONSerialization. `pretty` gives 2-space indents and `"k" : v`;
/// `sort_keys` orders object keys bytewise. Compact output has no spaces at all.
pub fn to_apple_json(v: &JsonValue, pretty: bool, sort_keys: bool) -> String {
    let mut out = String::new();
    write_value(v, pretty, sort_keys, 0, &mut out);
    out
}

/// Writes one value at nesting depth `depth`.
fn write_value(v: &JsonValue, pretty: bool, sort_keys: bool, depth: usize, out: &mut String) {
    match v {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonValue::Num(n) => out.push_str(&number(*n)),
        JsonValue::Str(s) => write_str(s, out),
        JsonValue::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(pretty, depth + 1, out);
                write_value(item, pretty, sort_keys, depth + 1, out);
            }
            newline(pretty, depth, out);
            out.push(']');
        }
        JsonValue::Obj(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut refs: Vec<&(String, JsonValue)> = pairs.iter().collect();
            if sort_keys {
                refs.sort_by(|a, b| a.0.cmp(&b.0));
            }
            out.push('{');
            for (i, (key, value)) in refs.into_iter().map(|p| (&p.0, &p.1)).enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(pretty, depth + 1, out);
                write_str(key, out);
                out.push_str(if pretty { " : " } else { ":" });
                write_value(value, pretty, sort_keys, depth + 1, out);
            }
            newline(pretty, depth, out);
            out.push('}');
        }
    }
}

/// Starts a new indented line when pretty printing.
fn newline(pretty: bool, depth: usize, out: &mut String) {
    if pretty {
        out.push('\n');
        for _ in 0..depth {
            out.push_str("  ");
        }
    }
}

/// Formats a number: shortest round-trip form, no `-0`, no NaN or infinity (written as `null`).
fn number(n: f64) -> String {
    if !n.is_finite() {
        return "null".to_string();
    }
    if n == 0.0 {
        return "0".to_string();
    }
    format!("{n}")
}

/// Writes a quoted string, escaping quotes, backslashes, `/` and control characters.
fn write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
