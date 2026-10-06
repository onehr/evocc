//! C2 RED through the actual driver; machine-level boundary tests complement it.
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(flags: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(flags)
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

fn assembly(flags: &[&str], source: &str) -> String {
    let mut args = vec!["--emit=asm", "-O2", "--ub-checks=unchecked"];
    args.extend_from_slice(flags);
    let result = run(&args, source);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

fn instructions(text: &str) -> usize {
    text.lines().filter(|line| line.starts_with("    ")).count()
}

fn without_identity(text: &str) -> String {
    text.lines()
        .filter(|line| {
            !line.starts_with("// evocc-native-isel=") && !line.starts_with("// evocc-local-")
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

#[test]
fn real_driver_selects_adjacent_idioms() {
    for (source, fused, removed) in [
        (
            "unsigned long f(unsigned long x,unsigned long y){return x+(y<<3);}",
            ", lsl #3",
            1,
        ),
        (
            "unsigned f(unsigned x,unsigned y){return x^(y>>7);}",
            ", lsr #7",
            1,
        ),
        (
            "unsigned long f(unsigned long x,unsigned long y,unsigned long z){return x+y*z;}",
            "    madd ",
            1,
        ),
        (
            "unsigned long f(unsigned long x,unsigned long y,unsigned long z){return x-y*z;}",
            "    msub ",
            1,
        ),
        ("long f(long x,signed char y){return x+y;}", ", sxtb", 1),
        ("int f(long x,signed char y){return x<y;}", ", sxtb", 1),
        (
            "int f(unsigned long x,unsigned long y){return x<(y>>3);}",
            ", lsr #3",
            1,
        ),
        (
            "unsigned long f(const unsigned long *p,unsigned long i){return p[i];}",
            ", lsl #3]",
            2,
        ),
        (
            "void f(unsigned long *p,unsigned long i,unsigned long v){p[i]=v;}",
            ", lsl #3]",
            2,
        ),
    ] {
        let before = assembly(&[], source);
        let after = assembly(&["--isel-opt=local"], source);
        assert!(after.contains(fused), "{source}\n{after}");
        assert_eq!(
            instructions(&before) - instructions(&after),
            removed,
            "{source}\n{before}\n{after}"
        );
    }
}

#[test]
fn live_temporary_and_volatile_memory_are_not_erased() {
    let source =
        "unsigned long f(unsigned long x,unsigned long y){unsigned long s=y<<3;return (x+s)^s;}";
    assert_eq!(
        assembly(&[], source),
        without_identity(&assembly(&["--isel-opt=local"], source))
    );
    let source = "unsigned long f(const volatile unsigned long *p,unsigned long i){return p[i];}";
    let after = assembly(&["--isel-opt=local"], source);
    assert!(!after.contains(", lsl #3]"), "{after}");
    assert!(after.contains("    ldr "), "{after}");
}

#[test]
fn protected_and_malformed_requests_remain_closed() {
    let source = "unsigned long f(unsigned long x,unsigned long y){return x+(y<<3);}";
    assert_eq!(
        assembly(&["--isel-opt=local,constants"], source),
        assembly(&["--isel-opt=constants,local"], source)
    );
    for guard in [
        "-fno-omit-frame-pointer",
        "-mno-omit-leaf-frame-pointer",
        "-mgeneral-regs-only",
        "-fpatchable-function-entry=2",
    ] {
        assert_eq!(
            assembly(&[guard], source),
            without_identity(&assembly(&[guard, "--isel-opt=constants,local"], source))
        );
    }
    for flags in [
        vec!["--emit=asm", "--isel-opt=local,local"],
        vec!["--emit=asm", "--isel-opt=local,"],
        vec!["--emit=asm", "--isel-opt=local", "--isel-opt=constants"],
        vec!["--check", "--isel-opt=local"],
    ] {
        assert!(!run(&flags, source).status.success());
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_local_selection_matches_off_and_independent_clang() {
    use evocc_mir::emit::{CodegenOptions, FrameVariants};
    use evocc_mir::{RelocationModel, Target, UbCheckProfile};
    let fixture = include_str!("../../../tests/fixtures/codegen/n03d-local-selection.c");
    let signatures = [
        (
            "unsigned long",
            "shifted_add",
            "unsigned long x, unsigned long y",
            "x,y",
        ),
        (
            "unsigned",
            "shifted_xor",
            "unsigned x, unsigned y",
            "(unsigned)x,(unsigned)y",
        ),
        (
            "unsigned long",
            "multiply_add",
            "unsigned long x, unsigned long y, unsigned long z",
            "x,y,z",
        ),
        (
            "unsigned long",
            "multiply_sub",
            "unsigned long x, unsigned long y, unsigned long z",
            "x,y,z",
        ),
        (
            "long",
            "extended_add",
            "long x, signed char y",
            "(long)(x&0xffffff),(signed char)y",
        ),
        (
            "int",
            "extended_compare",
            "long x, signed char y",
            "(long)x,(signed char)y",
        ),
        (
            "unsigned long",
            "arithmetic_shift",
            "unsigned long x, long y",
            "x,(long)y",
        ),
        (
            "unsigned",
            "multiply_word",
            "unsigned x, unsigned y, unsigned z",
            "(unsigned)x,(unsigned)y,(unsigned)z",
        ),
        (
            "int",
            "shifted_compare",
            "unsigned long x, unsigned long y",
            "x,y",
        ),
        (
            "unsigned long",
            "shared_shift",
            "unsigned long x, unsigned long y",
            "x,y",
        ),
        (
            "unsigned long",
            "indexed_load",
            "const unsigned long *p, unsigned long i",
            "a,i",
        ),
        (
            "unsigned long",
            "volatile_index",
            "const volatile unsigned long *p, unsigned long i",
            "a,i",
        ),
    ];
    let renamed = |prefix: &str| {
        let mut text = fixture.to_owned();
        for name in signatures.iter().map(|row| row.1).chain(["indexed_store"]) {
            text = text.replace(&format!("{name}("), &format!("{prefix}_{name}("));
        }
        text
    };
    // Disposable test files must not be removed from retained evidence.
    let dir = std::env::temp_dir().join(format!("evocc-native-local-{}", std::process::id()));
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::create_dir(&dir).unwrap();
    let mut harness = String::new();
    for prefix in ["off", "local", "combined", "reference"] {
        for (ret, name, params, _) in signatures {
            harness.push_str(&format!("extern {ret} {prefix}_{name}({params});\n"));
        }
        harness.push_str(&format!(
            "extern void {prefix}_indexed_store(unsigned long *, unsigned long, unsigned long);\n"
        ));
        let source = renamed(prefix);
        if prefix == "reference" {
            std::fs::write(dir.join("reference.c"), source).unwrap();
        } else {
            let ir = run(&["--emit=evoir", "-O2"], &source);
            assert!(
                ir.status.success(),
                "{}",
                String::from_utf8_lossy(&ir.stderr)
            );
            let module = evocc_evoir::parse_section9_module(&ir.stdout).unwrap();
            let assembly = evocc_mir::compile_module_with_options(
                Target::Darwin,
                &module,
                &|_| true,
                RelocationModel::PositionIndependent,
                &CodegenOptions {
                    ub_checks: UbCheckProfile::Unchecked,
                    local_selection: prefix != "off",
                    compact_constants: prefix == "combined",
                    frame: if prefix == "combined" {
                        FrameVariants {
                            paired_saves: true,
                            shared_epilogue: true,
                            leaf_frames: true,
                        }
                    } else {
                        FrameVariants::OFF
                    },
                    ..Default::default()
                },
            )
            .unwrap();
            if prefix != "off" {
                assert!(assembly.contains("// evocc-local-selected"));
            }
            std::fs::write(dir.join(format!("{prefix}.s")), assembly).unwrap();
        }
    }
    harness.push_str("int main(void) { unsigned long x=1,y=0xffffffffffffffffUL,z=0x8000000000000000UL;\nfor(unsigned n=0;n<4096;++n){ unsigned long a[8]={0,1,2,3,4,5,6,7}; unsigned long i=n%8;\n");
    for prefix in ["off", "local", "combined"] {
        for (index, (_, name, _, args)) in signatures.iter().enumerate() {
            harness.push_str(&format!(
                "if ({prefix}_{name}({args}) != reference_{name}({args})) return {};\n",
                index + 1
            ));
        }
        harness.push_str(&format!("{{ unsigned long b[8]={{0,1,2,3,4,5,6,7}}; {prefix}_indexed_store(b,i,x); for(unsigned k=0;k<8;++k) if(b[k]!=(k==i?x:k)) return 20; }}\n"));
    }
    harness.push_str("x=x*6364136223846793005UL+1; y^=x; z=(z<<1)|(z>>63); } return 0; }\n");
    std::fs::write(dir.join("harness.c"), harness).unwrap();
    let output = Command::new("clang")
        .args([
            "-O2",
            "off.s",
            "local.s",
            "combined.s",
            "reference.c",
            "harness.c",
            "-o",
            "probe",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "retained {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(dir.join("probe")).output().unwrap();
    assert!(
        output.status.success(),
        "exit {:?}; retained {}",
        output.status,
        dir.display()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
