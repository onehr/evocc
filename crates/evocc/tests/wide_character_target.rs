use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(arguments: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(arguments)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

// Measured with GCC-14 and Clang-19, independently at O0/O2:
// .evidence/builtin-probes/20260909-wide-character-target/{wide.c,results.txt}.
const SOURCE: &str = r#"
typedef __WCHAR_TYPE__ wchar_t;
static const wchar_t text[] = L"\u4f60\U0001f642" "!";
static wchar_t exact[2] = L"ab";
_Static_assert(sizeof(wchar_t) == EXPECT_BYTES, "target wchar type");
_Static_assert(__SIZEOF_WCHAR_T__ == EXPECT_BYTES, "predefined size");
_Static_assert(__WCHAR_WIDTH__ == EXPECT_BYTES * 8, "predefined width");
_Static_assert(__ARM_SIZEOF_WCHAR_T == EXPECT_BYTES, "ABI predefined size");
_Static_assert(__WCHAR_MIN__ == 0, "unsigned target wchar");
_Static_assert(sizeof(*L"x") == EXPECT_BYTES, "literal element");
_Static_assert(sizeof(*"x") == 1, "ordinary literal decay uses the same rule");
_Static_assert(sizeof(L'x') == EXPECT_BYTES, "character constant");
_Static_assert(__builtin_types_compatible_p(typeof(L'x'), wchar_t), "character type");
_Static_assert(__builtin_types_compatible_p(typeof(*L"x"), wchar_t), "string element type");
#if EXPECT_BYTES == 2
_Static_assert(__WCHAR_MAX__ == 65535, "short maximum");
_Static_assert(sizeof(text) == 10, "UTF16 units and terminator");
#else
_Static_assert(__WCHAR_MAX__ == 4294967295U, "hosted maximum");
_Static_assert(sizeof(text) == 16, "UTF32 units and terminator");
_Static_assert(L'\U0001f642' == 0x1f642, "non-BMP scalar");
#endif
int main(void) {
  wchar_t local[] = L"\u4f60" "!";
  wchar_t local_exact[2] = L"ab";
  const wchar_t *pointer = L"\u4f60";
  if (local[0] != 0x4f60 || local[1] != '!' || local[2] != 0) return 1;
  if (local_exact[0] != 'a' || local_exact[1] != 'b') return 2;
  if (exact[0] != 'a' || exact[1] != 'b' || *pointer != L'\u4f60') return 3;
  if (text[0] != 0x4f60) return 4;
  if (*"x" != 'x' || *L"x" != L'x') return 5;
#if EXPECT_BYTES == 2
  return text[1] != 0xd83d || text[2] != 0xde42 || text[3] != '!' || text[4] != 0;
#else
  return text[1] != 0x1f642 || text[2] != '!' || text[3] != 0;
#endif
}
"#;

fn check(flags: &[&str], expected_bytes: &str) {
    for action in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let mut arguments = vec![action, expected_bytes];
        arguments.extend_from_slice(flags);
        let output = run(&arguments, SOURCE);
        assert!(
            output.status.success(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        if action == "--emit=asm" {
            assert!(!output.stdout.is_empty());
        } else {
            assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "0");
        }
    }
}

#[test]
fn hosted_wide_values_use_unsigned_int_and_utf32() {
    check(&[], "-DEXPECT_BYTES=4");
}

#[test]
fn short_wide_values_use_unsigned_short_and_utf16() {
    check(&["-fshort-wchar"], "-DEXPECT_BYTES=2");
}

#[test]
fn the_last_wide_character_option_controls_all_consumers() {
    check(&["-fshort-wchar", "-fno-short-wchar"], "-DEXPECT_BYTES=4");
    check(&["-fno-short-wchar", "-fshort-wchar"], "-DEXPECT_BYTES=2");
}

#[test]
fn short_scalar_characters_do_not_silently_keep_a_trailing_surrogate() {
    // GCC keeps DE42 with a warning; Clang refuses this scalar literal.
    // The reviewed conflict policy chooses refusal, not truncation.
    let output = run(
        &["--parse-only", "-fshort-wchar"],
        "int f(void) { return L'\\U0001f642'; }",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("error[E1006]"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
