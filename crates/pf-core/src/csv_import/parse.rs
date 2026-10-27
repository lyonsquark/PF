//! R-CSV-1: lines and fields. A port of Swift `parseCSV` and `splitCSVLine`; the `csv` crate is
//! deliberately not used, so quoting matches Swift.

/// Headers and data rows of one CSV file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedCsv {
    /// Trimmed header names from the first non-blank line.
    pub headers: Vec<String>,
    /// One vector per later non-blank line.
    pub rows: Vec<Vec<String>>,
}

/// R-CSV-1: the line breaks Swift's `.newlines` set recognises (CR, LF, VT, FF, NEL, LS, PS).
fn is_newline(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0B}' | '\u{0C}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// R-CSV-1: splits one line on commas outside double quotes. Each `"` toggles quoting and is
/// dropped, so escaped `""` is not supported (Swift parity). Each field is trimmed. Always at
/// least one field.
pub fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for c in line.chars() {
        if c == '"' {
            in_quotes = !in_quotes;
        } else if c == ',' && !in_quotes {
            fields.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(c);
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// R-CSV-1: splits the text into lines, drops lines that are blank after trimming, takes the
/// first as the headers and splits every other line into fields.
pub fn parse_csv(text: &str) -> ParsedCsv {
    let mut lines = text
        .split(is_newline)
        .filter(|line| !line.trim().is_empty());
    let Some(first) = lines.next() else {
        return ParsedCsv::default();
    };
    ParsedCsv {
        headers: split_csv_line(first),
        rows: lines.map(split_csv_line).collect(),
    }
}
