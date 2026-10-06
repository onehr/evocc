//! Make rules that name the files a compilation read: `-M`, `-MM`, `-MD`,
//! `-MMD`, `-MF`, `-MT`, `-MQ` and `-MP`, as cc defines them.

use std::path::{Path, PathBuf};

/// Which headers a rule names.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Headers {
    /// Every header (`-M`, `-MD`).
    All,
    /// Every header outside the system directories (`-MM`, `-MMD`).
    User,
}

/// The dependency options of one command line.
#[derive(Default)]
pub(crate) struct Request {
    /// `-M` or `-MM`: the rule is the output, and nothing is compiled.
    pub only: Option<Headers>,
    /// `-MD` or `-MMD`: the rule goes to a file beside the compilation.
    pub beside: Option<Headers>,
    /// `-MF FILE`.
    pub file: Option<String>,
    /// `-MT TARGET` and `-MQ TARGET`, in order; the flag marks `-MQ`.
    pub targets: Vec<(String, bool)>,
    /// `-MP`: one phony rule for each header.
    pub phony: bool,
}

impl Request {
    /// The headers of the active request.
    pub(crate) fn headers(&self) -> Option<Headers> {
        self.only.or(self.beside)
    }
}

/// Escape a file name for make. A space, `#` and `$` need escapes.
fn escaped(name: &str) -> String {
    let mut text = String::with_capacity(name.len());
    for character in name.chars() {
        match character {
            ' ' | '\t' => {
                text.push('\\');
                text.push(character);
            }
            '#' => text.push_str("\\#"),
            '$' => text.push_str("$$"),
            _ => text.push(character),
        }
    }
    text
}

/// Show `path` relative to the working directory when it is inside it.
fn shown(path: &str, directory: Option<&Path>) -> String {
    if let Some(directory) = directory {
        if let Ok(relative) = Path::new(path).strip_prefix(directory) {
            return relative.to_string_lossy().into_owned();
        }
    }
    path.to_owned()
}

/// The rule for one compilation.
///
/// `input` is the source file as the command line names it. `opened` is the
/// list of files that preprocessing opened, the input file first. A header
/// under one of `system` is left out when `headers` is [`Headers::User`].
pub(crate) fn rule(
    request: &Request,
    default_target: &str,
    input: &str,
    opened: &[String],
    system: &[String],
) -> String {
    let directory = std::env::current_dir()
        .ok()
        .and_then(|directory| std::fs::canonicalize(directory).ok());
    let system: Vec<PathBuf> = system
        .iter()
        .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path)))
        .collect();
    let headers = request.headers().unwrap_or(Headers::All);
    let input_identity = std::fs::canonicalize(input).ok();
    let mut files = vec![escaped(input)];
    for path in opened {
        if input_identity
            .as_deref()
            .is_some_and(|identity| identity == Path::new(path))
        {
            continue;
        }
        if headers == Headers::User
            && system
                .iter()
                .any(|directory| Path::new(path).starts_with(directory))
        {
            continue;
        }
        let name = escaped(&shown(path, directory.as_deref()));
        if !files.contains(&name) {
            files.push(name);
        }
    }
    let targets: Vec<String> = if request.targets.is_empty() {
        vec![escaped(default_target)]
    } else {
        request
            .targets
            .iter()
            .map(|(target, quote)| {
                if *quote {
                    escaped(target)
                } else {
                    target.clone()
                }
            })
            .collect()
    };
    let mut text = String::new();
    let mut column = 0;
    let mut word = |text: &mut String, word: &str| {
        if column > 0 && column + word.len() + 1 > 76 {
            text.push_str(" \\\n ");
            column = 1;
        }
        if column > 0 {
            text.push(' ');
            column += 1;
        }
        text.push_str(word);
        column += word.len();
    };
    let head = format!("{}:", targets.join(" "));
    word(&mut text, &head);
    for file in &files {
        word(&mut text, file);
    }
    text.push('\n');
    if request.phony {
        for file in files.iter().skip(1) {
            text.push('\n');
            text.push_str(file);
            text.push_str(":\n");
        }
    }
    text
}

/// The file that `-MD` and `-MMD` write when `-MF` is absent: the output
/// with the suffix `.d`, or the input stem with `.d` in the working directory.
pub(crate) fn beside_path(output: Option<&str>, input: &str) -> String {
    match output {
        Some(output) => {
            let path = Path::new(output);
            path.with_extension("d").to_string_lossy().into_owned()
        }
        None => format!("{}.d", crate::cc::stem(input)),
    }
}
