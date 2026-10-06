//! The cc command line. It takes several inputs and one action: `-E`, `-S`,
//! `-c`, or link when no action is named. The driver compiles each C input in
//! this process. The platform assembler and linker make the objects and the
//! executable.

use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::deps::{self, Headers, Request};
use crate::toolchain::{self, Scratch, Style, Tools};
use crate::{diagnostics, DriverTarget};

/// The version that `--version`, `-v` and `-dumpversion` print.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The first line of `--version` and `-v`.
fn version_line() -> String {
    format!(
        "evocc {VERSION} ({} {})",
        env!("EVOCC_BUILD_COMMIT"),
        env!("EVOCC_BUILD_DATE")
    )
}

/// `--version`: the version line and the target, on standard output.
pub(crate) fn print_version(args: &[String]) -> ExitCode {
    let target = explicit_target(args).unwrap_or_else(DriverTarget::host);
    println!("{}\nTarget: {}", version_line(), target.triple());
    ExitCode::SUCCESS
}

/// The target that the command line names, if it names a valid one.
fn explicit_target(args: &[String]) -> Option<DriverTarget> {
    let mut found = None;
    let mut index = 0;
    while index < args.len() {
        let value = match args[index].as_str() {
            "--target" | "-target" => {
                index += 1;
                args.get(index).map(String::as_str)
            }
            arg => arg.strip_prefix("--target="),
        };
        if let Some(target) = value.and_then(|value| crate::parse_target(value).ok()) {
            found = Some(target);
        }
        index += 1;
    }
    found
}

const HELP: &str = "\
usage: evocc [OPTIONS] INPUT...

Actions (the default action links an executable):
  -E                 Preprocess to standard output or to -o FILE.
  -S                 Compile to assembly, NAME.s.
  -c                 Compile and assemble to an object, NAME.o.
  -fsyntax-only      Check the input and write nothing.
  -o FILE            Write the output to FILE. The default executable is a.out.

Inputs: .c and .i are C, .s is assembly, .S is assembly with the C
preprocessor, .o .a .so .dylib .tbd go to the linker. -x LANG names the
language of the next inputs: c, assembler, assembler-with-cpp or none.

Compilation:
  -I DIR -D NAME[=VALUE] -U NAME -include FILE -isystem DIR -nostdinc
  -std=c89|c99|c11|c17|gnu89|gnu99|gnu11|gnu17  -O0 -O1 -O2 -O3 -Os -Og -g
  -f... -m...        The known options take effect. The driver ignores
                     the others with one warning.
  -Wall -Wextra      Enable the five supported warnings.
  -Werror -Werror=NAME -Wno-NAME --cap-lints=allow|warn|deny
  -w                 Do not print warnings.
  --target=aarch64-apple-darwin|aarch64-unknown-linux-gnu
  --sysroot=DIR, -isysroot DIR   The root of the target's headers.
  EVOCC_SYSROOT       Linux header root when --sysroot is absent.
                     Otherwise use ../sysroot/TARGET beside the executable directory.

Dependencies: -M -MM -MD -MMD -MF FILE -MT TARGET -MQ TARGET -MP

Linking: -l LIB -L DIR -Wl,ARGS -Xlinker ARG -shared -static -pthread
Tools: --assembler=CMD (EVOCC_AS), --linker=CMD (EVOCC_CC)
Information: --version -v -dumpmachine -dumpversion --help
Development actions: --emit=asm|evoir|c-hir, --parse-only, --check and
the others take one input and print to standard output.
";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Link,
    Object,
    Assembly,
    SyntaxOnly,
    Preprocess,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    C,
    Assembly,
    AssemblyWithCpp,
    Linker,
}

/// The language that a `-x` option selects. `None` selects the file suffix.
fn language(value: &str) -> Result<Option<Kind>, String> {
    match value {
        "c" | "cpp-output" => Ok(Some(Kind::C)),
        "assembler" => Ok(Some(Kind::Assembly)),
        "assembler-with-cpp" => Ok(Some(Kind::AssemblyWithCpp)),
        "none" => Ok(None),
        _ => Err(format!(
            "unsupported language `{value}` for -x; use c, assembler, assembler-with-cpp or none"
        )),
    }
}

/// The kind of an input from its file name.
fn kind_of(path: &str) -> Option<Kind> {
    let name = Path::new(path).file_name()?.to_str()?;
    if name.contains(".so.") {
        return Some(Kind::Linker);
    }
    match Path::new(name).extension()?.to_str()? {
        "c" | "i" => Some(Kind::C),
        "s" => Some(Kind::Assembly),
        "S" | "sx" => Some(Kind::AssemblyWithCpp),
        "o" | "a" | "so" | "dylib" | "tbd" => Some(Kind::Linker),
        _ => None,
    }
}

/// The file name of `input` without its directory and its last suffix.
pub(crate) fn stem(input: &str) -> String {
    if input == "-" {
        return "-".to_owned();
    }
    Path::new(input).file_stem().map_or_else(
        || "input".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    )
}

/// One input as the command line names it.
struct Input {
    path: String,
    /// The `-x` language in effect at this input.
    language: Option<Kind>,
}

/// One argument of the link command, in command-line order.
enum LinkItem {
    Input(usize),
    Argument(String),
}

#[derive(Clone, Copy)]
enum Query {
    Machine,
    Version,
    Help,
}

struct Command {
    action: Action,
    output: Option<String>,
    inputs: Vec<Input>,
    link: Vec<LinkItem>,
    /// The options of each compilation, for the option parser of `main.rs`.
    compile: Vec<String>,
    /// `-I`, `-D`, `-U`, `-include` and `-isystem` for `.S` inputs.
    preprocessor: Vec<String>,
    /// `-Wa,` and `-Xassembler` arguments, in cc spelling.
    assembler_flags: Vec<String>,
    target: Option<DriverTarget>,
    sysroot: Option<String>,
    deps: Request,
    verbose: bool,
    quiet: bool,
    debug: bool,
    assembler: Option<String>,
    linker: Option<String>,
    query: Option<Query>,
    /// Options that the cc command line itself ignores.
    ignored: Vec<String>,
}

fn take<'a>(args: &'a [String], index: &mut usize, option: &str) -> Result<&'a str, String> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires an argument"))
}

/// Link options without a value.
const LINK_FLAGS: &[&str] = &[
    "-shared",
    "-static",
    "-rdynamic",
    "-pie",
    "-no-pie",
    "-nostdlib",
    "-nostartfiles",
    "-nodefaultlibs",
    "-static-libgcc",
    "-shared-libgcc",
    "-static-pie",
    "-dynamiclib",
    "-dynamic",
    "-bundle",
    "-s",
    "-headerpad_max_install_names",
];

/// Link options that take one value.
const LINK_OPTIONS: &[&str] = &[
    "-u",
    "-T",
    "-z",
    "-e",
    "-framework",
    "-weak_framework",
    "-install_name",
    "-compatibility_version",
    "-current_version",
    "-undefined",
    "-exported_symbols_list",
    "-unexported_symbols_list",
    "-rpath",
];

/// Compilation options that take one value.
const COMPILE_OPTIONS: &[&str] = &["-I", "-D", "-U", "-include", "-isystem", "-imacros"];

/// The development options that a cc compilation passes to the option parser.
const DEVELOPMENT_PREFIXES: &[&str] = &[
    "--ub-checks=",
    "--frame-opt=",
    "--isel-opt=",
    "--regalloc-opt=",
    "--eq-",
    "--proof-",
    "--core-entry=",
    "--error-format=",
    "--color=",
    "--opt-pass=",
    "--pass-report=",
];

impl Command {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut command = Self {
            action: Action::Link,
            output: None,
            inputs: Vec::new(),
            link: Vec::new(),
            compile: Vec::new(),
            preprocessor: Vec::new(),
            assembler_flags: Vec::new(),
            target: None,
            sysroot: None,
            deps: Request::default(),
            verbose: false,
            quiet: false,
            debug: false,
            assembler: None,
            linker: None,
            query: None,
            ignored: Vec::new(),
        };
        let mut language = None;
        let mut index = 0;
        let mut only_inputs = false;
        while index < args.len() {
            let arg = args[index].as_str();
            // Apple's cc ignores an empty argument, and makefiles pass one.
            if arg.is_empty() {
                index += 1;
                continue;
            }
            if only_inputs || arg == "-" || !arg.starts_with('-') {
                command.input(arg, language);
                index += 1;
                continue;
            }
            command.option(args, &mut index, &mut language, &mut only_inputs)?;
            index += 1;
        }
        Ok(command)
    }

    fn input(&mut self, path: &str, language: Option<Kind>) {
        self.link.push(LinkItem::Input(self.inputs.len()));
        self.inputs.push(Input {
            path: path.to_owned(),
            language,
        });
    }

    fn act(&mut self, action: Action) {
        self.action = self.action.max(action);
    }

    fn option(
        &mut self,
        args: &[String],
        index: &mut usize,
        language: &mut Option<Kind>,
        only_inputs: &mut bool,
    ) -> Result<(), String> {
        let arg = args[*index].as_str();
        match arg {
            "--" => *only_inputs = true,
            "-c" => self.act(Action::Object),
            "-S" => self.act(Action::Assembly),
            "-E" => self.act(Action::Preprocess),
            "-fsyntax-only" => self.act(Action::SyntaxOnly),
            "-o" => self.output = Some(take(args, index, arg)?.to_owned()),
            "-x" => *language = self::language(take(args, index, arg)?)?,
            "-v" => self.verbose = true,
            "-w" => {
                self.quiet = true;
                self.compile.push(arg.to_owned());
            }
            "--help" => self.query = Some(Query::Help),
            "-dumpmachine" => self.query = Some(Query::Machine),
            "-dumpversion" | "-dumpfullversion" => self.query = Some(Query::Version),
            "-pipe" | "-pedantic" | "-pedantic-errors" | "-m64" | "-mlittle-endian" => {}
            "-M" => {
                self.deps.only = Some(Headers::All);
                self.act(Action::Preprocess);
            }
            "-MM" => {
                self.deps.only = Some(Headers::User);
                self.act(Action::Preprocess);
            }
            "-MD" => self.deps.beside = Some(Headers::All),
            "-MMD" => self.deps.beside = Some(Headers::User),
            "-MP" => self.deps.phony = true,
            "-MG" => self.ignored.push(arg.to_owned()),
            "-MF" => self.deps.file = Some(take(args, index, arg)?.to_owned()),
            "-MT" | "-MQ" => {
                let target = take(args, index, arg)?.to_owned();
                self.deps.targets.push((target, arg == "-MQ"));
            }
            "--target" | "-target" => self.set_target(take(args, index, arg)?)?,
            "--sysroot" | "-isysroot" => self.sysroot = Some(take(args, index, arg)?.to_owned()),
            "-arch" => {
                let value = take(args, index, arg)?;
                if !matches!(value, "arm64" | "aarch64") {
                    return Err(format!(
                        "unsupported architecture `{value}`; EvoCC targets arm64"
                    ));
                }
            }
            "-pthread" => {
                self.compile.push(arg.to_owned());
                self.link.push(LinkItem::Argument(arg.to_owned()));
            }
            "-l" | "-L" => {
                let value = take(args, index, arg)?;
                self.link.push(LinkItem::Argument(format!("{arg}{value}")));
            }
            "-Xlinker" => {
                let value = take(args, index, arg)?.to_owned();
                self.link.push(LinkItem::Argument(arg.to_owned()));
                self.link.push(LinkItem::Argument(value));
            }
            "-Xassembler" => {
                let value = take(args, index, arg)?.to_owned();
                self.assembler_flags.push(arg.to_owned());
                self.assembler_flags.push(value);
            }
            "--param" => {
                let value = take(args, index, arg)?;
                self.ignored.push(format!("--param {value}"));
            }
            "-iquote" | "-idirafter" => {
                let value = take(args, index, arg)?.to_owned();
                let spelling = if arg == "-iquote" { "-I" } else { "-isystem" };
                self.compile.push(spelling.to_owned());
                self.compile.push(value.clone());
                self.preprocessor.push(spelling.to_owned());
                self.preprocessor.push(value);
            }
            "--pass-report" => {
                let value = take(args, index, arg)?.to_owned();
                self.compile.push(arg.to_owned());
                self.compile.push(value);
            }
            _ if COMPILE_OPTIONS.contains(&arg) => {
                let value = take(args, index, arg)?.to_owned();
                self.compile.push(arg.to_owned());
                self.compile.push(value.clone());
                self.preprocessor.push(arg.to_owned());
                self.preprocessor.push(value);
            }
            _ if LINK_FLAGS.contains(&arg) => self.link.push(LinkItem::Argument(arg.to_owned())),
            _ if LINK_OPTIONS.contains(&arg) => {
                let value = take(args, index, arg)?.to_owned();
                self.link.push(LinkItem::Argument(arg.to_owned()));
                self.link.push(LinkItem::Argument(value));
            }
            _ if arg.starts_with("--target=") => self.set_target(&arg["--target=".len()..])?,
            _ if arg.starts_with("--sysroot=") => {
                self.sysroot = Some(arg["--sysroot=".len()..].to_owned());
            }
            _ if arg.starts_with("--assembler=") => {
                self.assembler = Some(arg["--assembler=".len()..].to_owned());
            }
            _ if arg.starts_with("--linker=") => {
                self.linker = Some(arg["--linker=".len()..].to_owned());
            }
            _ if arg.starts_with("-o") => self.output = Some(arg[2..].to_owned()),
            _ if arg.starts_with("-x") => *language = self::language(&arg[2..])?,
            _ if arg.starts_with("-MF") => self.deps.file = Some(arg[3..].to_owned()),
            _ if arg.starts_with("-MT") || arg.starts_with("-MQ") => {
                self.deps
                    .targets
                    .push((arg[3..].to_owned(), arg.starts_with("-MQ")));
            }
            _ if arg.starts_with("-l") || arg.starts_with("-L") => {
                self.link.push(LinkItem::Argument(arg.to_owned()));
            }
            _ if arg.starts_with("-Wl,") => self.link.push(LinkItem::Argument(arg.to_owned())),
            _ if arg.starts_with("-Wa,") => self.assembler_flags.push(arg.to_owned()),
            _ if arg.starts_with("-Wp,") => self.preprocessor_options(&arg[4..])?,
            _ if arg.starts_with("-W") || arg.starts_with("--cap-lints=") => {
                self.compile.push(arg.to_owned())
            }
            _ if arg.starts_with("-I") || arg.starts_with("-D") || arg.starts_with("-U") => {
                self.compile.push(arg.to_owned());
                self.preprocessor.push(arg.to_owned());
            }
            _ if arg.starts_with("-isystem") => {
                self.compile.push(arg.to_owned());
                self.preprocessor.push(arg.to_owned());
            }
            "-g0" => {
                self.debug = false;
            }
            // Every other -g form asks for debug information.
            _ if arg.starts_with("-g") => self.debug = true,
            "-Oz" => self.compile.push("-Os".to_owned()),
            _ if DEVELOPMENT_PREFIXES
                .iter()
                .any(|prefix| arg.starts_with(prefix))
                || arg == "-Oeq-scalar"
                || arg == "--survey" =>
            {
                self.compile.push(arg.to_owned());
            }
            // The option parser decides on the rest. An unknown -f or -m
            // option becomes a warning there; anything else is an error.
            _ => self.compile.push(arg.to_owned()),
        }
        Ok(())
    }

    /// `-Wp,A,B` passes A and B to the preprocessor. `-MD FILE` and
    /// `-MMD FILE` there take the dependency file as their value.
    fn preprocessor_options(&mut self, list: &str) -> Result<(), String> {
        let pieces: Vec<String> = list.split(',').map(str::to_owned).collect();
        let mut index = 0;
        while index < pieces.len() {
            match pieces[index].as_str() {
                "-MD" | "-MMD" => {
                    self.deps.beside = Some(if pieces[index] == "-MD" {
                        Headers::All
                    } else {
                        Headers::User
                    });
                    index += 1;
                    let file = pieces
                        .get(index)
                        .ok_or_else(|| format!("-Wp,{} requires a file", pieces[index - 1]))?;
                    self.deps.file = Some(file.clone());
                }
                _ => {
                    let mut language = None;
                    let mut only_inputs = false;
                    self.option(&pieces, &mut index, &mut language, &mut only_inputs)?;
                }
            }
            index += 1;
        }
        Ok(())
    }

    fn set_target(&mut self, value: &str) -> Result<(), String> {
        let target = crate::parse_target(value)?;
        if self.target.replace(target).is_some() {
            return Err("--target may be specified only once".into());
        }
        Ok(())
    }
}

/// One classified input.
struct Source {
    path: String,
    kind: Kind,
}

/// The result of one in-process compilation.
struct Compiled {
    output: Vec<u8>,
    opened: Vec<String>,
    system: Vec<String>,
}

struct Session {
    command: Command,
    target: DriverTarget,
    /// Header discovery does not select a linker sysroot. Bundled headers
    /// contain no libraries, startup objects or dynamic loader.
    header_root: Option<String>,
    format: diagnostics::Format,
    scratch: Scratch,
    tools: Option<Tools>,
}

/// Run one cc command line.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let format = diagnostics::Format::for_usage(args);
    let command = match Command::parse(args) {
        Ok(command) => command,
        Err(message) => {
            format.usage(&message);
            return ExitCode::from(1);
        }
    };
    let target = command.target.unwrap_or_else(DriverTarget::host);
    match command.query {
        Some(Query::Machine) => {
            println!("{}", target.triple());
            return ExitCode::SUCCESS;
        }
        Some(Query::Version) => {
            println!("{VERSION}");
            return ExitCode::SUCCESS;
        }
        Some(Query::Help) => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        None => {}
    }
    if command.verbose {
        eprintln!("{}\nTarget: {}", version_line(), target.triple());
    }
    if command.inputs.is_empty() {
        if command.verbose {
            return ExitCode::SUCCESS;
        }
        format.usage("no input files");
        return ExitCode::from(1);
    }
    let mut session = Session {
        header_root: command.sysroot.clone(),
        command,
        target,
        format,
        scratch: Scratch::new(),
        tools: None,
    };
    match session.execute() {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

impl Session {
    fn fail(&self, message: &str) -> ExitCode {
        self.format.usage(message);
        ExitCode::from(1)
    }

    fn warn(&self, message: &str) {
        if !self.command.quiet {
            if self.format == crate::diagnostics::Format::Short {
                eprintln!("evocc: warning: {message}");
                return;
            }
            let mut diagnostic = crate::diagnostics::Diagnostic::command("", message);
            diagnostic.level = "warning";
            eprint!(
                "{}",
                crate::diagnostics::render(
                    &diagnostic,
                    self.format,
                    crate::diagnostics::command_color()
                )
            );
        }
    }

    /// The options of one compilation in the spelling of the option parser.
    fn compile_arguments(&self, mode: &str, input: &str) -> Vec<String> {
        let mut arguments = self.command.compile.clone();
        // Debug information exists for ELF objects only.
        if self.command.debug && !self.target.is_darwin() {
            arguments.push("-g".to_owned());
        }
        if let Some(root) = &self.header_root {
            arguments.push(format!("--sysroot={root}"));
        }
        arguments.push(format!("--target={}", self.target.triple()));
        arguments.push(mode.to_owned());
        arguments.push(input.to_owned());
        arguments
    }

    fn classify(&self) -> Result<Vec<Source>, ExitCode> {
        let mut sources = Vec::new();
        for input in &self.command.inputs {
            let stdin = input.path == "-";
            let named = kind_of(&input.path);
            let kind = match (input.language, named) {
                (Some(kind), _) => kind,
                (None, Some(Kind::Linker)) => Kind::Linker,
                (None, _) if self.command.action == Action::Preprocess => Kind::C,
                (None, Some(kind)) => kind,
                (None, None) if stdin => {
                    return Err(self.fail(
                        "-E or -x is required when the input is standard input (`-`)",
                    ))
                }
                (None, None) => {
                    return Err(self.fail(&format!(
                        "unknown file type of input `{}`; name a .c, .s, .S, .o or .a file, or select the language with -x",
                        input.path
                    )))
                }
            };
            if stdin && kind != Kind::C {
                return Err(self.fail("standard input (`-`) can only be C"));
            }
            let path = if stdin {
                "/dev/stdin".to_owned()
            } else {
                input.path.clone()
            };
            sources.push(Source { path, kind });
        }
        Ok(sources)
    }

    fn execute(&mut self) -> Result<(), ExitCode> {
        let sources = self.classify()?;
        let action = self.command.action;
        let per_file = sources
            .iter()
            .filter(|source| match action {
                Action::Link => false,
                Action::Object => source.kind != Kind::Linker,
                Action::SyntaxOnly => false,
                Action::Assembly | Action::Preprocess => source.kind == Kind::C,
            })
            .count();
        if per_file > 1
            && self
                .command
                .output
                .as_deref()
                .is_some_and(|output| output != "-")
        {
            return Err(self.fail("cannot specify -o with -c, -S or -E with multiple files"));
        }
        self.validate(&sources)?;

        if !cfg!(target_os = "linux")
            && self.target == DriverTarget::Aarch64UnknownLinuxGnu
            && action == Action::Link
            && self.command.linker.is_none()
        {
            return Err(self.fail(
                "Linux linking on a non-Linux host requires --linker=COMMAND; use -c to produce objects",
            ));
        }
        if self.target == DriverTarget::Aarch64UnknownLinuxGnu
            && !self.command.compile.iter().any(|arg| arg == "-nostdinc")
            && sources
                .iter()
                .any(|source| matches!(source.kind, Kind::C | Kind::AssemblyWithCpp))
        {
            self.header_root = Some(
                toolchain::linux_sysroot(self.command.sysroot.as_deref())
                    .map_err(|message| self.fail(&message))?,
            );
        }
        if self.command.verbose {
            if let Some(root) = &self.header_root {
                eprintln!("Sysroot: {root}");
            }
        }

        let mut objects: Vec<Option<String>> = vec![None; sources.len()];
        let mut failed = false;
        for (position, source) in sources.iter().enumerate() {
            let result = match (action, source.kind) {
                (_, Kind::Linker) => {
                    if action != Action::Link {
                        self.warn(&format!(
                            "linker input `{}` is not used because linking is not done",
                            source.path
                        ));
                    }
                    Ok(Some(source.path.clone()))
                }
                (Action::Preprocess, _) => self.preprocess(source).map(|()| None),
                (Action::SyntaxOnly, Kind::C) => self.check(source).map(|()| None),
                (Action::SyntaxOnly, _) => Ok(None),
                (Action::Assembly, Kind::C) => self.assembly(source).map(|()| None),
                (Action::Assembly, _) => {
                    self.warn(&format!(
                        "input `{}` is not used because -S stops before assembly",
                        source.path
                    ));
                    Ok(None)
                }
                (Action::Object | Action::Link, kind) => self.object(source, kind).map(Some),
            };
            match result {
                Ok(object) => objects[position] = object,
                Err(code) => {
                    failed = true;
                    if code != ExitCode::from(1) {
                        return Err(code);
                    }
                }
            }
        }
        if failed {
            return Err(ExitCode::from(1));
        }
        if action == Action::Link {
            self.link(&objects)?;
        }
        Ok(())
    }

    /// Check the compilation options once, before any work, and print one
    /// warning for each ignored option.
    fn validate(&self, sources: &[Source]) -> Result<(), ExitCode> {
        let input = sources
            .iter()
            .find(|source| source.kind == Kind::C)
            .map_or("/dev/null", |source| source.path.as_str());
        let options = crate::parse_options(&self.compile_arguments(self.mode(), input), true)
            .map_err(|message| self.fail(&message))?;
        for option in options.ignored_options.iter().chain(&self.command.ignored) {
            self.warn(&format!(
                "option `{option}` is not supported and has no effect"
            ));
        }
        Ok(())
    }

    fn mode(&self) -> &'static str {
        match self.command.action {
            Action::Preprocess => "-E",
            Action::SyntaxOnly => "--parse-only",
            Action::Assembly | Action::Object | Action::Link => "--emit=asm",
        }
    }

    /// Compile one C input in this process with the option parser of
    /// `main.rs`. The diagnostics are already on standard error on failure.
    fn compile(&self, input: &str, mode: &str) -> Result<Compiled, ExitCode> {
        let mut options = crate::parse_options(&self.compile_arguments(mode, input), true)
            .map_err(|message| self.fail(&message))?;
        options.cc_mode = true;
        options.captured = Some(RefCell::new(Vec::new()));
        let status = crate::run_options(&options);
        if status != ExitCode::SUCCESS {
            return Err(status);
        }
        Ok(Compiled {
            output: options
                .captured
                .take()
                .map(RefCell::into_inner)
                .unwrap_or_default(),
            opened: options.opened_files.take(),
            system: toolchain::system_header_directories(&options),
        })
    }

    /// Write `bytes` to `path`, or to standard output for `-`.
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), ExitCode> {
        let result = if path == "-" {
            std::io::stdout().lock().write_all(bytes)
        } else {
            std::fs::write(path, bytes)
        };
        result.map_err(|error| self.fail(&format!("cannot write `{path}`: {error}")))
    }

    /// The target of a dependency rule when `-MT` and `-MQ` are absent.
    fn rule_target(&self, source: &Source) -> String {
        match (&self.command.output, self.command.action) {
            (Some(output), Action::Object | Action::Assembly)
                if self.command.deps.only.is_none() =>
            {
                output.clone()
            }
            _ => format!("{}.o", stem(&self.command_name(source))),
        }
    }

    /// The input as the command line spells it: `-` for standard input.
    fn command_name(&self, source: &Source) -> String {
        if source.path == "/dev/stdin" && self.command.inputs.iter().any(|input| input.path == "-")
        {
            "-".to_owned()
        } else {
            source.path.clone()
        }
    }

    /// Write the `-MD` or `-MMD` rule of one compilation.
    fn write_beside_rule(&self, source: &Source, compiled: &Compiled) -> Result<(), ExitCode> {
        if self.command.deps.beside.is_none() || self.command.deps.only.is_some() {
            return Ok(());
        }
        let output = match self.command.action {
            Action::Object | Action::Assembly => self.command.output.as_deref(),
            _ => None,
        };
        let path = self
            .command
            .deps
            .file
            .clone()
            .unwrap_or_else(|| deps::beside_path(output, &self.command_name(source)));
        let rule = deps::rule(
            &self.command.deps,
            &self.rule_target(source),
            &self.command_name(source),
            &compiled.opened,
            &compiled.system,
        );
        self.write(&path, rule.as_bytes())
    }

    fn preprocess(&self, source: &Source) -> Result<(), ExitCode> {
        let compiled = self.compile(&source.path, "-E")?;
        if self.command.deps.only.is_some() {
            let rule = deps::rule(
                &self.command.deps,
                &self.rule_target(source),
                &self.command_name(source),
                &compiled.opened,
                &compiled.system,
            );
            let path = self
                .command
                .deps
                .file
                .clone()
                .or_else(|| self.command.output.clone())
                .unwrap_or_else(|| "-".to_owned());
            return self.write(&path, rule.as_bytes());
        }
        let path = self
            .command
            .output
            .clone()
            .unwrap_or_else(|| "-".to_owned());
        self.write(&path, &compiled.output)?;
        self.write_beside_rule(source, &compiled)
    }

    fn check(&self, source: &Source) -> Result<(), ExitCode> {
        let compiled = self.compile(&source.path, "--parse-only")?;
        self.write_beside_rule(source, &compiled)
    }

    fn assembly(&self, source: &Source) -> Result<(), ExitCode> {
        let compiled = self.compile(&source.path, "--emit=asm")?;
        let path = self
            .command
            .output
            .clone()
            .unwrap_or_else(|| format!("{}.s", stem(&self.command_name(source))));
        self.write(&path, &compiled.output)?;
        self.write_beside_rule(source, &compiled)
    }

    /// Make the object of one C or assembly input. The `-c` action writes it
    /// to its output name; linking uses a temporary file.
    fn object(&mut self, source: &Source, kind: Kind) -> Result<String, ExitCode> {
        let name = self.command_name(source);
        let output = match (self.command.action, &self.command.output) {
            (Action::Object, Some(output)) => PathBuf::from(output),
            (Action::Object, None) => PathBuf::from(format!("{}.o", stem(&name))),
            _ => self
                .scratch
                .file(&name, "o")
                .map_err(|message| self.fail(&message))?,
        };
        let (assembly, cpp) = match kind {
            Kind::C => {
                let compiled = self.compile(&source.path, "--emit=asm")?;
                let assembly = self
                    .scratch
                    .file(&name, "s")
                    .map_err(|message| self.fail(&message))?;
                self.write(&assembly.to_string_lossy(), &compiled.output)?;
                self.write_beside_rule(source, &compiled)?;
                (assembly, false)
            }
            Kind::Assembly => (PathBuf::from(&source.path), false),
            Kind::AssemblyWithCpp => (PathBuf::from(&source.path), true),
            Kind::Linker => unreachable!("linker inputs have no object step"),
        };
        self.assemble(&assembly, &output, cpp)?;
        Ok(output.to_string_lossy().into_owned())
    }

    fn tools(&mut self) -> Result<&Tools, ExitCode> {
        if self.tools.is_none() {
            let tools = toolchain::select(
                self.target,
                self.command.assembler.as_deref(),
                self.command.linker.as_deref(),
            )
            .map_err(|message| self.fail(&message))?;
            self.tools = Some(tools);
        }
        Ok(self.tools.as_ref().expect("the tools are selected"))
    }

    fn assemble(&mut self, input: &Path, output: &Path, cpp: bool) -> Result<(), ExitCode> {
        let scratch = self
            .scratch
            .directory()
            .map_err(|message| self.fail(&message))?;
        let preprocessor = self.command.preprocessor.clone();
        let flags = self.command.assembler_flags.clone();
        let verbose = self.command.verbose;
        let tool = self.tools()?.assembler.clone();
        let absolute = |path: &Path| std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        let input = absolute(input);
        // A tool in the container writes into the mounted scratch directory.
        let produced = if tool.sealed {
            scratch.join(format!("sealed-{}.o", stem(&input.to_string_lossy())))
        } else {
            output.to_owned()
        };
        let mut arguments = Vec::new();
        match tool.style {
            Style::Driver => {
                arguments.push("-c".to_owned());
                arguments.push("-x".to_owned());
                arguments.push(
                    if cpp {
                        "assembler-with-cpp"
                    } else {
                        "assembler"
                    }
                    .to_owned(),
                );
                if cpp {
                    arguments.extend(preprocessor);
                    if self.target == DriverTarget::Aarch64UnknownLinuxGnu {
                        if let Some(root) = &self.header_root {
                            arguments.push(format!("--sysroot={root}"));
                        }
                    }
                }
                arguments.extend(flags);
            }
            Style::Assembler => {
                if cpp {
                    return Err(self.fail(
                        "a .S input needs a C compiler driver as the assembler; set --assembler=cc",
                    ));
                }
                for flag in flags {
                    match flag.strip_prefix("-Wa,") {
                        Some(list) => arguments.extend(list.split(',').map(str::to_owned)),
                        None if flag != "-Xassembler" => arguments.push(flag),
                        None => {}
                    }
                }
            }
        }
        arguments.push(input.to_string_lossy().into_owned());
        arguments.push("-o".to_owned());
        arguments.push(produced.to_string_lossy().into_owned());
        let mut mounts = vec![scratch];
        if let Ok(directory) = std::env::current_dir() {
            mounts.push(directory);
        }
        if let Some(parent) = input.parent() {
            mounts.push(parent.to_owned());
        }
        if cpp && tool.sealed {
            if let Some(root) = &self.header_root {
                mounts.push(PathBuf::from(root));
            }
        }
        toolchain::run(&tool, &arguments, &mounts, verbose)?;
        if tool.sealed {
            std::fs::copy(&produced, output).map_err(|error| {
                self.fail(&format!("cannot write `{}`: {error}", output.display()))
            })?;
        }
        Ok(())
    }

    fn link(&mut self, objects: &[Option<String>]) -> Result<(), ExitCode> {
        let output = self
            .command
            .output
            .clone()
            .unwrap_or_else(|| "a.out".to_owned());
        let mut arguments = vec!["-o".to_owned(), output];
        for item in &self.command.link {
            match item {
                LinkItem::Input(index) => {
                    if let Some(object) = &objects[*index] {
                        arguments.push(object.clone());
                    }
                }
                LinkItem::Argument(argument) => arguments.push(argument.clone()),
            }
        }
        let verbose = self.command.verbose;
        let sysroot = self.command.sysroot.clone();
        let target = self.target;
        let tool = self.tools()?.linker.clone();
        if let Some(root) = sysroot {
            if target.is_darwin() {
                arguments.extend(["-isysroot".to_owned(), root]);
            } else if !tool.sealed {
                arguments.push(format!("--sysroot={root}"));
            }
        }
        let mut mounts = Vec::new();
        if let Ok(directory) = std::env::current_dir() {
            mounts.push(directory);
        }
        if objects
            .iter()
            .flatten()
            .any(|object| Path::new(object).is_absolute())
        {
            if let Ok(scratch) = self.scratch.directory() {
                mounts.push(scratch);
            }
        }
        toolchain::run(&tool, &arguments, &mounts, verbose)
    }
}
