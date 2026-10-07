//! Source bytes are not uniformly Unicode strings; consumers retain boundaries.
use std::io::Write;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

fn run(source: &[u8], mode: &str, route: usize, flags: &[&str]) -> Output {
    static SERIAL: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "evocc-source-bytes-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    let file = dir.join("input.c");
    std::fs::write(&file, source).unwrap();
    let wrapper = dir.join("wrapper.c");
    std::fs::write(&wrapper, b"#include \"input.c\"\n").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .args(["--target=aarch64-unknown-linux-gnu", mode])
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if route == 0 {
        command.arg("/dev/stdin");
    } else {
        command.arg(if route == 1 { &file } else { &wrapper });
    }
    let mut child = command.spawn().unwrap();
    if route == 0 {
        let _ = child.stdin.take().unwrap().write_all(source);
    }
    let output = child.wait_with_output().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    output
}

#[test]
fn pre_c11_prefix_identifiers_expand_on_every_source_route() {
    for route in 0..3 {
        for mode in ["-std=gnu89", "-std=gnu99", "-std=c99"] {
            let out = run(b"#define u8\nint main(void){char a[]=u8\"\xff\";return sizeof(a)!=2||(unsigned char)a[0]!=255;}", "--interpret-evoir", route, &[mode]);
            assert!(
                out.status.success(),
                "{route} {mode}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(out.stdout, b"0\n");
        }
    }
}

#[test]
fn permitted_bytes_keep_their_values_on_all_input_routes() {
    let source = b"/*\xff*/ int/**/main(void){char a[]=\"\xff\";char b[]=\"\xc3\xbf\";char c[]=\"\\xff\";return sizeof(a)!=2||sizeof(b)!=3||sizeof(c)!=2||(unsigned char)a[0]!=255||a[1]||((unsigned char)'\xff')!=255||(unsigned char)b[0]!=195||(unsigned char)b[1]!=191||(unsigned char)c[0]!=255;}";
    for route in 0..3 {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let out = run(source, mode, route, &[]);
            assert!(
                out.status.success(),
                "{route} {mode}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(out.stdout, b"0\n");
            }
        }
    }
}

#[test]
fn invalid_sequences_comments_splices_and_macro_transport_preserve_token_boundaries() {
    let source=b"#define BYTES \"\xc0\xaf\x80\xf5\xff\"\n// \xff\nint/**/main(void){char a[]=BYTES;char b[]=\"\xff\\\n\\\"/*x*/\";return sizeof(a)!=6||sizeof(b)!=8||(unsigned char)a[0]!=192||(unsigned char)a[4]!=255||(unsigned char)b[0]!=255||b[1]!=34;}";
    for route in 0..3 {
        let out = run(source, "--interpret-evoir", route, &[]);
        assert!(
            out.status.success(),
            "{route}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
}

#[test]
fn escaped_raw_bytes_do_not_turn_into_literal_octal_digits() {
    let source = b"#define BYTE \"\\\xff\"\nint main(void){char a[]=BYTE;char b[]=\"\\\\\xff\";char c[]=\"\\\\\\\xff\";char d[]=\"\xff123\";char e[]=\"\\x1\xff\";char f[]=\"\xff\\\xff\";return sizeof(a)!=2||(unsigned char)a[0]!=255||a[1]||sizeof(b)!=3||b[0]!=92||(unsigned char)b[1]!=255||b[2]||sizeof(c)!=3||c[0]!=92||(unsigned char)c[1]!=255||c[2]||sizeof(d)!=5||(unsigned char)d[0]!=255||d[1]!='1'||d[4]||sizeof(e)!=3||e[0]!=1||(unsigned char)e[1]!=255||e[2]||sizeof(f)!=3||(unsigned char)f[0]!=255||(unsigned char)f[1]!=255||f[2]||'\\\xff'!=EXPECT;}";
    for route in 0..3 {
        for flags in [
            ["-funsigned-char", "-DEXPECT=255"],
            ["-fsigned-char", "-DEXPECT=-1"],
        ] {
            for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
                let out = run(source, mode, route, &flags);
                assert!(
                    out.status.success(),
                    "{mode}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                if mode.starts_with("--interpret") {
                    assert_eq!(out.stdout, b"0\n", "{mode}, route {route}, {flags:?}");
                }
            }
        }
    }
}

#[test]
fn unsupported_locations_and_consumers_still_reject_invalid_encoding() {
    for source in [
        &b"int \xff;"[..],
        &b"int main(void){return \xff;}"[..],
        &b"char*x=L\"\xff\";"[..],
        &b"char*x=u8\"\xff\";"[..],
        &b"#include \"\xff.h\"\n"[..],
        &b"asm(\"\xff\");"[..],
        &b"#define W(x) L##x\nchar*x=W(\"\xff\");"[..],
        &b"#define S(x) #x\nchar*x=S(\"\xff\");"[..],
        &b"char*x=L\"ok\" \"\xff\";"[..],
        &b"char*x=\"\xff\" L\"ok\";"[..],
        &b"int x __attribute__((section(\"\xff\")));"[..],
        &b"#define HEADER \"\xff.h\"\n#include HEADER\n"[..],
        &b"#define BODY asm(\"\xff\");\nBODY"[..],
        &b"_Pragma(\"\xff\") int x;"[..],
        &b"#define P \"\xff\"\n_Pragma(P) int x;"[..],
        &b"#line 2 \"\xff\"\nint x;"[..],
    ] {
        for route in 0..3 {
            let out = run(source, "--emit=evoir", route, &[]);
            assert!(!out.status.success(), "{source:?}");
            assert!(
                String::from_utf8_lossy(&out.stderr).contains("E0002"),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

#[test]
fn unsigned_target_character_values_and_explicit_signed_conversion_are_distinct() {
    let source = b"int main(void){return '\xff'!=255 || '\\xff'!=255 || (signed char)'\xff'!=-1 || (unsigned char)'\xff'!=255;}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let out = run(source, mode, 0, &[]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, b"0\n");
    }
}

#[test]
fn plain_character_options_preserve_type_identity_and_literal_bytes() {
    let source = b"#if '\xff' != EXPECT\n#error preprocessor character signedness\n#endif\n#if (EXPECT < 0) == defined(__CHAR_UNSIGNED__)\n#error wrong char macro\n#endif\nint main(void){char x='\xff'; char a[]=\"\xff\"; return '\xff' != EXPECT || x != EXPECT || a[0] != EXPECT || sizeof(a)!=2 || a[1] || __builtin_types_compatible_p(char,signed char) || __builtin_types_compatible_p(char,unsigned char);}";
    for (flags, expected) in [
        (vec!["-fsigned-char"], "-DEXPECT=-1"),
        (vec!["-funsigned-char"], "-DEXPECT=255"),
        (vec!["-fsigned-char", "-funsigned-char"], "-DEXPECT=255"),
        (vec!["-funsigned-char", "-fsigned-char"], "-DEXPECT=-1"),
    ] {
        let mut flags = flags;
        flags.push(expected);
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let out = run(source, mode, 0, &flags);
            assert!(
                out.status.success(),
                "{flags:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(out.stdout, b"0\n");
            }
        }
    }
}
