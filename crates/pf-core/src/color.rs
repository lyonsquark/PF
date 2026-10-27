//! Colour hex parsing and formatting. No egui types: colours are plain byte arrays.

/// Default context colour when parsing fails (SwiftUI `.blue`, approximately).
pub const DEFAULT_BLUE: [u8; 4] = [0, 122, 255, 255];

/// R-CTX-4: trim, strip every '#', then 6 hex digits → [r,g,b,255]; 8 → [r,g,b,a]; otherwise None.
pub fn parse_hex(s: &str) -> Option<[u8; 4]> {
    let digits: String = s.trim().chars().filter(|c| *c != '#').collect();
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(&digits, 16).ok()?;
    match digits.len() {
        // A 6-digit value fits in 24 bits; alpha is opaque.
        6 => Some([(value >> 16) as u8, (value >> 8) as u8, value as u8, 255]),
        // An 8-digit value fits in 32 bits, so the truncating casts keep each byte.
        8 => Some([
            (value >> 24) as u8,
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ]),
        _ => None,
    }
}

/// Swift `toHex()`: "#%06x", i.e. lowercase, alpha dropped. [255,0,170,_] → "#ff00aa".
pub fn to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Covers: R-CTX-4
    fn r_ctx_4_parse_hex_six_and_eight() {
        assert_eq!(parse_hex("#FFD700"), Some([255, 215, 0, 255]));
        assert_eq!(parse_hex("8B451380"), Some([139, 69, 19, 128]));
    }

    #[test]
    /// Covers: R-CTX-4
    fn r_ctx_4_parse_hex_invalid() {
        assert_eq!(parse_hex(""), None);
        assert_eq!(parse_hex("#FFF"), None);
        assert_eq!(parse_hex("#GGGGGG"), None);
        assert_eq!(parse_hex("#1234567"), None);
    }

    #[test]
    /// Covers: R-CTX-4
    fn to_hex_lowercase_six_digits() {
        assert_eq!(to_hex([255, 0, 170]), "#ff00aa");
        assert_eq!(to_hex([0, 0, 0]), "#000000");
    }
}
