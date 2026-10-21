//! The platform tools around the compiler: the system header directories of
//! each target, the assembler, the linker, and the runs of those tools.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::OnceLock;

use crate::{DriverOptions, DriverTarget, SearchPath};

/// The image whose GCC assembles and links for the Linux target when the host
/// is not Linux. `tools/ladder/subject.py` assembles with the same image.
const SEALED_IMAGE: &str = "evo-nexlab:sealed";

/// The multiarch directory of AArch64 Debian and Ubuntu systems.
const LINUX_MULTIARCH: &str = "aarch64-linux-gnu";

/// Select one header root. A named root must work; never hide a bad override
/// by combining it with headers from another installation. Native Linux keeps
/// its system root when no explicit, packaged or source-checkout root exists.
pub(crate) fn linux_sysroot(explicit: Option<&str>) -> Result<String, String> {
    let named = explicit
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("EVOCC_SYSROOT").map(PathBuf::from));
    let valid = |root: &Path| root.join("usr/include/stdio.h").is_file();
    let selected = if let Some(root) = named {
        valid(&root).then_some(root)
    } else {
        let executable = std::env::current_exe().ok();
        let bundled = executable.as_deref().and_then(|exe| {
            Some(
                exe.parent()?
                    .parent()?
                    .join("sysroot/aarch64-unknown-linux-gnu"),
            )
        });
        let source = || {
            let root = executable
                .as_deref()?
                .parent()?
                .ancestors()
                .find(|root| root.join("Cargo.toml").is_file() && root.join("lean").is_dir())?;
            let root = root.join("sysroot/aarch64-unknown-linux-gnu");
            valid(&root).then_some(root)
        };
        bundled
            .filter(|root| valid(root))
            .or_else(source)
            .or_else(|| {
                if cfg!(target_os = "linux") {
                    return valid(Path::new("/")).then(|| PathBuf::from("/"));
                }
                if !cfg!(target_os = "macos") || !on_path("container") {
                    return None;
                }
                // Reuse a previous sealed-image export. This lookup never starts
                // a container, exports files or downloads headers.
                std::env::var_os("EVOCC_BENCH_SYSROOT")
                    .map(PathBuf::from)
                    .filter(|root| valid(root))
                    .or_else(|| {
                        let cwd = std::env::current_dir().ok()?;
                        cwd.ancestors().find_map(|parent| {
                            ["sysroot/aarch64-unknown-linux-gnu", ".build/ladder/sysroot"]
                                .into_iter()
                                .map(|name| parent.join(name))
                                .find(|root| valid(root))
                        })
                    })
            })
    };
    selected
        .and_then(|root| root.canonicalize().ok())
        .and_then(|root| root.to_str().map(str::to_owned))
        .ok_or_else(|| {
            "Linux sysroot is missing or invalid; use --sysroot=DIR, set EVOCC_SYSROOT, or install sysroot/aarch64-unknown-linux-gnu beside the bin directory".to_owned()
        })
}

/// The header directories of a target. Apple clang searches the user ones
/// after the command line's -I and -F directories and before its -isystem
/// and -iframework directories; the system ones follow every directory that
/// the command line names.
pub(crate) struct TargetDirectories {
    pub user: Vec<SearchPath>,
    pub system: Vec<SearchPath>,
}

/// The header directories of the target, in search order, or `None` when
/// the preprocessor keeps its built-in directories.
///
/// Darwin uses the clang resource headers, the macOS SDK and its framework
/// directories. Linux uses the directories of `--sysroot`. A cc compilation
/// for Linux on a Linux host uses the host's directories. The development
/// actions without `--sysroot` keep the built-in directories, so their output
/// does not change.
pub(crate) fn target_system_directories(options: &DriverOptions) -> Option<TargetDirectories> {
    if !options.standard_includes {
        return None;
    }
    let linux = |root: &Path| TargetDirectories {
        user: Vec::new(),
        system: linux_directories(root)
            .into_iter()
            .map(SearchPath::normal)
            .collect(),
    };
    match options.target {
        DriverTarget::Aarch64AppleDarwin => {
            Some(darwin_directories(options.system_root.as_deref()))
        }
        DriverTarget::Aarch64UnknownLinuxGnu => match &options.system_root {
            Some(root) => Some(linux(Path::new(root))),
            None if options.cc_mode && cfg!(target_os = "linux") => Some(linux(Path::new("/"))),
            None => None,
        },
    }
}

/// Every directory whose headers are system headers for `-MM` and `-MMD`.
pub(crate) fn system_header_directories(options: &DriverOptions) -> Vec<String> {
    let mut directories: Vec<String> = options
        .system_include_paths
        .iter()
        .map(|directory| directory.path.clone())
        .collect();
    match target_system_directories(options) {
        Some(target) => directories.extend(
            target
                .user
                .into_iter()
                .chain(target.system)
                .map(|directory| directory.path),
        ),
        None if options.standard_includes => {
            directories.extend(["/usr/include".to_owned(), "/usr/local/include".to_owned()]);
        }
        None => {}
    }
    directories
}

fn existing(paths: impl IntoIterator<Item = PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .filter(|path| path.is_dir())
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

/// GCC's own headers, the local headers, the multiarch headers, then the C
/// library headers, as GCC on AArch64 Debian searches them.
fn linux_directories(root: &Path) -> Vec<String> {
    let compiler = root.join("usr/lib/gcc").join(LINUX_MULTIARCH);
    let mut versions: Vec<(Vec<u32>, PathBuf)> = std::fs::read_dir(&compiler)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let key = name
                .split('.')
                .map(str::parse::<u32>)
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            let include = entry.path().join("include");
            include.join("stddef.h").is_file().then_some((key, include))
        })
        .collect();
    versions.sort();
    existing(
        versions
            .pop()
            .into_iter()
            .flat_map(|(_, include)| {
                let fixed = include.with_file_name("include-fixed");
                [include, fixed]
            })
            .chain([
                root.join("usr/local/include"),
                root.join("usr/include").join(LINUX_MULTIARCH),
                root.join("usr/include"),
            ]),
    )
}

/// The directories of Apple clang 21 for arm64 macOS, in its order: the
/// local headers as a user directory when no SDK root is named, then the
/// clang resource headers, the SDK headers, the toolchain headers, and the
/// SDK's framework and subframework directories.
fn darwin_directories(root: Option<&str>) -> TargetDirectories {
    let sdk = root.map(PathBuf::from).or_else(darwin_sdk);
    let local = if root.is_none() {
        Some(PathBuf::from("/usr/local/include"))
    } else {
        None
    };
    let toolchain = darwin_resource_headers()
        .and_then(|headers| Some(headers.ancestors().nth(4)?.join("include")));
    let headers = existing(
        sdk.iter()
            .map(|sdk| sdk.join("usr/local/include"))
            .chain(darwin_resource_headers())
            .chain(sdk.iter().map(|sdk| sdk.join("usr/include")))
            .chain(toolchain),
    );
    let frameworks = existing(sdk.iter().flat_map(|sdk| {
        [
            sdk.join("System/Library/Frameworks"),
            sdk.join("System/Library/SubFrameworks"),
        ]
    }));
    TargetDirectories {
        user: existing(local)
            .into_iter()
            .map(SearchPath::normal)
            .collect(),
        system: headers
            .into_iter()
            .map(SearchPath::normal)
            .chain(frameworks.into_iter().map(SearchPath::framework))
            .collect(),
    }
}

/// The macOS SDK: `SDKROOT`, then `xcrun`, then the Command Line Tools.
fn darwin_sdk() -> Option<PathBuf> {
    static SDK: OnceLock<Option<PathBuf>> = OnceLock::new();
    SDK.get_or_init(|| {
        if let Some(root) = std::env::var_os("SDKROOT").map(PathBuf::from) {
            if root.is_dir() {
                return Some(root);
            }
        }
        query(&["xcrun", "--sdk", "macosx", "--show-sdk-path"])
            .map(PathBuf::from)
            .filter(|path| path.is_dir())
            .or_else(|| {
                Some(PathBuf::from(
                    "/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk",
                ))
                .filter(|path| path.is_dir())
            })
    })
    .clone()
}

/// The resource headers of the system C compiler, such as `stddef.h` and
/// `stdarg.h`. The macOS SDK does not ship them.
fn darwin_resource_headers() -> Option<PathBuf> {
    static HEADERS: OnceLock<Option<PathBuf>> = OnceLock::new();
    HEADERS
        .get_or_init(|| {
            query(&["cc", "-print-resource-dir"])
                .map(|directory| PathBuf::from(directory).join("include"))
                .filter(|path| path.join("stddef.h").is_file())
        })
        .clone()
}

/// The resource headers that EvoCC itself reads, in the host namespace.
/// A cross query must not return a path that exists only inside the assembler
/// container. The bundled Linux root uses the same search as C preprocessing.
pub(crate) fn resource_headers(
    target: DriverTarget,
    root: Option<&str>,
) -> Result<PathBuf, String> {
    let headers = match target {
        DriverTarget::Aarch64AppleDarwin => darwin_resource_headers(),
        DriverTarget::Aarch64UnknownLinuxGnu => {
            let root = linux_sysroot(root)?;
            linux_directories(Path::new(&root))
                .into_iter()
                .map(PathBuf::from)
                .find(|path| path.join("stddef.h").is_file())
        }
    };
    headers.ok_or_else(|| "compiler resource headers are missing".to_owned())
}

/// The first line of a command's standard output, when the command succeeds.
fn query(argv: &[&str]) -> Option<String> {
    let output = Command::new(argv[0]).args(&argv[1..]).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let line = text.lines().next()?.trim();
    (output.status.success() && !line.is_empty()).then(|| line.to_owned())
}

/// How a tool takes its arguments.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Style {
    /// A C compiler driver: `cc -c -x assembler IN -o OUT`, `cc -o OUT ...`.
    Driver,
    /// A plain assembler: `as IN -o OUT`.
    Assembler,
}

/// One tool and the place where it runs.
#[derive(Clone)]
pub(crate) struct Tool {
    pub argv: Vec<String>,
    pub style: Style,
    /// Run the tool in the sealed image, not on the host.
    pub sealed: bool,
}

impl Tool {
    fn parse(command: &str, sealed: bool) -> Result<Self, String> {
        let argv: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
        let program = argv
            .first()
            .ok_or_else(|| "the tool command is empty".to_owned())?;
        let name = Path::new(program)
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let style = if name == "as" || name.ends_with("-as") {
            Style::Assembler
        } else {
            Style::Driver
        };
        Ok(Self {
            argv,
            style,
            sealed,
        })
    }
}

/// The assembler and the linker of one target.
pub(crate) struct Tools {
    pub assembler: Tool,
    pub linker: Tool,
}

/// Select the tools. `--assembler=` and `--linker=` come first, then
/// `EVOCC_AS` and `EVOCC_CC`, then the target's default.
pub(crate) fn select(
    target: DriverTarget,
    assembler: Option<&str>,
    linker: Option<&str>,
) -> Result<Tools, String> {
    let environment = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    };
    let assembler_override = assembler
        .map(str::to_owned)
        .or_else(|| environment("EVOCC_AS"));
    let linker_override = linker
        .map(str::to_owned)
        .or_else(|| environment("EVOCC_CC"));
    let (default, sealed) = match target {
        DriverTarget::Aarch64AppleDarwin => ("cc -arch arm64", false),
        DriverTarget::Aarch64UnknownLinuxGnu if cfg!(target_os = "linux") => {
            (if on_path("gcc") { "gcc" } else { "cc" }, false)
        }
        DriverTarget::Aarch64UnknownLinuxGnu => ("gcc", true),
    };
    Ok(Tools {
        assembler: match assembler_override {
            Some(command) => Tool::parse(&command, false)?,
            None => Tool::parse(default, sealed)?,
        },
        linker: match linker_override {
            Some(command) => Tool::parse(&command, false)?,
            None => Tool::parse(default, sealed)?,
        },
    })
}

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| directory.join(program).is_file())
    })
}

/// Quote one argument for the `-v` trace.
fn quoted(argument: &str) -> String {
    if !argument.is_empty()
        && argument
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@%+=:,./-_".contains(&byte))
    {
        argument.to_owned()
    } else {
        format!("'{}'", argument.replace('\'', "'\\''"))
    }
}

/// Run `arguments` with `tool`. The tool writes its own messages. A failure
/// is exit status 1, as cc reports it.
///
/// A sealed tool runs in the container with each directory of `mounts` at
/// its host path, so that the paths in `arguments` stay valid. The first
/// mount is the working directory of the tool.
pub(crate) fn run(
    tool: &Tool,
    arguments: &[String],
    mounts: &[PathBuf],
    verbose: bool,
    stdin: bool,
) -> Result<(), ExitCode> {
    let mut argv: Vec<String> = Vec::new();
    if tool.sealed {
        argv.extend(
            [
                "container",
                "run",
                "--rm",
                "--platform",
                "linux/arm64",
                "--network",
                "none",
                "--progress",
                "none",
            ]
            .map(str::to_owned),
        );
        if stdin {
            argv.push("--interactive".to_owned());
        }
        let mut seen = Vec::new();
        for mount in mounts {
            let host = std::fs::canonicalize(mount).unwrap_or_else(|_| mount.clone());
            // Keep the guest spelling used by the command arguments. On
            // macOS, /tmp and /var can name /private/tmp and /private/var;
            // mounting only the canonical spelling loses those inputs/outputs.
            let guest = std::path::absolute(mount).unwrap_or_else(|_| mount.clone());
            if !seen.contains(&guest) {
                argv.push("-v".to_owned());
                argv.push(format!("{}:{}", host.display(), guest.display()));
                seen.push(guest);
            }
        }
        if let Some(directory) = seen.first() {
            argv.push("-w".to_owned());
            argv.push(directory.display().to_string());
        }
        argv.push(SEALED_IMAGE.to_owned());
    }
    argv.extend(tool.argv.iter().cloned());
    argv.extend(arguments.iter().cloned());
    if verbose {
        let line: Vec<String> = argv.iter().map(|argument| quoted(argument)).collect();
        eprintln!(" {}", line.join(" "));
    }
    match Command::new(&argv[0]).args(&argv[1..]).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err(ExitCode::from(1)),
        Err(error) => {
            eprintln!("evocc: cannot run `{}`: {error}", argv[0]);
            Err(ExitCode::from(1))
        }
    }
}

/// A private directory for intermediate files, removed when it is dropped.
pub(crate) struct Scratch {
    path: Option<PathBuf>,
    next: u32,
}

impl Scratch {
    pub(crate) fn new() -> Self {
        Self {
            path: None,
            next: 0,
        }
    }

    /// The directory, made on first use.
    pub(crate) fn directory(&mut self) -> Result<PathBuf, String> {
        if let Some(path) = &self.path {
            return Ok(path.clone());
        }
        let base = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        for attempt in 0..100_u32 {
            let path = base.join(format!("evocc-{}-{stamp}-{attempt}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    let path = std::fs::canonicalize(&path).unwrap_or(path);
                    self.path = Some(path.clone());
                    return Ok(path);
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!(
                        "cannot create a temporary directory in {}: {error}",
                        base.display()
                    ))
                }
            }
        }
        Err("cannot create a temporary directory".into())
    }

    /// A new file name in the directory, with the stem of `input`.
    pub(crate) fn file(&mut self, input: &str, extension: &str) -> Result<PathBuf, String> {
        let directory = self.directory()?;
        self.next += 1;
        Ok(directory.join(format!(
            "{}-{}.{extension}",
            crate::cc::stem(input),
            self.next
        )))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}
