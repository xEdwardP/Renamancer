pub fn split(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(index) if index > 0 => name.split_at(index),
        _ => (name, ""),
    }
}

pub fn validate(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("El nombre está vacío.");
    }
    if name.contains("..")
        || name.chars().any(|character| {
            character <= '\u{1f}'
                || matches!(
                    character,
                    '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                )
        })
    {
        return Err("El nombre contiene caracteres que Windows no admite.");
    }
    if name.ends_with(['.', ' ']) {
        return Err("El nombre no puede terminar en punto ni en espacio.");
    }
    let device = name.split('.').next().unwrap_or(name).trim_end_matches(' ');
    let device = device.to_ascii_uppercase();
    let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            matches!(
                device.strip_prefix(prefix),
                Some("1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
            )
        });
    if reserved {
        return Err("El nombre está reservado por Windows.");
    }
    Ok(())
}
