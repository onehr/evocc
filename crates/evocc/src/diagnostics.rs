//! Presentation only: this module does not classify execution as successful.
use std::fmt::Write;
use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    #[default]
    Text,
    Short,
    Json,
    /// One `concise-1` object per diagnostic; see spec/diagnostics.md 5.2.
    Concise,
}

impl Format {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" => Ok(Self::Text),
            "short" => Ok(Self::Short),
            "json" => Ok(Self::Json),
            "concise" => Ok(Self::Concise),
            _ => Err(format!(
                "unsupported error format `{value}`; expected text, short, json or concise"
            )),
        }
    }

    /// A malformed command still honors an explicitly requested JSON channel.
    pub fn for_usage(args: &[String]) -> Self {
        if args.iter().any(|arg| arg == "--error-format=json") {
            Self::Json
        } else if args.iter().any(|arg| arg == "--error-format=concise") {
            Self::Concise
        } else if args.iter().any(|arg| arg == "--error-format=short") {
            Self::Short
        } else {
            Self::Text
        }
    }

    pub fn usage(self, message: &str) {
        self.usage_at(message, None);
    }

    /// Report a command error with a label on `argument`, when given.
    pub fn usage_at(self, message: &str, argument: Option<&str>) {
        if self == Self::Short {
            eprintln!("evocc: {message}");
        } else {
            eprint!(
                "{}",
                render(
                    &Diagnostic::argument("", message, argument),
                    self,
                    command_color()
                )
            );
        }
    }

    pub fn completion(self, result: i128) -> String {
        match self {
            Self::Text | Self::Short => {
                format!("check completed: main returned {result}. {CHECK_SCOPE}\n")
            }
            Self::Json => format!(
                "{{\"schema\":\"evocc.check.v1\",\"kind\":\"check\",\"status\":\"completed\",\"main_return\":\"{result}\",\"claim\":\"executed-path-only\",\"scope\":{}}}\n",
                quoted(CHECK_SCOPE)
            ),
            Self::Concise => format!(
                "{{\"status\":\"completed\",\"main_return\":\"{result}\",\"scope\":{}}}\n",
                quoted(CHECK_SCOPE)
            ),
        }
    }
}

/// The source name of command-line diagnostics. It is not a C file.
const COMMAND_LINE: &str = "<command-line>";
/// A longer command line shows only the labelled bytes in text.
const LONG_COMMAND_LINE: usize = 80;

/// The scope of a completed check. It is not a correctness claim.
const CHECK_SCOPE: &str =
    "No finding from the implemented checks on the executed path. Pointer subtraction and ordering obligations are not checked. This does not check that the results are correct.";

/// Every precise execution finding has this note. A concise record omits it.
const EXECUTED_PATH: &str = "this operation failed on the executed path";
/// The longest `source` excerpt of a concise record, in characters.
const EXCERPT: usize = 80;

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
        // Lexer columns count characters. The external diagnostic contract
        // counts bytes, so normalize from the preserved physical offsets.
        let (line_start, column_start) = source
            .get(..span.byte_start as usize)
            .map(|prefix| {
                prefix.iter().fold((1, 1), |(line, column), b| {
                    if *b == b'\n' {
                        (line + 1, 1)
                    } else {
                        (line, column + 1)
                    }
                })
            })
            .unwrap_or((span.line_start, span.column_start));
        let length = span.byte_end.saturating_sub(span.byte_start) as usize;
        let bytes = source
            .get(span.byte_start as usize..)
            .and_then(|tail| tail.get(..length))
            .unwrap_or_default();
        let mut line_end = line_start;
        let mut column_end = column_start;
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
            line_start,
            column_start,
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
    pub repairs: Vec<Repair>,
}

pub struct Repair {
    pub range: std::ops::Range<usize>,
    pub repair_id: &'static str,
    pub kind: &'static str,
    pub span: Span,
    pub replacement: String,
    pub title: &'static str,
}

impl Repair {
    pub fn json(&self) -> String {
        format!("{{\"repair_id\":{},\"kind\":{},\"file\":{},\"span\":{},\"replacement\":{},\"title\":{}}}",
            quoted(self.repair_id), quoted(self.kind), quoted(&self.span.file), span_json(&self.span), quoted(&self.replacement), quoted(self.title))
    }

    /// The concise repair. `at` and `end` give the whole edit span.
    fn concise(&self) -> String {
        format!(
            "{{\"id\":{},\"kind\":{},\"title\":{},\"at\":{},\"end\":\"{}:{}\",\"replacement\":{}}}",
            quoted(self.repair_id),
            quoted(self.kind),
            quoted(self.title),
            quoted(&location(&self.span)),
            self.span.line_end,
            self.span.column_end,
            quoted(&self.replacement)
        )
    }
}

fn span_json(span: &Span) -> String {
    format!("{{\"file\":{},\"line_start\":{},\"column_start\":{},\"line_end\":{},\"column_end\":{},\"label\":{},\"primary\":{}}}",
        quoted(&span.file), span.line_start, span.column_start, span.line_end, span.column_end, quoted(&span.label), span.primary)
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
            repairs: Vec::new(),
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
            Span::source(COMMAND_LINE, source.as_bytes(), span, message),
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
    if format == Format::Concise {
        return concise(diagnostic, primary);
    }
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
            let mut source = lines.get((line - 1) as usize).copied().unwrap_or_default();
            let mut start = if line == span.line_start {
                span.column_start as usize - 1
            } else {
                0
            }
            .min(source.len());
            let mut end = if line == span.line_end {
                span.column_end as usize - 1
            } else {
                source.len()
            }
            .min(source.len());
            // A long command line shows only the labelled bytes. The location
            // above keeps the column in the complete line.
            let (mut before, mut after) = ("", "");
            if span.file == COMMAND_LINE
                && span.line_start == span.line_end
                && source.len() > LONG_COMMAND_LINE
                && (start > 0 || end < source.len())
            {
                before = if start > 0 { "... " } else { "" };
                after = if end < source.len() { " ..." } else { "" };
                source = &source[start..end];
                (start, end) = (0, end - start);
            }
            let indent = displayed(&source[..start]).chars().count();
            let length = displayed(&source[..end])
                .chars()
                .count()
                .saturating_sub(indent)
                .max(1);
            writeln!(
                out,
                " {line:>width$} | {before}{}{after}",
                displayed(source)
            )
            .unwrap();
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
                " ".repeat(before.len() + indent),
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
    for repair in diagnostic.repairs.iter().filter(|r| r.kind == "safe") {
        let title = repair.title;
        let replacement = quoted(&repair.replacement);
        writeln!(out, "   = help: {title}; replace with {replacement}").unwrap();
    }
    if format != Format::Json {
        return out;
    }
    let spans: Vec<_> = diagnostic.spans.iter().map(span_json).collect();
    let repairs: Vec<_> = diagnostic.repairs.iter().map(Repair::json).collect();
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
        "{{\"code\":{code},\"level\":{},\"message\":{},\"spans\":[{}],\"notes\":[{}],\"help\":[{}],\"rendered\":{},\"repairs\":[{}]}}\n",
        quoted(diagnostic.level),
        quoted(&diagnostic.message),
        spans.join(","),
        list(&diagnostic.notes),
        list(&diagnostic.help),
        quoted(&out),
        repairs.join(",")
    )
}

/// A `concise-1` diagnostic. Section 5.2 fixes its keys and their order. An
/// absent key is omitted, not null.
fn concise(diagnostic: &Diagnostic, primary: &Span) -> String {
    let code = if diagnostic.code.is_empty() {
        "null".into()
    } else {
        quoted(&diagnostic.code)
    };
    let mut out = format!(
        "{{\"code\":{code},\"message\":{}",
        quoted(&diagnostic.message)
    );
    if diagnostic.level != "error" {
        write!(out, ",\"level\":{}", quoted(diagnostic.level)).unwrap();
    }
    write!(out, ",\"at\":{}", quoted(&location(primary))).unwrap();
    if primary.line_end != primary.line_start {
        write!(
            out,
            ",\"end\":\"{}:{}\"",
            primary.line_end, primary.column_end
        )
        .unwrap();
    }
    if let Some(source) = excerpt(primary) {
        write!(out, ",\"source\":{}", quoted(&source)).unwrap();
    }
    if primary.label != diagnostic.message {
        write!(out, ",\"label\":{}", quoted(&primary.label)).unwrap();
    }
    let also: Vec<_> = diagnostic
        .spans
        .iter()
        .filter(|span| !span.primary)
        .map(|span| match span.label.as_str() {
            "" => location(span),
            label => format!("{} {label}", location(span)),
        })
        .collect();
    let notes: Vec<_> = diagnostic
        .notes
        .iter()
        .filter(|note| note.as_str() != EXECUTED_PATH)
        .collect();
    let list = |items: &[&String]| {
        items
            .iter()
            .map(|s| quoted(s))
            .collect::<Vec<_>>()
            .join(",")
    };
    for (key, items) in [
        ("also", also.iter().collect::<Vec<_>>()),
        ("notes", notes),
        ("help", diagnostic.help.iter().collect()),
    ] {
        if !items.is_empty() {
            write!(out, ",\"{key}\":[{}]", list(&items)).unwrap();
        }
    }
    if let Some(why) = explanation_line(&diagnostic.code) {
        write!(out, ",\"why\":{}", quoted(why)).unwrap();
    }
    if let Some(fix) = repair_hint(&diagnostic.code) {
        write!(out, ",\"fix\":{}", quoted(fix)).unwrap();
    }
    if !diagnostic.repairs.is_empty() {
        let repairs: Vec<_> = diagnostic.repairs.iter().map(Repair::concise).collect();
        write!(out, ",\"repairs\":[{}]", repairs.join(",")).unwrap();
    }
    out.push_str("}\n");
    out
}

/// `PATH:LINE:COLUMN` of the start of a span.
fn location(span: &Span) -> String {
    let directory = std::env::current_dir().and_then(std::fs::canonicalize);
    let file = directory
        .ok()
        .and_then(|directory| relative(&span.file, &directory))
        .unwrap_or_else(|| span.file.clone());
    format!("{file}:{}:{}", span.line_start, span.column_start)
}

/// The name of `file` relative to the canonical `directory`, when the file is
/// below it. Canonical paths keep a symbolic link, such as `/var` on macOS,
/// from hiding the relation.
fn relative(file: &str, directory: &std::path::Path) -> Option<String> {
    let path = std::fs::canonicalize(file).ok()?;
    let name = path.strip_prefix(directory).ok()?.to_str()?;
    (!name.is_empty()).then(|| name.to_owned())
}

/// The span text on its first line, as the text form shows it under the
/// markers. A long excerpt is cut. ` ...` marks a span that continues.
fn excerpt(span: &Span) -> Option<String> {
    let line = span
        .source
        .split(|byte| *byte == b'\n')
        .nth(span.line_start as usize - 1)
        .unwrap_or_default();
    let start = (span.column_start as usize - 1).min(line.len());
    let end = if span.line_end == span.line_start {
        (span.column_end as usize - 1).min(line.len())
    } else {
        line.len()
    };
    let indent = displayed(&line[..start]).chars().count();
    let width = displayed(&line[..end])
        .chars()
        .count()
        .saturating_sub(indent);
    let mut text: String = displayed(line).chars().skip(indent).take(width).collect();
    if text.is_empty() {
        return None;
    }
    if text.chars().count() > EXCERPT {
        text = text.chars().take(EXCERPT - 3).collect::<String>() + "...";
    }
    if span.line_end > span.line_start {
        text.push_str(" ...");
    }
    Some(text)
}

include!(concat!(env!("OUT_DIR"), "/diagnostic_catalogue.rs"));

/// The first line of the first paragraph of the explanation page.
fn explanation_line(code: &str) -> Option<&'static str> {
    let index = CATALOGUE.binary_search_by_key(&code, |row| row.0).ok()?;
    CATALOGUE[index]
        .1
        .lines()
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .filter(|line| !line.starts_with("```"))
}

/// The one-line repair hint from `explain/hints.txt`, when the code has one.
fn repair_hint(code: &str) -> Option<&'static str> {
    let index = HINTS.binary_search_by_key(&code, |row| row.0).ok()?;
    Some(HINTS[index].1)
}

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

pub(crate) fn quoted(text: &str) -> String {
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
    fn long_command_lines_show_only_the_labelled_argument() {
        let line = format!("{} -fno-common -c x.c", "-DA=1 ".repeat(20).trim_end());
        let start = line.find("-fno-common").unwrap();
        let span = |from: usize, to: usize| {
            evocc_common::SourceSpan::new(
                evocc_common::FileId::DUMMY,
                from as u32,
                to as u32,
                1,
                from as u32 + 1,
            )
        };
        let text = |file: &str, source: &[u8], from, to| {
            let diagnostic = Diagnostic::new(
                "",
                "message",
                Span::source(file, source, span(from, to), "label"),
            );
            render(&diagnostic, Format::Text, Color::Never)
        };
        // The location keeps the column in the complete command line.
        assert_eq!(
            text("<command-line>", line.as_bytes(), start, start + 11),
            format!(
                "error: message\n  --> <command-line>:1:{}\n   |\n 1 | ... -fno-common ...\n   |     ^^^^^^^^^^^ label\n   |\n",
                start + 1
            )
        );
        // A side without omitted bytes has no ellipsis.
        let first = text("<command-line>", line.as_bytes(), 0, 5);
        assert!(
            first.contains(" 1 | -DA=1 ...\n   | ^^^^^ label\n"),
            "{first}"
        );
        let last = text(
            "<command-line>",
            line.as_bytes(),
            line.len() - 3,
            line.len(),
        );
        assert!(
            last.contains(" 1 | ... x.c\n   |     ^^^ label\n"),
            "{last}"
        );
        // The complete invocation, a short command line and a C source line
        // keep the whole line.
        let whole = text("<command-line>", line.as_bytes(), 0, line.len());
        assert!(whole.contains(&format!(" 1 | {line}\n")), "{whole}");
        let short = text("<command-line>", b"-fno-common -c x.c", 0, 11);
        assert!(
            short.contains(" 1 | -fno-common -c x.c\n   | ^^^^^^^^^^^ label\n"),
            "{short}"
        );
        let source = format!("int x = {}1;", "1 + ".repeat(40));
        let c = text("long.c", source.as_bytes(), 4, 5);
        assert!(c.contains(&format!(" 1 | {source}\n")), "{c}");
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
    fn concise_keeps_the_key_order_and_omits_defaults() {
        let mut diagnostic = sample();
        diagnostic.notes.insert(0, EXECUTED_PATH.into());
        assert_eq!(
            render(&diagnostic, Format::Concise, Color::Always),
            concat!(
                "{\"code\":\"E3002\",\"message\":\"division by zero\",\"at\":\"demo.c:2:12\",",
                "\"source\":\"a / b\",\"label\":\"divisor is 0 here\",",
                "\"notes\":[\"the divisor was 0 on the executed path\"],",
                "\"help\":[\"check the divisor before the division\"],",
                "\"why\":\"The executed integer division has a zero divisor.\",",
                "\"fix\":\"Check the divisor and choose the required error behavior before dividing.\"}\n"
            )
        );
        // A command warning has no code, page or hint. An insertion point has no text.
        let span = |start, end| {
            evocc_common::SourceSpan::new(evocc_common::FileId::DUMMY, start, end, 1, start + 1)
        };
        let mut warning = Diagnostic::new(
            "",
            "message",
            Span::source(COMMAND_LINE, b"-x y", span(0, 2), "label"),
        );
        warning.level = "warning";
        let mut secondary = Span::source(COMMAND_LINE, b"-x y", span(3, 3), "");
        secondary.primary = false;
        warning.spans.push(secondary);
        assert_eq!(
            render(&warning, Format::Concise, Color::Never),
            "{\"code\":null,\"message\":\"message\",\"level\":\"warning\",\"at\":\"<command-line>:1:1\",\"source\":\"-x\",\"label\":\"label\",\"also\":[\"<command-line>:1:4\"]}\n"
        );
    }

    #[test]
    fn concise_source_is_the_shown_first_line_and_is_cut() {
        let excerpt_of = |source: &[u8], start: u32, end: u32, line: u32, column: u32| {
            let span = evocc_common::SourceSpan::new(
                evocc_common::FileId::new(1),
                start,
                end,
                line,
                column,
            );
            excerpt(&Span::source("cut.c", source, span, "label"))
        };
        // Tabs expand from the start of the line, as under the text markers.
        assert_eq!(excerpt_of(b"ab\tc\n", 1, 4, 1, 2).as_deref(), Some("b  c"));
        assert_eq!(
            excerpt_of(b"a\x1bb\n", 0, 3, 1, 1).as_deref(),
            Some("a\\x1bb")
        );
        assert_eq!(excerpt_of(b"abc\n", 1, 1, 1, 2), None);
        let body = format!("int x = {}1;", "1 + ".repeat(30));
        let source = format!("\t{body}\nint y;\n");
        // 80 characters stay whole. A longer text keeps 77 and `...`.
        let whole = excerpt_of(source.as_bytes(), 1, 81, 1, 2).unwrap();
        assert_eq!((whole.chars().count(), whole.as_str()), (80, &body[..80]));
        let cut = excerpt_of(source.as_bytes(), 1, 82, 1, 2).unwrap();
        assert_eq!(cut, format!("{}...", &body[..77]));
        // A span that continues on the next line ends with ` ...`.
        let continued = excerpt_of(source.as_bytes(), 1, source.len() as u32 - 2, 1, 2).unwrap();
        assert_eq!(continued, format!("{}... ...", &body[..77]));
        let short = excerpt_of(b"f(\n);\n", 0, 4, 1, 1).unwrap();
        assert_eq!(short, "f( ...");
    }

    #[test]
    fn concise_paths_compare_canonical_directories() {
        let directory =
            std::env::temp_dir().join(format!("evocc-concise-path-{}", std::process::id()));
        std::fs::create_dir_all(directory.join("sub")).unwrap();
        let file = directory.join("sub/input.c");
        std::fs::write(&file, "").unwrap();
        let file = file.to_str().unwrap();
        let canonical = directory.canonicalize().unwrap();
        // The temporary directory can itself pass through a symbolic link.
        assert_eq!(relative(file, &canonical).as_deref(), Some("sub/input.c"));
        assert_eq!(
            relative(file, &canonical.join("sub")).as_deref(),
            Some("input.c")
        );
        assert_eq!(relative(file, &canonical.join("other")), None);
        assert_eq!(relative(directory.to_str().unwrap(), &canonical), None);
        assert_eq!(relative(COMMAND_LINE, &canonical), None);
        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn every_page_has_a_why_sentence_and_hints_are_sorted() {
        for (code, _) in CATALOGUE {
            let why = explanation_line(code).unwrap();
            assert!(why.ends_with('.'), "{code}: {why}");
        }
        assert!(HINTS.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (code, hint) in HINTS {
            assert!(explanation_line(code).is_some(), "{code}");
            assert_eq!(repair_hint(code), Some(*hint));
        }
        assert_eq!(repair_hint("E9999"), None);
        assert_eq!(explanation_line(""), None);
    }

    #[test]
    fn catalogue_is_unique_and_withdrawn_pages_stay_explicit() {
        assert!(CATALOGUE.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (code, _) in CATALOGUE {
            let text = explain(code).unwrap();
            assert!(text.starts_with(&format!("# {code}: ")));
        }
        assert!(explain("E3007").unwrap().contains("withdrawn"));
    }
}
