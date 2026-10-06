//! Presentation only: this module does not classify execution as successful.
use std::fmt::Write;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Text,
    Json,
}

impl Format {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(format!(
                "unsupported error format `{value}`; expected text or json"
            )),
        }
    }

    /// A malformed command still honors an explicitly requested JSON channel.
    pub fn for_usage(args: &[String]) -> Self {
        if args.iter().any(|arg| arg == "--error-format=json") {
            Self::Json
        } else {
            Self::Text
        }
    }

    pub fn usage(self, message: &str) {
        if self == Self::Text {
            eprintln!("evocc: {message}");
        } else {
            eprintln!(
                "{}",
                render_json(None, None, None, message, "driver", "driver")
            );
        }
    }

    pub fn error(
        self,
        file: &str,
        location: Option<(u32, u32)>,
        code: &str,
        message: &str,
        phase: &str,
        category: &str,
    ) {
        if self == Self::Json {
            eprintln!(
                "{}",
                render_json(Some(file), location, Some(code), message, phase, category)
            );
        } else if let Some((line, column)) = location {
            eprintln!("{file}:{line}:{column}: error[{code}]: {message}");
        } else {
            eprintln!("{file}: error[{code}]: {message}");
        }
    }

    pub fn completion(self, result: i128) -> String {
        match self {
            Self::Text => format!("check completed: main returned {result}; no finding on the executed path\n"),
            Self::Json => format!("{{\"schema\":\"evocc.check.v1\",\"kind\":\"check\",\"status\":\"completed\",\"main_return\":\"{result}\",\"claim\":\"executed-path-only\"}}\n"),
        }
    }
}

struct Explanation {
    code: &'static str,
    message: Option<&'static str>,
    detected: Option<&'static str>,
}

include!(concat!(env!("OUT_DIR"), "/diagnostic_catalogue.rs"));

fn explanation(code: &str) -> Option<&'static Explanation> {
    CATALOGUE
        .binary_search_by_key(&code, |row| row.code)
        .ok()
        .map(|index| &CATALOGUE[index])
}

pub fn explain(code: &str) -> Result<String, String> {
    let row = explanation(code).ok_or_else(|| format!("unknown diagnostic code `{code}`"))?;
    Ok(match (row.message, row.detected) {
        (Some(message), Some(detected)) => format!(
            "{code}: {message}\nDetected: {detected}\nAuthority: spec/evoir.md, section 6.3 Diagnostic Authority.\n\nA runtime finding describes an executed path, not every possible input.\nResource limits and unsupported execution do not establish absence of UB.\n"
        ),
        _ => format!("{code}\nTODO: explanation missing from EvoIR section 6.3.\nThis code is named in the specification or compiler source. Its absence from\nthe explanation table does not change the existing diagnostic's meaning.\n"),
    })
}

pub fn phase(code: &str) -> &'static str {
    match code {
        "E0003" => "input",
        "E0004" => "codegen",
        "E0112" => "parsing",
        _ if code.starts_with("E00") => "lexing",
        _ if code.starts_with("E01") => "preprocessing",
        _ if code.starts_with("E1") || code.starts_with("E2") => "parsing",
        _ => "lowering",
    }
}

/// Categories describe the existing finding; unknown codes fail conservatively.
pub fn execution_category(code: &str) -> &'static str {
    match code {
        "E3008" => "resource-limit",
        "E3021" | "E3025" | "E3031" | "E3033" | "E3039" | "E3040" | "E3064" | "E3065" => {
            "unsupported-execution"
        }
        "E3001" | "E3002" | "E3003" | "E3004" | "E3005" | "E3006" | "E3009" | "E3010" | "E3011"
        | "E3012" | "E3013" | "E3014" | "E3015" | "E3016" | "E3018" | "E3019" | "E3022"
        | "E3024" | "E3027" | "E3038" | "E3048" | "E3063" => "undefined-behavior",
        _ => "execution-contract",
    }
}

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch <= '\u{1f}' => {
                write!(out, "\\u{:04x}", u32::from(ch)).unwrap();
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn render_json(
    file: Option<&str>,
    location: Option<(u32, u32)>,
    code: Option<&str>,
    message: &str,
    phase: &str,
    category: &str,
) -> String {
    let source = file.map_or_else(|| "null".into(), quoted);
    let code_text = code.map_or_else(|| "null".into(), quoted);
    let location = location.map_or_else(
        || "null".into(),
        |(line, column)| format!("{{\"line\":{line},\"column\":{column}}}"),
    );
    let availability = match code.and_then(explanation) {
        Some(row) if row.message.is_some() => "available",
        Some(_) => "todo",
        None => "unavailable",
    };
    format!("{{\"schema\":\"evocc.diagnostic.v1\",\"kind\":\"diagnostic\",\"severity\":\"error\",\"code\":{code_text},\"message\":{},\"phase\":{},\"category\":{},\"source\":{source},\"location\":{location},\"explanation\":{}}}", quoted(message), quoted(phase), quoted(category), quoted(availability))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_preserves_unicode_and_escapes_controls() {
        assert_eq!(
            quoted("雪\"\\\n\r\t\u{0}\u{1f}"),
            "\"雪\\\"\\\\\\n\\r\\t\\u0000\\u001f\""
        );
    }

    #[test]
    fn catalogue_is_unique_and_missing_pages_stay_explicit() {
        assert!(CATALOGUE.windows(2).all(|pair| pair[0].code < pair[1].code));
        for row in CATALOGUE {
            let text = explain(row.code).unwrap();
            assert!(row.message.is_some() || text.contains("TODO: explanation missing"));
        }
        assert!(explain("E3007").unwrap().contains("withdrawn"));
    }
}
