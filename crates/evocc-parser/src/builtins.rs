//! The builtin spellings that the front end resolves. `__has_builtin`
//! answers from `crates/evocc-cpp/predefined/has-builtin.txt`, and a test
//! checks that the file lists exactly the implemented ones, [`names`].

use evocc_lex::TokenKind;

/// Builtins that the parser promotes to a keyword token.
const KEYWORDS: &[(&str, TokenKind)] = &[
    ("__builtin_expect", TokenKind::BuiltinExpect),
    ("__atomic_load_n", TokenKind::BuiltinAtomic),
    ("__atomic_store_n", TokenKind::BuiltinAtomic),
    ("__builtin_constant_p", TokenKind::BuiltinConstantP),
    ("__builtin_choose_expr", TokenKind::BuiltinChooseExpr),
    (
        "__builtin_types_compatible_p",
        TokenKind::BuiltinTypesCompatibleP,
    ),
    ("__builtin_classify_type", TokenKind::BuiltinClassifyType),
    ("__builtin_offsetof", TokenKind::BuiltinOffsetof),
    ("__builtin_object_size", TokenKind::BuiltinObjectSize),
    (
        "__builtin_dynamic_object_size",
        TokenKind::BuiltinObjectSize,
    ),
    ("__builtin_has_attribute", TokenKind::BuiltinHasAttribute),
    ("__builtin_strlen", TokenKind::BuiltinStrlen),
    ("__builtin_va_start", TokenKind::BuiltinVaStart),
    ("__builtin_va_end", TokenKind::BuiltinVaEnd),
    ("__builtin_va_copy", TokenKind::BuiltinVaCopy),
    ("__builtin_va_arg", TokenKind::BuiltinVaArg),
    ("__builtin_unreachable", TokenKind::BuiltinUnreachable),
    ("__builtin_trap", TokenKind::BuiltinTrap),
    ("__builtin_return_address", TokenKind::BuiltinReturnAddress),
    ("__builtin_frame_address", TokenKind::BuiltinFrameAddress),
    ("__builtin_add_overflow", TokenKind::BuiltinAddOverflow),
    ("__builtin_sub_overflow", TokenKind::BuiltinSubOverflow),
    ("__builtin_mul_overflow", TokenKind::BuiltinMulOverflow),
];

/// Builtins with the prototype of the C library function that they name.
const LIBRARY: &[&str] = &[
    "__builtin___clear_cache",
    "__builtin_memset",
    "__builtin_nan",
    "__builtin_nanf",
    "__builtin_nanl",
    "__builtin_memcpy",
    "__builtin_memcmp",
    "__builtin_strcmp",
    "__builtin_strncmp",
    "__builtin_alloca",
    "__builtin_memmove",
    "__builtin_printf",
    "__builtin_sprintf",
    "__builtin_snprintf",
    "__builtin_puts",
    "__builtin_strchr",
    "__builtin_strncpy",
    "__builtin_strcat",
    "__builtin_strncat",
    "__builtin_memchr",
    "__builtin_malloc",
    "__builtin_free",
    "__builtin_abs",
    "__builtin_labs",
    "__builtin_llabs",
    "__builtin_fabs",
    "__builtin_fabsf",
    "__builtin_fabsl",
    "__builtin_strcpy",
    "__builtin_abort",
    "__builtin_exit",
    "__builtin__Exit",
    "__builtin_prefetch",
    "__sync_synchronize",
    "__atomic_thread_fence",
    "__atomic_signal_fence",
    "__builtin_extract_return_addr",
];

const CTYPE: &[&str] = &[
    "__builtin_isdigit",
    "__builtin_isalpha",
    "__builtin_isspace",
    "__builtin_toupper",
    "__builtin_tolower",
];

const BIT: &[&str] = &[
    "__builtin_ffs",
    "__builtin_ffsl",
    "__builtin_ffsll",
    "__builtin_clz",
    "__builtin_clzl",
    "__builtin_clzll",
    "__builtin_ctz",
    "__builtin_ctzl",
    "__builtin_ctzll",
    "__builtin_popcount",
    "__builtin_popcountl",
    "__builtin_popcountll",
    "__builtin_clrsb",
    "__builtin_clrsbl",
    "__builtin_clrsbll",
    "__builtin_parity",
    "__builtin_parityl",
    "__builtin_parityll",
    "__builtin_bswap16",
    "__builtin_bswap32",
    "__builtin_bswap64",
];

/// Builtins that the expression parser selects by spelling, without a
/// keyword token: the vector builtins and the GNU argument pack.
pub(crate) const SPELLED: &[&str] = &[
    "__builtin_shuffle",
    "__builtin_shufflevector",
    "__builtin_convertvector",
    "__builtin_va_arg_pack",
    "__builtin_va_arg_pack_len",
];

/// Library builtins that the front end recognizes in order to refuse them:
/// `E2933` and `E2935`. `__has_builtin` reports them as absent.
const REFUSED: &[&str] = &["__builtin_puts", "__builtin_exit", "__builtin__Exit"];

/// The read-modify-write operations of the legacy `__sync` and the GNU
/// `__atomic` fetch-op and op-fetch forms.
const LEGACY_OPERATIONS: &[&str] = &["add", "sub", "and", "or", "xor", "nand"];

/// The keyword token of a builtin spelling, or `None` for an identifier.
pub(crate) fn token_kind(name: &str) -> Option<TokenKind> {
    // Every builtin spelling is reserved: it begins with two underscores.
    if !name.starts_with("__") {
        return None;
    }
    if let Some(&(_, kind)) = KEYWORDS.iter().find(|(spelling, _)| *spelling == name) {
        return Some(kind);
    }
    if super::legacy_atomic(name).is_some() || super::gnu_atomic(name).is_some() {
        return Some(TokenKind::BuiltinAtomic);
    }
    if super::floating::is_builtin(name)
        || LIBRARY.contains(&name)
        || evocc_chir::fortify::builtin(name).is_some()
    {
        return Some(TokenKind::BuiltinLibrary);
    }
    if CTYPE.contains(&name) {
        return Some(TokenKind::BuiltinCtype);
    }
    BIT.contains(&name).then_some(TokenKind::BuiltinBit)
}

/// Every builtin spelling that the front end implements, sorted. The
/// AArch64 target intrinsics are included when they lower to target
/// operations and their prototype does not need the `arm_neon.h` pragma.
#[must_use]
pub fn names() -> Vec<String> {
    let mut names: Vec<String> = KEYWORDS
        .iter()
        .map(|(name, _)| *name)
        .chain(
            LIBRARY
                .iter()
                .copied()
                .filter(|name| !REFUSED.contains(name)),
        )
        .chain(CTYPE.iter().copied())
        .chain(BIT.iter().copied())
        .chain(SPELLED.iter().copied())
        .chain(super::floating::BUILTINS.iter().copied())
        .chain(
            evocc_chir::fortify::CHECKED_BUILTINS
                .iter()
                .map(|builtin| builtin.builtin),
        )
        .chain(["__builtin_va_list"])
        .map(str::to_owned)
        .collect();
    for operation in LEGACY_OPERATIONS {
        names.push(format!("__sync_fetch_and_{operation}"));
        names.push(format!("__sync_{operation}_and_fetch"));
    }
    for operation in LEGACY_OPERATIONS {
        names.push(format!("__atomic_fetch_{operation}"));
        names.push(format!("__atomic_{operation}_fetch"));
    }
    names.extend(
        [
            "__atomic_load",
            "__atomic_store",
            "__atomic_exchange",
            "__atomic_compare_exchange",
            "__atomic_exchange_n",
            "__atomic_compare_exchange_n",
            "__atomic_test_and_set",
            "__atomic_clear",
            "__atomic_always_lock_free",
            "__atomic_is_lock_free",
        ]
        .map(str::to_owned),
    );
    names.extend(
        [
            "__sync_bool_compare_and_swap",
            "__sync_val_compare_and_swap",
            "__sync_lock_test_and_set",
            "__sync_lock_release",
        ]
        .map(str::to_owned),
    );
    names.extend(
        evocc_types::neon::names()
            .filter(|name| {
                name.starts_with("__builtin_aarch64_")
                    && evocc_types::neon::operation(name).is_some()
                    && evocc_types::neon::signature(name)
                        .is_some_and(|types| !types.iter().any(|ty| names_tuple(ty)))
            })
            .map(str::to_owned),
    );
    names.sort();
    names.dedup();
    names
}

/// Whether a target signature type names a tuple record. The `arm_neon.h`
/// pragma creates those records, so a builtin that uses one resolves only
/// after the pragma, and `__has_builtin` does not report it.
fn names_tuple(ty: &str) -> bool {
    let base = ty
        .trim_start_matches("ptr(")
        .trim_end_matches(')')
        .trim_start_matches("const ");
    evocc_types::neon::TUPLE_BASES.iter().any(|(public, _)| {
        base.strip_prefix(public)
            .is_some_and(|count| matches!(count, "x2_t" | "x3_t" | "x4_t"))
    })
}
