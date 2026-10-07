//! Full-buffer editor checks reuse compiler diagnostics; no C code is executed.
use super::{
    json::{self, Json},
    locate, quote, Result,
};
use std::collections::BTreeMap;
use std::io::{self, BufRead, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn text<'a>(value: &'a Json, key: &str) -> Result<&'a str> {
    value.get(key)?.string()
}
fn integer(value: &Json) -> Result<i64> {
    match value {
        Json::Number(n) => Ok(n.parse()?),
        _ => Err("expected an integer".into()),
    }
}
fn listed(word: &str, choices: &str) -> bool {
    choices.split_whitespace().any(|choice| choice == word)
}
fn send(out: &mut impl Write, body: String) -> Result<()> {
    write!(out, "Content-Length: {}\r\n\r\n{body}", body.len())?;
    Ok(out.flush()?)
}
fn response(
    out: &mut impl Write,
    id: Json,
    result: std::result::Result<Json, (i32, String)>,
) -> Result<()> {
    let (key, value) = match result {
        Ok(value) => ("result", value.render()),
        Err((code, message)) => (
            "error",
            format!(r#"{{"code":{code},"message":{}}}"#, quote(&message)),
        ),
    };
    let id = id.render();
    send(
        out,
        format!(r#"{{"jsonrpc":"2.0","id":{id},"{key}":{value}}}"#),
    )
}
fn read_frame(input: &mut impl BufRead) -> Result<Option<String>> {
    let mut length = None;
    let mut total = 0;
    loop {
        let mut line = String::new();
        if input.take(8193).read_line(&mut line)? == 0 {
            return if total == 0 {
                Ok(None)
            } else {
                Err("incomplete protocol header".into())
            };
        }
        total += line.len();
        if total > 8192 || !line.ends_with("\r\n") {
            return Err("invalid protocol header".into());
        }
        if line == "\r\n" {
            break;
        }
        let (key, value) = line
            .trim_end()
            .split_once(':')
            .ok_or("invalid protocol header")?;
        if key.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() {
                return Err("duplicate content length".into());
            }
            length = Some(value.trim().parse::<usize>()?);
        }
    }
    let length = length
        .filter(|n| *n <= 16 * 1024 * 1024)
        .ok_or("missing or excessive content length")?;
    let mut body = vec![0; length];
    input.read_exact(&mut body)?;
    Ok(Some(String::from_utf8(body)?))
}

fn path(uri: &str) -> Result<PathBuf> {
    let encoded = uri
        .strip_prefix("file://localhost/")
        .map(|p| format!("/{p}"))
        .or_else(|| uri.strip_prefix("file://").map(str::to_owned))
        .ok_or("expected a file URI")?;
    if !encoded.starts_with('/') {
        return Err("remote file URI is not supported".into());
    }
    let mut bytes = Vec::new();
    let mut i = 0;
    while i < encoded.len() {
        if encoded.as_bytes()[i] == b'%' {
            let hex = encoded.get(i + 1..i + 3).ok_or("invalid URI escape")?;
            bytes.push(u8::from_str_radix(hex, 16)?);
            i += 3;
        } else {
            bytes.push(encoded.as_bytes()[i]);
            i += 1;
        }
    }
    if bytes.contains(&0) {
        return Err("file URI contains a zero byte".into());
    }
    Ok(PathBuf::from(String::from_utf8(bytes)?))
}
fn uri(path: &Path) -> String {
    let mut result = String::from("file://");
    for b in path.as_os_str().as_encoded_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(b) {
            result.push(*b as char);
        } else {
            result.push_str(&format!("%{b:02X}"));
        }
    }
    result
}
fn absolute(base: &Path, file: &str) -> PathBuf {
    let joined = base.join(file);
    std::fs::canonicalize(&joined).unwrap_or_else(|_| {
        let mut result = PathBuf::new();
        for part in joined.components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    result.pop();
                }
                _ => result.push(part),
            }
        }
        result
    })
}

/// Tokenize a compilation database command, never evaluate shell expansions.
fn words(command: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = command.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && quote != Some('\'') {
            let next = chars.next().ok_or("unfinished command escape")?;
            if quote == Some('"') && !"$`\"\\\n".contains(next) {
                word.push('\\');
            }
            if next != '\n' {
                word.push(next);
            }
            started = true;
        } else if quote == Some(c) {
            quote = None;
        } else if quote.is_none() && matches!(c, '\'' | '"') {
            quote = Some(c);
            started = true;
        } else if c.is_whitespace() && quote.is_none() {
            if started {
                words.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            if quote != Some('\'') && matches!(c, '$' | '`') {
                return Err("shell expansion is not supported in compile_commands".into());
            }
            if quote.is_none() && ";|&<>".contains(c) {
                return Err("shell operators are not supported in compile_commands".into());
            }
            word.push(c);
            started = true;
        }
    }
    if quote.is_some() {
        return Err("unfinished command quote".into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

fn compile_flags(root: &Path, source: &Path) -> Result<(PathBuf, String, Vec<String>)> {
    let database = root.join("compile_commands.json");
    let mut directory = root.to_owned();
    let mut input = source
        .to_str()
        .ok_or("source path is not UTF-8")?
        .to_owned();
    let mut arguments = Vec::new();
    if database.exists() {
        let records = json::parse(&std::fs::read_to_string(database)?)?;
        for row in records.array()? {
            let cwd = absolute(root, text(row, "directory")?);
            let file = text(row, "file")?;
            if absolute(&cwd, file) != absolute(root, &input) {
                continue;
            }
            arguments = if let Ok(args) = row.get("arguments") {
                args.array()?
                    .iter()
                    .map(|a| Ok(a.string()?.to_owned()))
                    .collect::<Result<_>>()?
            } else {
                words(text(row, "command")?)?
            };
            if arguments.is_empty() {
                return Err("empty compilation command".into());
            }
            directory = cwd;
            input = file.into();
            break;
        }
    }
    let mut flags = Vec::new();
    let mut args = arguments.into_iter().skip(1).flat_map(|arg| {
        arg.strip_prefix("-Wp,")
            .map(|s| {
                s.split(',')
                    .map(|s| (true, s.to_owned()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![(false, arg)])
    });
    while let Some((preprocessor, arg)) = args.next() {
        if listed(&arg, "-o -MF -MT -MQ -MJ --pass-report")
            || (preprocessor && listed(&arg, "-MD -MMD"))
        {
            args.next().ok_or("missing output option argument")?;
            continue;
        }
        if listed(&arg, "-c -S -E -M -MM -MD -MMD -MP -MG -- -fsyntax-only")
            || "-o -MF -MT -MQ -MJ -save-temps --emit= --repair="
                .split_whitespace()
                .any(|p| arg.starts_with(p))
        {
            continue;
        }
        if listed(&arg, "-I -D -U -include -imacros -isystem -iquote -idirafter -isysroot --sysroot --target -target -arch -x") {
            flags.push(arg);
            flags.push(args.next().ok_or("missing compilation option argument")?.1);
            continue;
        }
        if arg.starts_with('@') {
            return Err("editor commands require expanded response files".into());
        }
        if !arg.starts_with('-') && absolute(&directory, &arg) == absolute(&directory, &input) {
            input = arg;
            continue;
        }
        flags.push(arg);
    }
    Ok((directory, input, flags))
}

/// The compiler contract uses byte columns; LSP positions use UTF-16 units.
fn range(span: &Json, source: &str) -> Result<Json> {
    let position = |line: &str, column: &str| -> Result<Json> {
        let line = usize::try_from(integer(span.get(line)?)?)?
            .checked_sub(1)
            .ok_or("zero source line")?;
        let column = usize::try_from(integer(span.get(column)?)?)?
            .checked_sub(1)
            .ok_or("zero source column")?;
        let content = source
            .split('\n')
            .nth(line)
            .ok_or("source line is outside buffer")?;
        let prefix = content
            .get(..column)
            .ok_or("source column is not a UTF-8 boundary")?;
        json::parse(&format!(
            r#"{{"line":{line},"character":{}}}"#,
            prefix.encode_utf16().count()
        ))
    };
    json::parse(&format!(
        r#"{{"start":{},"end":{}}}"#,
        position("line_start", "column_start")?.render(),
        position("line_end", "column_end")?.render()
    ))
}

struct Document {
    path: PathBuf,
    text: String,
    version: i64,
    due: Option<Instant>,
    actions: Vec<Json>,
    published: BTreeMap<String, Vec<Json>>,
}
struct Server {
    root: PathBuf,
    compiler: PathBuf,
    initialized: bool,
    shutdown: bool,
    documents: BTreeMap<String, Document>,
}
impl Server {
    // Keep each translation unit's contribution until that unit is rechecked
    // or closed. Closing one includer must not clear another's header errors.
    fn publish(&self, key: &str, out: &mut impl Write) -> Result<()> {
        let diagnostics = self
            .documents
            .values()
            .filter_map(|d| d.published.get(key))
            .flatten()
            .cloned()
            .collect();
        let version = self
            .documents
            .get(key)
            .map_or(String::new(), |d| format!(r#", "version":{}"#, d.version));
        send(
            out,
            format!(
                r#"{{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{{"uri":{},"diagnostics":{}{version}}}}}"#,
                quote(key),
                Json::Array(diagnostics).render()
            ),
        )
    }
    fn check(&mut self, key: &str, out: &mut impl Write) -> Result<()> {
        let document = self.documents.get_mut(key).ok_or("document is not open")?;
        document.due = None;
        document.actions.clear();
        let mut groups = BTreeMap::from([(key.to_owned(), Vec::<Json>::new())]);
        let checked: Result<()> = (|| {
            let (cwd, input, flags) = compile_flags(&self.root, &document.path)?;
            let mut child = Command::new(&self.compiler)
                .current_dir(&cwd)
                .arg("-Wall")
                .args(flags)
                .args(
                    "-fsyntax-only --source-from-stdin --error-format=json --color=never"
                        .split_whitespace(),
                )
                .arg(&input)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;
            // Writing on a separate thread avoids pipe deadlock on large buffers.
            let mut stdin = child.stdin.take().ok_or("compiler stdin is absent")?;
            let bytes = document.text.clone();
            let writer = std::thread::spawn(move || stdin.write_all(bytes.as_bytes()));
            let output = child.wait_with_output()?;
            let written = writer.join().map_err(|_| "compiler input worker failed")?;
            if output.status.success() {
                written?;
            }
            if output.stderr.is_empty() && !output.status.success() {
                return Err("compiler failed without a diagnostic".into());
            }
            for line in std::str::from_utf8(&output.stderr)?.lines() {
                let record = json::parse(line)?;
                let span = record
                    .get("spans")?
                    .array()?
                    .iter()
                    .find(|s| s.get("primary").ok() == Some(&Json::Bool(true)))
                    .ok_or("missing primary span")?;
                let file = text(span, "file")?;
                if file.starts_with('<') {
                    return Err(text(&record, "message")?.into());
                }
                let file = absolute(&cwd, file);
                let own = file == absolute(&cwd, input.as_str());
                let source = if own {
                    document.text.clone()
                } else {
                    std::fs::read_to_string(&file)?
                };
                let target = if own { key.to_owned() } else { uri(&file) };
                let severity = if text(&record, "level")? == "error" {
                    1
                } else {
                    2
                };
                let diagnostic = json::parse(&format!(
                    r#"{{"range":{},"severity":{severity},"code":{},"source":"evocc","message":{}}}"#,
                    range(span, &source)?.render(),
                    record.get("code")?.render(),
                    record.get("message")?.render()
                ))?;
                groups.entry(target).or_default().push(diagnostic.clone());
                if !own {
                    continue;
                }
                for repair in record.get("repairs")?.array()? {
                    if text(repair, "kind")? != "safe"
                        || absolute(&cwd, text(repair, "file")?) != file
                    {
                        continue;
                    }
                    document.actions.push(json::parse(&format!(
                        r#"{{"title":{},"kind":"quickfix","diagnostics":[{}],"edit":{{"documentChanges":[{{"textDocument":{{"uri":{},"version":{}}},"edits":[{{"range":{},"newText":{}}}]}}]}}}}"#,
                        repair.get("title")?.render(), diagnostic.render(), quote(key), document.version,
                        range(repair.get("span")?, &document.text)?.render(), repair.get("replacement")?.render()))?);
                }
            }
            Ok(())
        })();
        if let Err(error) = checked {
            document.actions.clear();
            groups.clear();
            let message = quote(&format!("cannot check editor buffer: {error}"));
            groups.insert(key.into(), vec![json::parse(&format!(
                r#"{{"range":{{"start":{{"line":0,"character":0}},"end":{{"line":0,"character":0}}}},"severity":1,"code":"editor-check","source":"evo","message":{message}}}"#))?]);
        }
        let affected: std::collections::BTreeSet<_> = document
            .published
            .keys()
            .chain(groups.keys())
            .cloned()
            .collect();
        document.published = groups;
        for target in affected {
            self.publish(&target, out)?;
        }
        Ok(())
    }

    fn dispatch(&mut self, method: &str, params: &Json, out: &mut impl Write) -> Result<Json> {
        match method {
            "initialize" => {
                if self.initialized {
                    return Err("server is already initialized".into());
                }
                if let Ok(Json::String(root)) = params.get("rootUri") {
                    self.root = path(root)?;
                } else if let Ok(folders) = params.get("workspaceFolders").and_then(Json::array) {
                    if let Some(folder) = folders.first() {
                        self.root = path(text(folder, "uri")?)?;
                    }
                }
                self.initialized = true;
                return json::parse(&format!(
                    r#"{{"capabilities":{{"positionEncoding":"utf-16","textDocumentSync":{{"openClose":true,"change":1,"save":{{"includeText":true}}}},"codeActionProvider":true}},"serverInfo":{{"name":"evo","version":"{}"}}}}"#,
                    env!("CARGO_PKG_VERSION")
                ));
            }
            "initialized" => return Ok(Json::Null),
            "shutdown" => {
                self.shutdown = true;
                return Ok(Json::Null);
            }
            _ => {}
        }
        let td = params.get("textDocument")?;
        let key = text(td, "uri")?;
        match method {
            "textDocument/didOpen" => {
                self.documents.insert(
                    key.into(),
                    Document {
                        path: path(key)?,
                        text: text(td, "text")?.into(),
                        version: integer(td.get("version")?)?,
                        due: None,
                        actions: Vec::new(),
                        published: BTreeMap::new(),
                    },
                );
                self.check(key, out)?;
            }
            "textDocument/didChange" => {
                let document = self.documents.get_mut(key).ok_or("document is not open")?;
                let version = integer(td.get("version")?)?;
                if version <= document.version {
                    return Err("document version did not increase".into());
                }
                let changes = params.get("contentChanges")?.array()?;
                let mut replacement = document.text.clone();
                for change in changes {
                    if change.get("range").is_ok() {
                        return Err("incremental text changes are not supported".into());
                    }
                    replacement = text(change, "text")?.into();
                }
                document.text = replacement;
                document.version = version;
                document.actions.clear();
                document.due = Some(Instant::now() + Duration::from_millis(300));
            }
            "textDocument/didSave" => {
                let document = self.documents.get_mut(key).ok_or("document is not open")?;
                if let Ok(content) = text(params, "text") {
                    document.text = content.into();
                }
                self.check(key, out)?;
            }
            "textDocument/didClose" => {
                if let Some(document) = self.documents.remove(key) {
                    for target in document.published.keys() {
                        self.publish(target, out)?;
                    }
                }
            }
            "textDocument/codeAction" => {
                if self.documents.get(key).is_some_and(|d| d.due.is_some()) {
                    self.check(key, out)?;
                }
                let document = self.documents.get(key).ok_or("document is not open")?;
                let context = params.get("context")?;
                if let Ok(only) = context.get("only").and_then(Json::array) {
                    if !only
                        .iter()
                        .any(|kind| kind.string().is_ok_and(|s| s == "quickfix" || s.is_empty()))
                    {
                        return Ok(Json::Array(Vec::new()));
                    }
                }
                let requested = params.get("range")?;
                let point = |value: &Json| -> Result<(i64, i64)> {
                    Ok((
                        integer(value.get("line")?)?,
                        integer(value.get("character")?)?,
                    ))
                };
                let start = point(requested.get("start")?)?;
                let end = point(requested.get("end")?)?;
                let mut actions = Vec::new();
                for action in &document.actions {
                    let range = action.get("diagnostics")?.array()?[0].get("range")?;
                    if point(range.get("start")?)? <= end && start <= point(range.get("end")?)? {
                        actions.push(action.clone());
                    }
                }
                return Ok(Json::Array(actions));
            }
            _ => return Err("unknown editor method".into()),
        }
        Ok(Json::Null)
    }
}

pub fn serve() -> Result<i32> {
    let mut server = Server {
        root: std::env::current_dir()?,
        compiler: locate("evocc", "EVOCC")?,
        initialized: false,
        shutdown: false,
        documents: BTreeMap::new(),
    };
    let (sender, receiver) = mpsc::sync_channel(32);
    std::thread::spawn(move || {
        let mut input = io::stdin().lock();
        while let Some(frame) = read_frame(&mut input).transpose() {
            let failed = frame.is_err();
            if sender.send(frame.map_err(|e| e.to_string())).is_err() || failed {
                break;
            }
        }
    });
    let mut out = io::stdout().lock();
    loop {
        let next = server.documents.values().filter_map(|d| d.due).min();
        let message = if let Some(deadline) = next {
            receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        } else {
            receiver
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        };
        let body = match message {
            Ok(Ok(body)) => body,
            Ok(Err(message)) => return Err(message.into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(i32::from(!server.shutdown)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let ready: Vec<_> = server
                    .documents
                    .iter()
                    .filter(|(_, d)| d.due.is_some_and(|t| t <= Instant::now()))
                    .map(|(key, _)| key.clone())
                    .collect();
                for key in ready {
                    server.check(&key, &mut out)?;
                }
                continue;
            }
        };
        let message = match json::parse(&body) {
            Ok(message) => message,
            Err(error) => {
                response(&mut out, Json::Null, Err((-32700, error.to_string())))?;
                continue;
            }
        };
        let id = message.get("id").ok().cloned();
        let method = text(&message, "method").unwrap_or("");
        if text(&message, "jsonrpc").ok() != Some("2.0")
            || method.is_empty()
            || id
                .as_ref()
                .is_some_and(|v| !matches!(v, Json::Number(_) | Json::String(_)))
        {
            response(
                &mut out,
                Json::Null,
                Err((-32600, "invalid protocol request".into())),
            )?;
            continue;
        }
        if method == "exit" && id.is_none() {
            return Ok(i32::from(!server.shutdown));
        }
        let empty = Json::Object(BTreeMap::new());
        let params = message.get("params").unwrap_or(&empty);
        let result = if !server.initialized && method != "initialize" {
            Err((-32002, "server is not initialized".into()))
        } else if server.shutdown {
            Err((-32600, "server is shut down".into()))
        } else if !listed(method, "initialize initialized shutdown textDocument/didOpen textDocument/didChange textDocument/didSave textDocument/didClose textDocument/codeAction") {
            Err((-32601, "method is not supported".into()))
        } else {
            server
                .dispatch(method, params, &mut out)
                .map_err(|e| (-32602, e.to_string()))
        };
        if let Some(id) = id {
            response(&mut out, id, result)?;
        } else if let Err((_, message)) = result {
            if !method.starts_with("$/") {
                eprintln!("evo lsp: {message}");
            }
        }
    }
}
