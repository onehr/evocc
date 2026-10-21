//! Clang's `availability` attribute for a macOS deployment target (EvoIR
//! 10.1.9). Apple's SDK headers mark declarations with it through
//! `__API_AVAILABLE` and related macros. The front end refuses a use of a
//! declaration that is unavailable for the deployment target and makes a
//! declaration that is not yet introduced a weak import, as Apple clang does.
//! The preprocessor refuses a `#pragma clang attribute` that would apply such
//! an attribute.

use std::fmt;

/// A version as Clang writes it: one to three decimal components, separated
/// by `.` or by `_`. A missing component compares as zero, so `12` equals
/// `12.0.0`; the written components are kept for the diagnostic text.
#[derive(Clone, Copy, Debug)]
pub struct Version {
    components: [u32; 3],
    count: usize,
}

impl Version {
    /// The version `major.minor.patch`.
    #[must_use]
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            components: [major, minor, patch],
            count: 3,
        }
    }

    /// Read `13`, `13.0`, `10.14.4` or `10_14_4`. One separator is used
    /// throughout, and each component is a decimal number.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let separator = if text.contains('_') { '_' } else { '.' };
        let mut components = [0; 3];
        let mut count = 0;
        for part in text.split(separator) {
            if count == 3 || part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            components[count] = part.parse().ok()?;
            count += 1;
        }
        Some(Self { components, count })
    }

    fn key(self) -> [u32; 3] {
        self.components
    }

    /// The major component, which a macOS deployment target needs to be 10
    /// or more.
    #[must_use]
    pub fn major(self) -> u32 {
        self.components[0]
    }

    /// The encoding of Apple's headers, as Clang writes it: 14.2 is 140200,
    /// and a version before 10.10 has four digits, so 10.9.5 is 1095.
    #[must_use]
    pub fn encoded(self) -> u32 {
        let [major, minor, patch] = self.components;
        if (major, minor) < (10, 10) {
            major * 100 + minor.min(9) * 10 + patch.min(9)
        } else {
            major * 10_000 + minor * 100 + patch
        }
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, component) in self.components[..self.count].iter().enumerate() {
            if index > 0 {
                formatter.write_str(".")?;
            }
            write!(formatter, "{component}")?;
        }
        Ok(())
    }
}

/// One `availability` attribute: its platform and its clauses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Availability {
    /// Clang's canonical platform name: `macosx` and `macOS` are `macos`.
    pub platform: String,
    pub introduced: Option<Version>,
    pub deprecated: Option<Version>,
    pub obsoleted: Option<Version>,
    pub unavailable: bool,
    pub strict: bool,
    /// The text of the `message` string literal, without its quotes.
    pub message: Option<String>,
    /// An `environment=` clause names a platform environment that macOS
    /// does not have.
    pub environment: bool,
}

impl Availability {
    /// Read the arguments of one `availability(...)` as written. `Err(index)`
    /// names the first argument that Clang refuses.
    pub fn parse<S: AsRef<str>>(arguments: &[S]) -> Result<Self, usize> {
        let Some(platform) = arguments.first().map(|text| text.as_ref().trim()) else {
            return Err(0);
        };
        if platform.is_empty()
            || !platform
                .bytes()
                .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
            || platform.as_bytes()[0].is_ascii_digit()
        {
            return Err(0);
        }
        let mut availability = Self {
            platform: canonical_platform(platform).to_owned(),
            introduced: None,
            deprecated: None,
            obsoleted: None,
            unavailable: false,
            strict: false,
            message: None,
            environment: false,
        };
        for (index, argument) in arguments.iter().enumerate().skip(1) {
            let argument = argument.as_ref().trim();
            let (key, value) = match argument.split_once('=') {
                Some((key, value)) => (key.trim(), Some(value.trim())),
                None => (argument, None),
            };
            let version = || value.and_then(Version::parse).ok_or(index);
            let literal = || {
                value
                    .and_then(|value| value.strip_prefix('"'))
                    .and_then(|value| value.strip_suffix('"'))
                    .map(str::to_owned)
                    .ok_or(index)
            };
            match (key, value) {
                // Clang reads `introduced=NA` as unavailable and ignores
                // `deprecated=NA`.
                ("introduced", Some("NA")) => availability.unavailable = true,
                ("deprecated", Some("NA")) => {}
                ("introduced", Some(_)) => availability.introduced = Some(version()?),
                ("deprecated", Some(_)) => availability.deprecated = Some(version()?),
                ("obsoleted", Some(_)) => availability.obsoleted = Some(version()?),
                ("unavailable", None) => availability.unavailable = true,
                ("strict", None) => availability.strict = true,
                ("message", Some(_)) => availability.message = Some(literal()?),
                ("replacement", Some(_)) => {
                    literal()?;
                }
                ("environment", Some(value)) if !value.is_empty() => {
                    availability.environment = true;
                }
                _ => return Err(index),
            }
        }
        Ok(availability)
    }

    /// Whether the attribute applies to a macOS target.
    #[must_use]
    pub fn applies_to_macos(&self) -> bool {
        self.platform == "macos" && !self.environment
    }

    /// Clang ignores an attribute whose versions are out of order.
    fn ordered(&self) -> bool {
        let before = |earlier: Option<Version>, later: Option<Version>| match (earlier, later) {
            (Some(earlier), Some(later)) => earlier <= later,
            _ => true,
        };
        before(self.introduced, self.deprecated)
            && before(self.deprecated, self.obsoleted)
            && before(self.introduced, self.obsoleted)
    }

    /// Whether two attributes for one platform agree, as Apple clang checks
    /// a later attribute against an earlier one: each version is absent in
    /// one of them or equal, and `unavailable` is the same.
    fn agrees_with(&self, other: &Self) -> bool {
        let same = |left: Option<Version>, right: Option<Version>| match (left, right) {
            (Some(left), Some(right)) => left == right,
            _ => true,
        };
        same(self.introduced, other.introduced)
            && same(self.deprecated, other.deprecated)
            && same(self.obsoleted, other.obsoleted)
            && self.unavailable == other.unavailable
    }

    fn filled_from(mut self, earlier: &Self) -> Self {
        self.introduced = self.introduced.or(earlier.introduced);
        self.deprecated = self.deprecated.or(earlier.deprecated);
        self.obsoleted = self.obsoleted.or(earlier.obsoleted);
        self.strict |= earlier.strict;
        if self.message.is_none() {
            self.message.clone_from(&earlier.message);
        }
        self
    }

    /// The effect of the attribute for `deployment`, in Clang's order:
    /// `unavailable`, then a later introduction, then obsolescence, then
    /// deprecation.
    #[must_use]
    pub fn effect(&self, deployment: Version) -> Effect {
        let hint = self
            .message
            .as_deref()
            .filter(|message| !message.is_empty())
            .map_or_else(String::new, |message| format!(" - {message}"));
        if self.unavailable {
            return Effect::Unavailable(format!("not available on macOS{hint}"));
        }
        if let Some(introduced) = self
            .introduced
            .filter(|introduced| deployment < *introduced)
        {
            return if self.strict {
                Effect::Unavailable(format!("introduced in macOS {introduced}{hint}"))
            } else {
                Effect::NotYetIntroduced
            };
        }
        if let Some(obsoleted) = self.obsoleted.filter(|obsoleted| deployment >= *obsoleted) {
            return Effect::Unavailable(format!("obsoleted in macOS {obsoleted}{hint}"));
        }
        if self
            .deprecated
            .is_some_and(|deprecated| deployment >= deprecated)
        {
            return Effect::Deprecated;
        }
        Effect::Available
    }
}

/// Clang's canonical name of a platform in an `availability` attribute.
fn canonical_platform(name: &str) -> &str {
    match name {
        "macosx" | "macOS" => "macos",
        "macosx_app_extension" | "macOSApplicationExtension" => "macos_app_extension",
        "iOS" => "ios",
        "tvOS" => "tvos",
        "watchOS" => "watchos",
        "iOSApplicationExtension" => "ios_app_extension",
        "tvOSApplicationExtension" => "tvos_app_extension",
        "watchOSApplicationExtension" => "watchos_app_extension",
        "macCatalyst" => "maccatalyst",
        "macCatalystApplicationExtension" => "maccatalyst_app_extension",
        "xrOS" => "xros",
        "xrOSApplicationExtension" => "xros_app_extension",
        _ => name,
    }
}

/// What an availability attribute means for a declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Available,
    /// The declaration is introduced after the deployment target. A function
    /// or object declaration without a definition is a weak import.
    NotYetIntroduced,
    /// Apple clang warns at a use; this profile has no effect for it.
    Deprecated,
    /// A use is refused; the text is Apple clang's reason.
    Unavailable(String),
}

/// The macOS `availability` attributes of one declaration chain, merged in
/// source order as Apple clang 21 merges them. An attribute that disagrees
/// with the merged one removes both, and an attribute whose merged versions
/// are out of order is ignored with the merged one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Merged {
    current: Option<Availability>,
}

impl Merged {
    /// Merge the next attribute. Attributes of other platforms are ignored.
    pub fn merge(&mut self, next: Availability) {
        if !next.applies_to_macos() {
            return;
        }
        self.current = match self.current.take() {
            None => next.ordered().then_some(next),
            Some(current) if next.agrees_with(&current) => {
                let merged = next.filled_from(&current);
                merged.ordered().then_some(merged)
            }
            Some(_) => None,
        };
    }

    /// The effect for `deployment`.
    #[must_use]
    pub fn effect(&self, deployment: Version) -> Effect {
        self.current
            .as_ref()
            .map_or(Effect::Available, |current| current.effect(deployment))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> Availability {
        Availability::parse(arguments).unwrap()
    }

    #[test]
    fn versions_read_dots_and_underscores_and_compare_missing_parts_as_zero() {
        assert_eq!(Version::parse("12").unwrap().to_string(), "12");
        assert_eq!(Version::parse("10_14_4").unwrap().to_string(), "10.14.4");
        assert_eq!(Version::parse("12"), Version::parse("12.0.0"));
        assert!(Version::parse("11.0").unwrap() < Version::parse("11.0.1").unwrap());
        for bad in ["", "1.", ".1", "1.2.3.4", "1x", "10.14_4", "+1"] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        assert_eq!(Version::new(12, 3, 1).encoded(), 120_301);
        assert_eq!(Version::new(10, 9, 5).encoded(), 1095);
        assert_eq!(Version::new(10, 10, 0).encoded(), 101_000);
    }

    #[test]
    fn effects_follow_apple_clang_for_a_macos_11_target() {
        let target = Version::new(11, 0, 0);
        let effect = |arguments: &[&str]| parse(arguments).effect(target);
        assert_eq!(
            effect(&["macos", "unavailable"]),
            Effect::Unavailable("not available on macOS".into())
        );
        assert_eq!(
            effect(&["macosx", "introduced=10.0", "obsoleted=10.15"]),
            Effect::Unavailable("obsoleted in macOS 10.15".into())
        );
        assert_eq!(
            effect(&["macos", "introduced=13.0"]),
            Effect::NotYetIntroduced
        );
        assert_eq!(
            effect(&["macos", "strict", "introduced=13.0"]),
            Effect::Unavailable("introduced in macOS 13.0".into())
        );
        assert_eq!(
            effect(&[
                "macos",
                "introduced=10.0",
                "deprecated=10.12",
                "message=\"use other\""
            ]),
            Effect::Deprecated
        );
        assert_eq!(effect(&["macos", "introduced=11.0"]), Effect::Available);
        assert_eq!(
            effect(&["macos", "introduced=NA"]),
            Effect::Unavailable("not available on macOS".into())
        );
        assert_eq!(
            effect(&["macos", "introduced=10.0", "deprecated=NA"]),
            Effect::Available
        );
        assert_eq!(Availability::parse(&["macos", "obsoleted=NA"]), Err(1));
        assert!(!parse(&["ios", "unavailable"]).applies_to_macos());
        assert!(!parse(&["macos_app_extension", "unavailable"]).applies_to_macos());
        assert_eq!(Availability::parse(&["macos", "since=1.0"]), Err(1));
        assert_eq!(
            Availability::parse(&["macos", "introduced=1.2.3.4"]),
            Err(1)
        );
    }

    #[test]
    fn disagreeing_attributes_cancel_and_agreeing_ones_merge() {
        let target = Version::new(11, 0, 0);
        let merged = |list: &[&[&str]]| {
            let mut merged = Merged::default();
            for arguments in list {
                merged.merge(parse(arguments));
            }
            merged.effect(target)
        };
        let introduced = |version: &'static str| -> Vec<&'static str> {
            vec![
                "macos",
                Box::leak(format!("introduced={version}").into_boxed_str()),
            ]
        };
        let (thirteen, fourteen) = (introduced("13.0"), introduced("14.0"));
        assert_eq!(merged(&[&thirteen, &fourteen]), Effect::Available);
        assert_eq!(
            merged(&[&thirteen, &fourteen, &thirteen]),
            Effect::NotYetIntroduced
        );
        assert_eq!(merged(&[&thirteen, &thirteen]), Effect::NotYetIntroduced);
        assert_eq!(
            merged(&[&["macos", "unavailable"], &thirteen]),
            Effect::Available
        );
        assert_eq!(
            merged(&[&["macos", "deprecated=12.0"], &thirteen]),
            Effect::Available
        );
        assert_eq!(
            merged(&[&["macos", "introduced=13.0", "deprecated=14.0"], &thirteen]),
            Effect::NotYetIntroduced
        );
        assert_eq!(
            merged(&[&thirteen, &["ios", "introduced=20.0"]]),
            Effect::NotYetIntroduced
        );
    }
}
