//! Presentation only: this module does not classify execution as successful.
use std::fmt::Write;
use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Text,
    Short,
    Json,
}

impl Format {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" => Ok(Self::Text),
            "short" => Ok(Self::Short),
            "json" => Ok(Self::Json),
            _ => Err(format!(
                "unsupported error format `{value}`; expected text, short or json"
            )),
        }
    }

    /// A malformed command still honors an explicitly requested JSON channel.
    pub fn for_usage(args: &[String]) -> Self {
        if args.iter().any(|arg| arg == "--error-format=json") {
            Self::Json
        } else if args.iter().any(|arg| arg == "--error-format=short") {
            Self::Short
        } else {
            Self::Text
        }
    }

    pub fn usage(self, message: &str) {
        if self == Self::Short {
            eprintln!("evocc: {message}");
        } else {
            eprint!(
                "{}",
                render(&Diagnostic::command("", message), self, command_color())
            );
        }
    }

    pub fn completion(self, result: i128) -> String {
        match self {
            Self::Text | Self::Short => format!(
                "check completed: main returned {result}; no finding on the executed path\n"
            ),
            Self::Json => format!(
                "{{\"schema\":\"evocc.check.v1\",\"kind\":\"check\",\"status\":\"completed\",\"main_return\":\"{result}\",\"claim\":\"executed-path-only\"}}\n"
            ),
        }
    }
}

pub fn command_color() -> Color {
    std::env::args()
        .filter_map(|arg| {
            arg.strip_prefix("--color=")
                .and_then(|value| Color::parse(value).ok())
        })
        .next_back()
        .unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Color {
    #[default]
    Auto,
    Always,
    Never,
}

impl Color {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            _ => Err(format!(
                "unsupported color `{value}`; expected auto, always or never"
            )),
        }
    }
}

/// Coordinates are one-based byte columns. The end is exclusive.
pub struct Span {
    pub file: String,
    pub line_start: u32,
    pub column_start: u32,
    pub line_end: u32,
    pub column_end: u32,
    pub label: String,
    pub primary: bool,
    pub source: Vec<u8>,
}

impl Span {
    pub fn source(file: &str, source: &[u8], span: evocc_common::SourceSpan, label: &str) -> Self {
        let length = span.byte_end.saturating_sub(span.byte_start) as usize;
        let bytes = source
            .get(span.byte_start as usize..)
            .and_then(|tail| tail.get(..length))
            .unwrap_or_default();
        let mut line_end = span.line_start;
        let mut column_end = span.column_start;
        for byte in bytes {
            if *byte == b'\n' {
                line_end += 1;
                column_end = 1;
            } else {
                column_end += 1;
            }
        }
        Self {
            file: file.into(),
            line_start: span.line_start,
            column_start: span.column_start,
            line_end,
            column_end,
            label: label.into(),
            primary: true,
            source: source.into(),
        }
    }
}

/// Presentation data only. A renderer must not infer source locations.
pub struct Diagnostic {
    pub code: String,
    pub level: &'static str,
    pub message: String,
    pub spans: Vec<Span>,
    pub notes: Vec<String>,
    pub help: Vec<String>,
}

impl Diagnostic {
    pub fn new(code: &str, message: &str, span: Span) -> Self {
        let note = match execution_category(code) {
            "resource-limit" => Some(
                "execution reached a resource limit; this is not a finding of undefined behavior",
            ),
            "unsupported-execution" => Some(
                "execution is not supported for this operation; absence of undefined behavior is not established",
            ),
            _ => None,
        };
        Self {
            code: code.into(),
            level: "error",
            message: message.into(),
            spans: vec![span],
            notes: note.into_iter().map(str::to_owned).collect(),
            help: Vec::new(),
        }
    }

    /// An argument span identifies an invocation error, not a C expression.
    pub fn command(code: &str, message: &str) -> Self {
        Self::argument(code, message, None)
    }

    /// The producer supplies the affected argument. Do not parse the message
    /// to infer a filename. Without an argument, show the complete invocation.
    pub fn argument(code: &str, message: &str, argument: Option<&str>) -> Self {
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        let source = args.join(" ");
        let selected = argument.and_then(|value| args.iter().rposition(|arg| arg == value));
        let start = selected.map_or(0, |index| args[..index].iter().map(|s| s.len() + 1).sum());
        let end = selected.map_or(source.len(), |index| start + args[index].len());
        let prefix = &source[..start];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or_default().len() + 1;
        let span = evocc_common::SourceSpan::new(
            evocc_common::FileId::DUMMY,
            start as u32,
            end as u32,
            line as u32,
            column as u32,
        );
        let header = if message.starts_with('-') {
            format!("option {message}")
        } else {
            message.to_owned()
        };
        Self::new(
            code,
            &header,
            Span::source("<command-line>", source.as_bytes(), span, message),
        )
    }
}

/// Escape controls in source text. Tabs use four-column stops. Invalid source
/// bytes remain visible as hex escapes; the compiler input is not rewritten.
fn displayed(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let byte = bytes[offset];
        if byte == b'\t' {
            let columns = result.chars().count();
            result.push_str(&" ".repeat(4 - columns % 4));
            offset += 1;
        } else if byte.is_ascii_control() || byte == 0x7f {
            write!(result, "\\x{byte:02x}").unwrap();
            offset += 1;
        } else {
            let width = match byte {
                0..=0x7f => 1,
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => 0,
            };
            match bytes
                .get(offset..offset + width)
                .filter(|_| width != 0)
                .and_then(|s| std::str::from_utf8(s).ok())
            {
                Some(s) => {
                    result.push_str(s);
                    offset += width;
                }
                None => {
                    write!(result, "\\x{byte:02x}").unwrap();
                    offset += 1;
                }
            }
        }
    }
    result
}

pub fn render(diagnostic: &Diagnostic, format: Format, color: Color) -> String {
    let primary = diagnostic
        .spans
        .iter()
        .filter(|span| span.primary)
        .collect::<Vec<_>>();
    assert_eq!(
        primary.len(),
        1,
        "a diagnostic must have exactly one primary span"
    );
    for span in &diagnostic.spans {
        assert!(!span.file.is_empty() && span.line_start > 0 && span.column_start > 0);
        assert!((span.line_end, span.column_end) >= (span.line_start, span.column_start));
    }
    let primary = primary[0];
    let heading = if diagnostic.code.is_empty() {
        diagnostic.level.to_owned()
    } else {
        format!("{}[{}]", diagnostic.level, diagnostic.code)
    };
    if format == Format::Short {
        return format!(
            "{}:{}:{}: {}: {}\n",
            primary.file, primary.line_start, primary.column_start, heading, diagnostic.message
        );
    }
    let paint = format != Format::Json
        && (color == Color::Always || (color == Color::Auto && std::io::stderr().is_terminal()));
    let (red, reset) = if paint {
        ("\x1b[1;31m", "\x1b[0m")
    } else {
        ("", "")
    };
    let mut out = format!("{red}{heading}{reset}: {}\n", diagnostic.message);
    for span in &diagnostic.spans {
        let arrow = if span.primary { "-->" } else { ":::" };
        writeln!(
            out,
            "  {arrow} {}:{}:{}",
            span.file, span.line_start, span.column_start
        )
        .unwrap();
        let width = span.line_end.to_string().len();
        writeln!(out, " {:width$} |", "").unwrap();
        let lines = span.source.split(|byte| *byte == b'\n').collect::<Vec<_>>();
        for line in span.line_start..=span.line_end {
            let source = lines.get((line - 1) as usize).copied().unwrap_or_default();
            let start = if line == span.line_start {
                span.column_start as usize - 1
            } else {
                0
            }
            .min(source.len());
            let end = if line == span.line_end {
                span.column_end as usize - 1
            } else {
                source.len()
            }
            .min(source.len());
            let indent = displayed(&source[..start]).chars().count();
            let length = displayed(&source[..end])
                .chars()
                .count()
                .saturating_sub(indent)
                .max(1);
            writeln!(out, " {line:>width$} | {}", displayed(source)).unwrap();
            let marker = if span.primary { "^" } else { "-" };
            let label = if line == span.line_end {
                span.label.as_str()
            } else {
                ""
            };
            writeln!(
                out,
                " {:width$} | {}{red}{}{reset}{}{}",
                "",
                " ".repeat(indent),
                marker.repeat(length),
                if label.is_empty() { "" } else { " " },
                label
            )
            .unwrap();
        }
        writeln!(out, " {:width$} |", "").unwrap();
    }
    for note in &diagnostic.notes {
        writeln!(out, "   = note: {note}").unwrap();
    }
    for help in &diagnostic.help {
        writeln!(out, "   = help: {help}").unwrap();
    }
    if format != Format::Json {
        return out;
    }
    let spans = diagnostic.spans.iter().map(|span| format!(
        "{{\"file\":{},\"line_start\":{},\"column_start\":{},\"line_end\":{},\"column_end\":{},\"label\":{},\"primary\":{}}}",
        quoted(&span.file), span.line_start, span.column_start, span.line_end, span.column_end, quoted(&span.label), span.primary
    )).collect::<Vec<_>>().join(",");
    let list = |items: &[String]| {
        items
            .iter()
            .map(|s| quoted(s))
            .collect::<Vec<_>>()
            .join(",")
    };
    let code = if diagnostic.code.is_empty() {
        "null".into()
    } else {
        quoted(&diagnostic.code)
    };
    format!(
        "{{\"code\":{code},\"level\":{},\"message\":{},\"spans\":[{spans}],\"notes\":[{}],\"help\":[{}],\"rendered\":{}}}\n",
        quoted(diagnostic.level),
        quoted(&diagnostic.message),
        list(&diagnostic.notes),
        list(&diagnostic.help),
        quoted(&out)
    )
}

include!(concat!(env!("OUT_DIR"), "/diagnostic_catalogue.rs"));

pub fn explain(code: &str) -> Result<String, String> {
    let index = CATALOGUE
        .binary_search_by_key(&code, |row| row.0)
        .map_err(|_| format!("no explanation for code {code}"))?;
    Ok(CATALOGUE[index]
        .1
        .lines()
        .filter(|line| !line.starts_with("```"))
        .map(|line| format!("{line}\n"))
        .collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Diagnostic {
        let source = b"int main(void) {\n    return a / b;\n}\n";
        let span = evocc_common::SourceSpan::new(evocc_common::FileId::new(1), 28, 33, 2, 12);
        let mut diagnostic = Diagnostic::new(
            "E3002",
            "division by zero",
            Span::source("demo.c", source, span, "divisor is 0 here"),
        );
        diagnostic
            .notes
            .push("the divisor was 0 on the executed path".into());
        diagnostic
            .help
            .push("check the divisor before the division".into());
        diagnostic
    }

    #[test]
    fn text_includes_labels_notes_and_help() {
        assert_eq!(
            render(&sample(), Format::Text, Color::Never),
            concat!(
                "error[E3002]: division by zero\n",
                "  --> demo.c:2:12\n",
                "   |\n",
                " 2 |     return a / b;\n",
                "   |            ^^^^^ divisor is 0 here\n",
                "   |\n",
                "   = note: the divisor was 0 on the executed path\n",
                "   = help: check the divisor before the division\n"
            )
        );
    }

    #[test]
    #[should_panic(expected = "exactly one primary span")]
    fn missing_primary_span_is_a_bug() {
        let mut diagnostic = sample();
        diagnostic.spans.clear();
        render(&diagnostic, Format::Text, Color::Never);
    }

    #[test]
    #[should_panic(expected = "exactly one primary span")]
    fn two_primary_spans_are_a_bug() {
        let mut diagnostic = sample();
        diagnostic.spans.extend(sample().spans);
        render(&diagnostic, Format::Text, Color::Never);
    }

    #[test]
    fn secondary_labels_keep_their_own_file_and_source() {
        let mut diagnostic = sample();
        let mut secondary = Span::source(
            "header.h",
            b"int b;\n",
            evocc_common::SourceSpan::new(evocc_common::FileId::new(2), 4, 5, 1, 5),
            "declared here",
        );
        secondary.primary = false;
        diagnostic.spans.push(secondary);
        let text = render(&diagnostic, Format::Text, Color::Never);
        assert!(
            text.contains("::: header.h:1:5\n   |\n 1 | int b;\n   |     - declared here"),
            "{text}"
        );
    }

    #[test]
    fn multiline_span_marks_each_line_and_labels_the_end() {
        let diagnostic = Diagnostic::new(
            "E1001",
            "invalid expression",
            Span::source(
                "multiline.c",
                b"abcd\nefgh\n",
                evocc_common::SourceSpan::new(evocc_common::FileId::new(1), 1, 7, 1, 2),
                "expression ends here",
            ),
        );
        let text = render(&diagnostic, Format::Text, Color::Never);
        assert!(text.contains("1 | abcd\n   |  ^^^\n"), "{text}");
        assert!(
            text.contains("2 | efgh\n   | ^^ expression ends here\n"),
            "{text}"
        );
    }

    #[test]
    fn tabs_raw_bytes_and_controls_do_not_change_input() {
        assert_eq!(displayed(b"\tx\t\xff\x1b"), "    x   \\xff\\x1b");
        assert_eq!(displayed("é".as_bytes()), "é");
        assert_eq!(displayed(b"\xe2\x82"), "\\xe2\\x82");
    }

    #[test]
    fn json_has_one_line_and_never_terminal_colors() {
        let json = render(&sample(), Format::Json, Color::Always);
        assert_eq!(json.lines().count(), 1);
        assert!(!json.contains('\x1b'));
        assert!(json.contains("\"spans\":[{\"file\":\"demo.c\",\"line_start\":2,\"column_start\":12,\"line_end\":2,\"column_end\":17"));
        assert!(json.contains("\"notes\":[\"the divisor was 0 on the executed path\"]"));
    }

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
