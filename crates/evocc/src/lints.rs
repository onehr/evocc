//! Five warning switches, with specific settings before group defaults.
use evocc_chir::lints::NAMES;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Allow,
    Warn,
    Deny,
}

pub struct Levels {
    specific: [Option<Level>; 5],
    all: bool,
    errors: bool,
    quiet: bool,
    cap: Level,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            specific: [None; 5],
            all: false,
            errors: false,
            quiet: false,
            cap: Level::Deny,
        }
    }
}

impl Levels {
    pub fn option(&mut self, flag: &str) -> Result<(), String> {
        match flag {
            "-Wall" | "-Wextra" => self.all = true,
            "-Werror" => self.errors = true,
            "-w" => self.quiet = true,
            _ if flag.starts_with("--cap-lints=") => {
                self.cap = match &flag[12..] {
                    "allow" => Level::Allow,
                    "warn" => Level::Warn,
                    "deny" => Level::Deny,
                    _ => return Err("unsupported lint cap; expected allow, warn or deny".into()),
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
                }
                // Other GCC warning options retain the driver's existing acceptance.
            }
        }
        Ok(())
    }

    pub fn level(&self, lint: usize, legacy: bool) -> Level {
        let default = if lint == 2 && !legacy {
            Level::Deny
        } else if lint >= 2 || self.all {
            Level::Warn
        } else {
            Level::Allow
        };
        let mut level = self.specific[lint].unwrap_or(default);
        if level == Level::Warn && self.errors {
            level = Level::Deny;
        }
        if self.quiet {
            Level::Allow
        } else {
            level.min(self.cap)
        }
    }
}
