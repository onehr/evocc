//! Warning switches, with specific settings before group defaults.
//! `W0012` has GCC 14's levels for its permissive error (EvoIR 6.3).
use evocc_chir::lints::NAMES;

/// `incompatible-pointer-types` (`W0012`).
pub const INCOMPATIBLE_POINTER_TYPES: usize = 11;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Allow,
    Warn,
    Deny,
}

pub struct Levels {
    specific: [Option<Level>; NAMES.len()],
    no_error: [bool; NAMES.len()],
    /// The last of `-Werror=NAME` (`true`) and `-Wno-error=NAME` (`false`).
    classification: [Option<bool>; NAMES.len()],
    /// The last of `-fpermissive` and `-fno-permissive`.
    permissive: bool,
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
            specific: [None; NAMES.len()],
            no_error: [false; NAMES.len()],
            classification: [None; NAMES.len()],
            permissive: false,
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

    /// `-fpermissive` (`true`) and `-fno-permissive` (`false`); the last wins.
    pub fn set_permissive(&mut self, permissive: bool) {
        self.permissive = permissive;
    }

    /// GCC 14 reports a lint in a system header only when it stays an error
    /// by its own default: `W0012` at a deny base level that no option
    /// lowered. Every other lint follows `-Wsystem-headers`.
    pub fn reports_in_system_header(&self, lint: usize, legacy: bool) -> bool {
        lint == INCOMPATIBLE_POINTER_TYPES
            && !legacy
            && !self.permissive
            && self.level(lint, legacy) == Level::Deny
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
                    self.classification[index] = Some(false);
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
                        self.classification[index] = Some(true);
                    }
                }
                // Other GCC warning options retain the driver's existing acceptance.
            }
        }
        Ok(())
    }

    pub fn level(&self, lint: usize, legacy: bool) -> Level {
        if lint == INCOMPATIBLE_POINTER_TYPES {
            return self.permissive_error_level(lint, legacy);
        }
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

    /// GCC 14's permissive error (EvoIR 6.3): deny by default in C99 and
    /// later modes, warn in C89 and GNU89 and with `-fpermissive`. The last
    /// of `-Werror=NAME` and `-Wno-error=NAME` sets the level, over the base
    /// level and over `-Werror`; `-Wno-NAME` allows it until a later
    /// `-Werror=NAME`. `-w` removes only a warning: a deny base level that no
    /// option lowered stays deny.
    fn permissive_error_level(&self, lint: usize, legacy: bool) -> Level {
        if self.specific[lint] == Some(Level::Allow) {
            return Level::Allow;
        }
        let base_warning = legacy || self.permissive;
        let level = match self.classification[lint] {
            Some(true) => Level::Deny,
            Some(false) => Level::Warn,
            None if base_warning && !self.errors => Level::Warn,
            None => Level::Deny,
        };
        if self.quiet && (base_warning || level == Level::Warn) {
            Level::Allow
        } else {
            level.min(self.cap)
        }
    }
}
