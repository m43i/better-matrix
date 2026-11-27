use std::collections::HashMap;
use std::sync::LazyLock;

static CATALOG: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|value| !value.is_empty());
    let language = locale.as_deref().and_then(|locale| locale.split(['_', '-', '.']).next());
    let catalog = match language {
        Some("de") => include_str!("../i18n/de.json"),
        Some("fr") => include_str!("../i18n/fr.json"),
        _ => include_str!("../i18n/en.json"),
    };
    serde_json::from_str(catalog).unwrap_or_default()
});

pub(crate) fn t(key: &str) -> &str {
    CATALOG.get(key).map_or(key, String::as_str)
}

pub(crate) fn format(key: &str, args: &[(&str, String)]) -> String {
    // Interpolate only placeholders from the catalog. A supplied value is always literal text.
    let mut output = String::new();
    for (index, part) in t(key).split('{').enumerate() {
        if index > 0
            && let Some((name, rest)) = part.split_once('}')
            && let Some((_, value)) = args.iter().find(|(key, _)| *key == name)
        {
            output.push_str(value);
            output.push_str(rest);
        } else {
            if index > 0 {
                output.push('{');
            }
            output.push_str(part);
        }
    }
    output
}

macro_rules! tf {
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        crate::text::format($key, &[$((stringify!($name), ($value).to_string())),+])
    };
}
