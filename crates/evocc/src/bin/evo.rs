//! User commands delegate compilation and checking to the installed compiler.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::os::unix::{ffi::OsStrExt, process::ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Output};
use std::time::{SystemTime, UNIX_EPOCH};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[path = "evo/json.rs"]
mod json;
#[path = "evo/lsp.rs"]
mod lsp;
#[path = "evo/mcp.rs"]
mod mcp;

fn json_requested() -> bool {
    std::env::var_os("EVO_BUILD_TRACE").is_none()
        && std::env::args_os()
            .skip(1)
            .take_while(|a| a != "--")
            .any(|a| a == "--json")
}

fn json_error(error: &str, status: i32) {
    println!("{{\"error\":{},\"exit_code\":{status}}}", quote(error));
}

fn code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(1))
}

fn quote(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn locate(name: &str, variable: &str) -> Result<PathBuf> {
    let sibling = std::env::current_exe()?.with_file_name(name);
    let path = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if sibling.is_file() {
                sibling
            } else if name == "evoir-sem" {
                sibling
                    .parent()
                    .and_then(|dir| {
                        dir.ancestors().find(|root| {
                            root.join("Cargo.toml").is_file() && root.join("lean").is_dir()
                        })
                    })
                    .map(|root| root.join("lean/.lake/build/bin/evoir-sem"))
                    .filter(|path| path.is_file())
                    .unwrap_or_else(|| PathBuf::from(name))
            } else {
                PathBuf::from(name)
            }
        });
    if path.is_file() {
        return Ok(fs::canonicalize(path)?);
    }
    if path.components().count() == 1 {
        for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
            let candidate = dir.join(&path);
            if candidate.is_file() {
                return Ok(fs::canonicalize(candidate)?);
            }
        }
    }
    Err(format!("cannot find {}; set {variable}", path.display()).into())
}

fn temporary(prefix: &str) -> Result<PathBuf> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = std::env::temp_dir().join(format!("{prefix}-{}-{stamp}", std::process::id()));
    fs::create_dir(&path)?;
    Ok(path)
}

fn forward(output: &Output) -> Result<()> {
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    Ok(())
}

fn fix(compiler: &Path, args: &[String]) -> Result<i32> {
    let (flags, files): (Vec<_>, Vec<_>) = args
        .iter()
        .map(String::as_str)
        .partition(|s| s.starts_with('-'));
    let file = match files.as_slice() {
        [file] => *file,
        [] => return Err("fix requires a C file".into()),
        _ => return Err("fix requires exactly one C file".into()),
    };
    let json = flags.contains(&"--json");
    let mut flags: Vec<_> = flags.into_iter().filter(|s| *s != "--json").collect();
    flags.sort_unstable();
    let mode = match flags.as_slice() {
        ["--plan"] => "plan",
        ["--apply"] => "safe",
        ["--apply", "--review"] => "review",
        _ => return Err("fix requires --plan or --apply [--review], with optional --json".into()),
    };
    let format = if json { "json" } else { "text" };
    let output = Command::new(compiler)
        .args(["--parse-only", "-Wall", "--cap-lints=warn", "--color=never"])
        .arg(format!("--repair={mode}"))
        .arg(format!("--error-format={format}"))
        .arg(file)
        .output()?;
    if json && output.stdout.is_empty() {
        let rows = std::str::from_utf8(&output.stderr)?
            .lines()
            .collect::<Vec<_>>()
            .join(",");
        println!("{{\"file\":{},\"repairs\":[],\"diagnostics\":[{rows}],\"applied\":0,\"exit_code\":{}}}", quote(file), code(output.status));
    } else {
        forward(&output)?;
    }
    Ok(code(output.status))
}

fn run(compiler: &Path, args: &[OsString]) -> Result<i32> {
    let file = args.first().ok_or("usage: evo run FILE.c [-- ARGS]")?;
    if args.len() > 1 && args[1] != "--" {
        return Err("program arguments must follow --".into());
    }
    let mut command = Command::new(compiler);
    command
        .args(["--interpret-c-hir", "--color=never"])
        .arg(file);
    for argument in std::iter::once(file).chain(args.iter().skip(2)) {
        let hex: String = argument
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        command.arg(format!("--program-arg-hex={hex}"));
    }
    let output = command.output()?;
    io::stderr().write_all(&output.stderr)?;
    if !output.status.success() {
        return Ok(code(output.status));
    }
    let value: i128 = std::str::from_utf8(&output.stdout)?.trim().parse()?;
    Ok((value as u8).into())
}

fn prove(compiler: &Path, args: &[String], json: bool) -> Result<i32> {
    let input = args
        .first()
        .ok_or("usage: evo prove FILE.c -o OUT.s [--entry NAME ...]")?;
    let mut out = None;
    let mut entries = BTreeSet::new();
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().ok_or("missing output path or entry name")?;
        match flag.as_str() {
            "-o" if out.is_none() => out = Some(value),
            "--entry" => {
                entries.insert(value.clone());
            }
            _ => return Err(format!("unknown prove option: {flag}").into()),
        }
    }
    let out = out.ok_or("prove requires -o OUT.s")?;
    let root = temporary("evo-proof")?;
    let mut errors = Vec::new();
    let mut checker_path = "null".to_owned();
    let result: Result<i32> = (|| {
        if entries.is_empty() {
            let listed = Command::new(compiler)
                .args(["--list-defined-functions", input])
                .output()?;
            errors.extend_from_slice(&listed.stderr);
            if !listed.status.success() {
                return Ok(code(listed.status));
            }
            entries.extend(
                std::str::from_utf8(&listed.stdout)?
                    .lines()
                    .map(str::to_owned),
            );
        }
        let checker = locate("evoir-sem", "EVOCC_PROOF_CHECKER")?;
        checker_path = quote(&checker.to_string_lossy());
        let digest = evocc_proof::sha256_hex(&fs::read(&checker)?);
        let proof = root.join("checked");
        let output = Command::new(compiler)
            .args([
                "--emit=asm",
                "--target=aarch64-unknown-linux-gnu",
                "-O0",
                "-fno-builtin",
                "--color=never",
            ])
            .arg(format!("--proof-required={}", proof.display()))
            .arg(format!("--proof-checker={}", checker.display()))
            .arg(format!("--proof-checker-sha256={digest}"))
            .args(entries.iter().map(|name| format!("--core-entry={name}")))
            .arg(input)
            .output()?;
        if !output.status.success() {
            errors.extend_from_slice(&output.stderr);
            return Ok(code(output.status));
        }
        for line in output.stderr.split_inclusive(|b| *b == b'\n') {
            if !line.starts_with(b"evocc: proof-required result:") {
                errors.extend_from_slice(line);
            }
        }
        fs::copy(proof.join("output.s"), out)?;
        if !json {
            println!(
                "checked from typed Core source to final assembly text: {}",
                proof.join("record.json").display()
            );
        }
        Ok(0)
    })();
    if json {
        let status = match &result {
            Ok(status) => *status,
            Err(error) => {
                errors.extend_from_slice(error.to_string().as_bytes());
                1
            }
        };
        let record = root.join("checked/record.json");
        let (label, stages, path) = if record.is_file() {
            let record_data = json::parse(&fs::read_to_string(&record)?)?;
            let stages = record_data
                .get("stages")?
                .array()?
                .iter()
                .map(|s| {
                    Ok(format!(
                        "{{\"id\":{},\"status\":{}}}",
                        quote(s.get("id")?.string()?),
                        quote(s.get("status")?.string()?)
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            (
                quote(record_data.get("result")?.string()?),
                stages.join(","),
                quote(&record.to_string_lossy()),
            )
        } else {
            ("null".into(), String::new(), "null".into())
        };
        println!("{{\"result\":{label},\"stages\":[{stages}],\"record_path\":{path},\"proof_directory\":{},\"checker\":{checker_path},\"exit_code\":{status},\"stderr\":{}}}", quote(&root.to_string_lossy()), quote(std::str::from_utf8(&errors)?));
        return Ok(status);
    }
    io::stderr().write_all(&errors)?;
    if !matches!(result, Ok(0)) {
        eprintln!("evo: proof files kept at {}", root.display());
    }
    result
}

// Make invokes this same binary as CC. Each process writes one private record;
// parallel compiler calls never share a partially written record.
fn compile_record(args: &[String], trace: &Path) -> Result<i32> {
    let compiler = std::env::var_os("EVO_BUILD_COMPILER").ok_or("missing build compiler")?;
    let output = Command::new(compiler).args(args).output()?;
    let sources = if output.status.success()
        && !args
            .iter()
            .any(|a| matches!(a.as_str(), "-E" | "-M" | "-MM" | "-fsyntax-only"))
    {
        args.iter()
            .enumerate()
            .filter(|(i, a)| {
                a.ends_with(".c")
                    && !a.starts_with('-')
                    && (*i == 0
                        || !matches!(
                            args[i - 1].as_str(),
                            "-o" | "-include"
                                | "-imacros"
                                | "-I"
                                | "-isystem"
                                | "-iquote"
                                | "-idirafter"
                                | "-MF"
                                | "-MT"
                                | "-MQ"
                                | "--sysroot"
                        ))
            })
            .map(|(_, s)| {
                quote(
                    &fs::canonicalize(s)
                        .unwrap_or_else(|_| PathBuf::from(s))
                        .to_string_lossy(),
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };
    let warnings = warning_counts(&String::from_utf8_lossy(&output.stderr));
    let target = args
        .iter()
        .find_map(|a| a.strip_prefix("--target="))
        .unwrap_or("");
    let record = format!(
        "{target}\n{}\n{sources}\n{}",
        sources.lines().count(),
        warnings
            .iter()
            .map(|(k, v)| format!("{k}={v}\n"))
            .collect::<String>()
    );
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    fs::write(
        trace.join(format!("{}-{stamp}.record", std::process::id())),
        record,
    )?;
    forward(&output)?;
    Ok(code(output.status))
}

fn warning_counts(text: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("warning[") {
            if let Some((code, _)) = rest.split_once(']') {
                *counts.entry(code.into()).or_default() += 1;
            }
        }
    }
    counts
}

fn build(compiler: &Path, args: &[String], json: bool) -> Result<i32> {
    let split = args.iter().position(|a| a == "--").unwrap_or(args.len());
    if split > 1 {
        return Err("usage: evo build [DIR] [-- MAKE_ARGS]".into());
    }
    let directory = fs::canonicalize(
        args.first()
            .filter(|_| split == 1)
            .map_or(".", String::as_str),
    )?;
    let version = Command::new(compiler).arg("--version").output()?;
    if !version.status.success() {
        if json {
            json_error(std::str::from_utf8(&version.stderr)?, code(version.status));
        } else {
            forward(&version)?;
        }
        return Ok(code(version.status));
    }
    let target = Command::new(compiler).arg("-dumpmachine").output()?;
    if !target.status.success() {
        if json {
            json_error(std::str::from_utf8(&target.stderr)?, code(target.status));
        } else {
            forward(&target)?;
        }
        return Ok(code(target.status));
    }
    let mut target = String::from_utf8(target.stdout)?.trim().to_owned();
    let trace = temporary("evo-build")?;
    let launcher = std::env::current_exe()?;
    let path = launcher.to_str().ok_or("compiler path is not UTF-8")?;
    let cc = if path
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"/_-.".contains(&b))
    {
        path.to_owned()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    };
    let log = fs::File::create(directory.join("evo-build.log"))?;
    let mut steps = Vec::new();
    if directory.join("configure").is_file() {
        steps.push(vec!["./configure".to_owned()]);
    }
    let mut make = vec!["make".into()];
    make.extend(args.iter().skip(split + 1).cloned());
    make.push(format!("CC={cc}"));
    steps.push(make);
    let mut result = 0;
    let mut failure = "null".to_owned();
    for step in steps {
        let status = Command::new(&step[0])
            .args(&step[1..])
            .current_dir(&directory)
            .env("CC", &cc)
            .env("EVO_BUILD_TRACE", &trace)
            .env("EVO_BUILD_COMPILER", compiler)
            .stdout(log.try_clone()?)
            .stderr(log.try_clone()?)
            .status();
        result = status.map(code).unwrap_or(1);
        if result != 0 {
            failure = format!(
                "[{}]",
                step.iter().map(|s| quote(s)).collect::<Vec<_>>().join(",")
            );
            break;
        }
    }
    let mut sources = BTreeSet::new();
    let mut warnings = BTreeMap::<String, usize>::new();
    for entry in fs::read_dir(&trace)? {
        let text = fs::read_to_string(entry?.path())?;
        let mut lines = text.lines();
        if let Some(found) = lines.next().filter(|s| !s.is_empty()) {
            target = found.into();
        }
        let count: usize = lines.next().ok_or("missing source count")?.parse()?;
        for _ in 0..count {
            sources.insert(lines.next().ok_or("missing source")?.to_owned());
        }
        for item in lines.filter(|s| !s.is_empty()) {
            let (name, count) = item.split_once('=').ok_or("invalid warning count")?;
            *warnings.entry(name.into()).or_default() += count.parse::<usize>()?;
        }
    }
    let warnings = warnings
        .iter()
        .map(|(k, v)| format!("{}:{v}", quote(k)))
        .collect::<Vec<_>>()
        .join(",");
    let receipt = format!("{{\"version\":{},\"target\":{},\"sources\":[{}],\"warnings\":{{{warnings}}},\"failing_command\":{failure},\"exit_code\":{result}}}\n",
        quote(String::from_utf8_lossy(&version.stdout).lines().next().unwrap_or("")), quote(&target), sources.into_iter().collect::<Vec<_>>().join(","));
    fs::write(directory.join("evo-build-receipt.json"), &receipt)?;
    fs::remove_dir_all(trace)?;
    if json {
        print!("{receipt}");
    } else {
        println!(
            "build exit {result}; log: {}",
            directory.join("evo-build.log").display()
        );
    }
    Ok(result)
}

fn execute() -> Result<i32> {
    let mut raw: Vec<_> = std::env::args_os().skip(1).collect();
    if let Some(trace) = std::env::var_os("EVO_BUILD_TRACE") {
        let args = raw
            .into_iter()
            .map(|s| {
                s.into_string()
                    .map_err(|_| "compiler argument is not UTF-8")
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        return compile_record(&args, Path::new(&trace));
    }
    let json = json_requested();
    let end = raw.iter().position(|a| a == "--").unwrap_or(raw.len());
    for i in (0..end).rev() {
        if raw[i] == "--json" {
            raw.remove(i);
        }
    }
    let verb = raw
        .first()
        .map_or(Ok("help"), |s| s.to_str().ok_or("command is not UTF-8"))?;
    let usage = match verb {
        "help" | "--help" if raw.len() <= 1 || (raw.len() == 2 && raw[1] == "--json") => {
            "evo run|check|explain|prove|build|fix|mcp|lsp|version|help"
        }
        "run" => "evo run FILE.c [-- ARGS]",
        "check" => "evo check FILE.c",
        "explain" => "evo explain CODE",
        "prove" => "evo prove FILE.c -o OUT.s [--entry NAME ...]",
        "build" => "evo build [DIR] [-- MAKE_ARGS]",
        "fix" => "evo fix FILE.c --plan|--apply [--review] [--json]",
        "mcp" => "evo mcp",
        "lsp" => "evo lsp",
        "version" => "evo version",
        _ => return Err(format!("unknown command: {verb}").into()),
    };
    if matches!(verb, "help" | "--help")
        || ((raw.len() == 2 || (raw.len() == 3 && raw[2] == "--json")) && raw[1] == "--help")
    {
        let status = if raw.is_empty() { 64 } else { 0 };
        let message = format!("usage: {usage}");
        if json {
            println!("{{\"usage\":{},\"exit_code\":{status}}}", quote(&message));
        } else {
            println!("{message}");
        }
        return Ok(status);
    }
    if matches!(verb, "mcp" | "lsp") {
        if raw.len() != 1 {
            return Err(format!("usage: {usage}").into());
        }
        return if verb == "mcp" {
            mcp::serve()
        } else {
            lsp::serve()
        };
    }
    let compiler = locate("evocc", "EVOCC")?;
    if verb == "run" {
        if json {
            raw.get(1).ok_or("run requires a C file")?;
            let output = Command::new(std::env::current_exe()?).args(&raw).output()?;
            let status = code(output.status);
            println!(
                "{{\"exit_code\":{status},\"stdout\":{},\"stderr\":{}}}",
                quote(std::str::from_utf8(&output.stdout)?),
                quote(std::str::from_utf8(&output.stderr)?)
            );
            return Ok(status);
        }
        return run(&compiler, &raw[1..]);
    }
    let mut args = raw[1..]
        .iter()
        .map(|s| s.to_str().map(str::to_owned).ok_or("argument is not UTF-8"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    match verb {
        "prove" => prove(&compiler, &args, json),
        "build" => build(&compiler, &args, json),
        "fix" => {
            if json {
                args.push("--json".into());
            }
            fix(&compiler, &args)
        }
        "check" | "explain" | "version" => {
            if args.len() != usize::from(verb != "version") {
                return Err(format!("usage: {usage}").into());
            }
            let flag = match verb {
                "check" => "--check",
                "explain" => "--explain",
                _ => "--version",
            };
            let mut command = Command::new(compiler);
            command.arg(flag).args(&args);
            if json && verb == "check" {
                command.arg("--error-format=json");
            }
            let output = command.output()?;
            if !json {
                forward(&output)?;
            } else if verb == "check" {
                let diagnostics = std::str::from_utf8(&output.stderr)?
                    .lines()
                    .map(json::parse)
                    .collect::<Result<Vec<_>>>()?;
                let completion = if output.stdout.is_empty() {
                    json::Json::Null
                } else {
                    json::parse(std::str::from_utf8(&output.stdout)?)?
                };
                println!(
                    "{{\"exit_code\":{},\"completion\":{},\"diagnostics\":{}}}",
                    code(output.status),
                    completion.render(),
                    json::Json::Array(diagnostics).render()
                );
            } else if !output.status.success() {
                json_error(std::str::from_utf8(&output.stderr)?, code(output.status));
            } else {
                let text = std::str::from_utf8(&output.stdout)?;
                if verb == "version" {
                    println!("{{\"version\":{},\"exit_code\":0}}", quote(text.trim()));
                } else {
                    let header = format!("# {}: ", args[0]);
                    let title = text
                        .lines()
                        .next()
                        .and_then(|s| s.strip_prefix(&header))
                        .ok_or("invalid explanation heading")?;
                    println!(
                        "{{\"code\":{},\"title\":{},\"text\":{},\"exit_code\":0}}",
                        quote(&args[0]),
                        quote(title),
                        quote(text)
                    );
                }
            }
            Ok(code(output.status))
        }
        _ => unreachable!(),
    }
}

fn main() -> ExitCode {
    match execute() {
        Ok(status) => ExitCode::from(status as u8),
        Err(error) => {
            if json_requested() {
                json_error(&error.to_string(), 1);
            } else {
                eprintln!("evo: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
