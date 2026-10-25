//! Five warning switches, with specific settings before group defaults.
use evocc_chir::lints::NAMES;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Allow,
    Warn,
    Deny,
}

pub struct Levels {
    specific: [Option<Level>; 10],
    no_error: [bool; 10],
    all: bool,
    pedantic: bool,
    errors: bool,
    quiet: bool,
    system_headers: bool,
    cap: Level,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            specific: [None; 10],
            no_error: [false; 10],
            all: false,
            pedantic: false,
            errors: false,
            quiet: false,
            system_headers: false,
            cap: Level::Deny,
        }
    }
}

impl Levels {
    pub fn system_header_warnings(&self) -> bool {
        self.system_headers
    }
    /// Driver warnings have no source-level lint name. They still obey the
    /// global warning policy used by compiler capability probes.
    pub fn driver_warning(&self) -> Level {
        if self.quiet {
            Level::Allow
        } else {
            (if self.errors {
                Level::Deny
            } else {
                Level::Warn
            })
            .min(self.cap)
        }
    }

    pub fn option(&mut self, flag: &str) -> Result<(), String> {
        match flag {
            "-Wall" | "-Wextra" => self.all = true,
            "-Wpedantic" => self.pedantic = true,
            "-Wno-pedantic" => self.pedantic = false,
            "-Werror" => self.errors = true,
            "-Wno-error" => self.errors = false,
            "-w" => self.quiet = true,
            "-Wsystem-headers" => self.system_headers = true,
            "-Wno-system-headers" => self.system_headers = false,
            _ if flag.starts_with("--cap-lints=") => {
                self.cap = match &flag[12..] {
                    "allow" => Level::Allow,
                    "warn" => Level::Warn,
                    "deny" => Level::Deny,
                    _ => return Err("unsupported lint cap; expected allow, warn or deny".into()),
                }
            }
            _ if flag.starts_with("-Wno-error=") => {
                let name = &flag["-Wno-error=".len()..];
                if let Some(index) = NAMES.iter().position(|&item| item == name) {
                    self.no_error[index] = true;
                }
            }
            _ => {
                let (name, level) = if let Some(name) = flag.strip_prefix("-Werror=") {
                    (name, Level::Deny)
                } else if let Some(name) = flag.strip_prefix("-Wno-") {
                    (name, Level::Allow)
                } else {
                    (flag.strip_prefix("-W").unwrap_or(flag), Level::Warn)
                };
                if let Some(index) = NAMES.iter().position(|&item| item == name) {
                    self.specific[index] = Some(level);
                    if level == Level::Deny {
                        self.no_error[index] = false;
                    }
                }
                // Other GCC warning options retain the driver's existing acceptance.
            }
        }
        Ok(())
    }

    pub fn level(&self, lint: usize, legacy: bool) -> Level {
        let default = if lint == 2 && !legacy {
            Level::Deny
        } else if lint == 5 {
            if self.all || self.pedantic {
                Level::Warn
            } else {
                Level::Allow
            }
        } else if lint == 6 {
            if self.pedantic {
                Level::Warn
            } else {
                Level::Allow
            }
        } else if lint >= 2 || self.all {
            Level::Warn
        } else {
            Level::Allow
        };
        let mut level = self.specific[lint].unwrap_or(default);
        if level == Level::Warn && self.errors && !self.no_error[lint] {
            level = Level::Deny;
        }
        if self.quiet {
            Level::Allow
        } else {
            level.min(self.cap)
        }
    }
}
