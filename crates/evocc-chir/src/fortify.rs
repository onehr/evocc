//! GNU object-size checked library builtins: `__builtin___memcpy_chk` and
//! its family. The fortified C library headers of glibc and of the macOS SDK
//! call them with the size of the destination object.
//!
//! Each builtin names a C library function and its checked entry point, for
//! example `memcpy` and `__memcpy_chk`. The checked entry point stops the
//! program when the destination is too small. A call keeps the check unless
//! the size is `(size_t)-1`, which means that no size is known, or unless
//! constant arguments show that the check cannot fail. The parser applies
//! this rule to source constants, and substitution applies it again after it
//! resolves object-size queries.

/// One parameter or result type of a checked builtin prototype.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Param {
    /// `void *`
    Pointer,
    /// `const void *`
    ConstPointer,
    /// `char *`
    Chars,
    /// `const char *`
    ConstChars,
    /// `size_t`
    Size,
    /// `int`
    Int,
    /// The target `va_list`.
    VaList,
    /// `FILE *`, or an opaque `void *` before the header defines `FILE`.
    Stream,
}

/// The prototype of a checked builtin and its relation to the plain function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CheckedBuiltin {
    /// The builtin spelling, for example `__builtin___memcpy_chk`.
    pub builtin: &'static str,
    /// The C library function without the check, for example `memcpy`.
    pub plain: &'static str,
    /// The C library entry point with the check, for example `__memcpy_chk`.
    pub checked: &'static str,
    pub result: Param,
    /// The named parameters of the checked entry point.
    pub parameters: &'static [Param],
    pub variadic: bool,
    /// The destination object size, absent for stream formatting functions.
    pub size: Option<usize>,
    /// The parameter that receives the format-checking flag, if any.
    pub flag: Option<usize>,
    /// The parameter whose constant value no greater than the size proves
    /// that the check cannot fail, if any.
    pub length: Option<usize>,
}

impl CheckedBuiltin {
    /// The named parameters of the plain function.
    #[must_use]
    pub fn plain_parameters(&self) -> Vec<Param> {
        self.parameters
            .iter()
            .enumerate()
            .filter(|(index, _)| self.keeps(*index))
            .map(|(_, parameter)| *parameter)
            .collect()
    }

    /// Whether argument `index` of the checked call is also an argument of
    /// the plain call. Anonymous arguments are always kept.
    #[must_use]
    pub fn keeps(&self, index: usize) -> bool {
        Some(index) != self.size && Some(index) != self.flag
    }

    /// Decide from constant argument values whether the plain function can
    /// replace the checked entry point. `constant(index)` is the value of
    /// argument `index` after conversion to its parameter type, when that
    /// value is known.
    ///
    /// The flag of the formatting builtins asks the C library for checks
    /// beyond the size, so a nonzero or unknown flag always keeps the call.
    #[must_use]
    pub fn check_is_redundant(&self, constant: impl Fn(usize) -> Option<u64>) -> bool {
        self.check_with_source(constant, None)
    }

    /// Include the bytes through the first NUL of a literal copy source.
    /// Stream formatting has no size check to discharge: keep its library
    /// entry point and flag even when the flag happens to be zero.
    #[must_use]
    pub fn check_with_source(
        &self,
        constant: impl Fn(usize) -> Option<u64>,
        copy_bytes: Option<u64>,
    ) -> bool {
        if self.flag.is_some_and(|flag| constant(flag) != Some(0)) {
            return false;
        }
        let Some(size) = self.size.and_then(&constant) else {
            return false;
        };
        size == u64::MAX
            || self
                .length
                .and_then(&constant)
                .is_some_and(|length| length <= size)
            || (matches!(self.plain, "strcpy" | "stpcpy")
                && copy_bytes.is_some_and(|length| length <= size))
    }

    /// Only a literal copy supplies this bound. Do not use it for strcat:
    /// its destination's current length also contributes to the write.
    #[must_use]
    pub fn literal_copy_size(&self, arguments: &[&crate::CHirExpr<'_>]) -> Option<u64> {
        if !matches!(self.plain, "strcpy" | "stpcpy") {
            return None;
        }
        let crate::CHirExpr::StringLit(literal, _) = arguments.get(1)? else {
            return None;
        };
        if !matches!(
            literal.prefix,
            crate::StringPrefix::None | crate::StringPrefix::U8
        ) {
            return None;
        }
        let units = literal.decoded_units()?;
        let length = units
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(units.len());
        u64::try_from(length).ok()?.checked_add(1)
    }
}

const fn entry(
    builtin: &'static str,
    plain: &'static str,
    checked: &'static str,
    result: Param,
    parameters: &'static [Param],
    variadic: bool,
    (size, flag, length): (usize, Option<usize>, Option<usize>),
) -> CheckedBuiltin {
    CheckedBuiltin {
        builtin,
        plain,
        checked,
        result,
        parameters,
        variadic,
        size: Some(size),
        flag,
        length,
    }
}

use Param::{Chars, ConstChars, ConstPointer, Int, Pointer, Size, Stream, VaList};

const COPY: &[Param] = &[Pointer, ConstPointer, Size, Size];
const STRING: &[Param] = &[Chars, ConstChars, Size];
const BOUNDED_STRING: &[Param] = &[Chars, ConstChars, Size, Size];

const fn stream_entry(
    builtin: &'static str,
    plain: &'static str,
    checked: &'static str,
    parameters: &'static [Param],
    variadic: bool,
    flag: usize,
) -> CheckedBuiltin {
    CheckedBuiltin {
        builtin,
        plain,
        checked,
        result: Int,
        parameters,
        variadic,
        size: None,
        flag: Some(flag),
        length: None,
    }
}

/// The checked builtins selected by the glibc and macOS SDK headers.
pub const CHECKED_BUILTINS: &[CheckedBuiltin] = &[
    entry(
        "__builtin___memccpy_chk",
        "memccpy",
        "__memccpy_chk",
        Pointer,
        &[Pointer, ConstPointer, Int, Size, Size],
        false,
        (4, None, Some(3)),
    ),
    entry(
        "__builtin___strlcpy_chk",
        "strlcpy",
        "__strlcpy_chk",
        Size,
        BOUNDED_STRING,
        false,
        (3, None, Some(2)),
    ),
    entry(
        "__builtin___strlcat_chk",
        "strlcat",
        "__strlcat_chk",
        Size,
        BOUNDED_STRING,
        false,
        (3, None, Some(2)),
    ),
    stream_entry(
        "__builtin___printf_chk",
        "printf",
        "__printf_chk",
        &[Int, ConstChars],
        true,
        0,
    ),
    stream_entry(
        "__builtin___vprintf_chk",
        "vprintf",
        "__vprintf_chk",
        &[Int, ConstChars, VaList],
        false,
        0,
    ),
    stream_entry(
        "__builtin___fprintf_chk",
        "fprintf",
        "__fprintf_chk",
        &[Stream, Int, ConstChars],
        true,
        1,
    ),
    stream_entry(
        "__builtin___vfprintf_chk",
        "vfprintf",
        "__vfprintf_chk",
        &[Stream, Int, ConstChars, VaList],
        false,
        1,
    ),
    entry(
        "__builtin___memcpy_chk",
        "memcpy",
        "__memcpy_chk",
        Pointer,
        COPY,
        false,
        (3, None, Some(2)),
    ),
    entry(
        "__builtin___memmove_chk",
        "memmove",
        "__memmove_chk",
        Pointer,
        COPY,
        false,
        (3, None, Some(2)),
    ),
    entry(
        "__builtin___mempcpy_chk",
        "mempcpy",
        "__mempcpy_chk",
        Pointer,
        COPY,
        false,
        (3, None, Some(2)),
    ),
    entry(
        "__builtin___memset_chk",
        "memset",
        "__memset_chk",
        Pointer,
        &[Pointer, Int, Size, Size],
        false,
        (3, None, Some(2)),
    ),
    entry(
        "__builtin___strcpy_chk",
        "strcpy",
        "__strcpy_chk",
        Chars,
        STRING,
        false,
        (2, None, None),
    ),
    entry(
        "__builtin___stpcpy_chk",
        "stpcpy",
        "__stpcpy_chk",
        Chars,
        STRING,
        false,
        (2, None, None),
    ),
    entry(
        "__builtin___strcat_chk",
        "strcat",
        "__strcat_chk",
        Chars,
        STRING,
        false,
        (2, None, None),
    ),
    entry(
        "__builtin___strncpy_chk",
        "strncpy",
        "__strncpy_chk",
        Chars,
        BOUNDED_STRING,
        false,
        (3, None, Some(2)),
    ),
    entry(
        "__builtin___stpncpy_chk",
        "stpncpy",
        "__stpncpy_chk",
        Chars,
        BOUNDED_STRING,
        false,
        (3, None, Some(2)),
    ),
    // strncat appends to the existing string, so its length argument does
    // not bound the bytes it writes.
    entry(
        "__builtin___strncat_chk",
        "strncat",
        "__strncat_chk",
        Chars,
        BOUNDED_STRING,
        false,
        (3, None, None),
    ),
    entry(
        "__builtin___sprintf_chk",
        "sprintf",
        "__sprintf_chk",
        Int,
        &[Chars, Int, Size, ConstChars],
        true,
        (2, Some(1), None),
    ),
    entry(
        "__builtin___vsprintf_chk",
        "vsprintf",
        "__vsprintf_chk",
        Int,
        &[Chars, Int, Size, ConstChars, VaList],
        false,
        (2, Some(1), None),
    ),
    entry(
        "__builtin___snprintf_chk",
        "snprintf",
        "__snprintf_chk",
        Int,
        &[Chars, Size, Int, Size, ConstChars],
        true,
        (3, Some(2), Some(1)),
    ),
    entry(
        "__builtin___vsnprintf_chk",
        "vsnprintf",
        "__vsnprintf_chk",
        Int,
        &[Chars, Size, Int, Size, ConstChars, VaList],
        false,
        (3, Some(2), Some(1)),
    ),
];

/// The checked builtin with this builtin spelling.
#[must_use]
pub fn builtin(spelling: &str) -> Option<&'static CheckedBuiltin> {
    CHECKED_BUILTINS
        .iter()
        .find(|builtin| builtin.builtin == spelling)
}

/// The checked builtin whose checked entry point has this name.
#[must_use]
pub fn checked_entry(name: &str) -> Option<&'static CheckedBuiltin> {
    CHECKED_BUILTINS
        .iter()
        .find(|builtin| builtin.checked == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_plain_call_drops_exactly_the_size_and_flag() {
        for builtin in CHECKED_BUILTINS {
            let removed = usize::from(builtin.flag.is_some()) + usize::from(builtin.size.is_some());
            assert_eq!(
                builtin.plain_parameters().len() + removed,
                builtin.parameters.len(),
                "{}",
                builtin.builtin
            );
            if let Some(size) = builtin.size {
                assert_eq!(builtin.parameters[size], Param::Size);
            }
            if let Some(flag) = builtin.flag {
                assert_eq!(builtin.parameters[flag], Param::Int);
            }
            if let Some(length) = builtin.length {
                assert_eq!(builtin.parameters[length], Param::Size);
            }
            assert_eq!(
                builtin.builtin,
                format!("__builtin___{}_chk", builtin.plain)
            );
            assert_eq!(builtin.checked, format!("__{}_chk", builtin.plain));
        }
    }

    #[test]
    fn only_an_unknown_size_or_a_proven_length_drops_the_check() {
        let memcpy = builtin("__builtin___memcpy_chk").unwrap();
        let values = |length, size| move |index| [None, None, length, size][index];
        assert!(memcpy.check_is_redundant(values(Some(8), Some(u64::MAX))));
        assert!(memcpy.check_is_redundant(values(None, Some(u64::MAX))));
        assert!(memcpy.check_is_redundant(values(Some(8), Some(8))));
        assert!(!memcpy.check_is_redundant(values(Some(9), Some(8))));
        assert!(!memcpy.check_is_redundant(values(None, Some(8))));
        assert!(!memcpy.check_is_redundant(values(Some(8), None)));

        let strcpy = builtin("__builtin___strcpy_chk").unwrap();
        assert!(strcpy.check_is_redundant(|index| (index == 2).then_some(u64::MAX)));
        assert!(!strcpy.check_is_redundant(|index| (index == 2).then_some(1 << 20)));

        let sprintf = builtin("__builtin___sprintf_chk").unwrap();
        let format = |flag, size| move |index| [None, flag, size][index];
        assert!(sprintf.check_is_redundant(format(Some(0), Some(u64::MAX))));
        assert!(!sprintf.check_is_redundant(format(Some(1), Some(u64::MAX))));
        assert!(!sprintf.check_is_redundant(format(None, Some(u64::MAX))));

        let snprintf = builtin("__builtin___snprintf_chk").unwrap();
        let bounded = |length, flag, size| move |index| [None, length, flag, size][index];
        assert!(snprintf.check_is_redundant(bounded(Some(32), Some(0), Some(32))));
        assert!(!snprintf.check_is_redundant(bounded(Some(33), Some(0), Some(32))));
        assert!(!snprintf.check_is_redundant(bounded(Some(32), Some(1), Some(32))));

        let strncat = builtin("__builtin___strncat_chk").unwrap();
        assert!(!strncat.check_is_redundant(|index| [None, None, Some(1), Some(64)][index]));
    }
}
