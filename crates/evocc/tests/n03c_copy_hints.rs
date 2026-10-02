//! Public-driver boundary for the optional allocator candidate order.
use std::io::Write;
use std::process::{Command, Output, Stdio};

const SOURCE: &str = "unsigned long f(int p,unsigned long x,unsigned long y){unsigned long a;if(p)a=x;else a=y;return a+7;}";

fn run(flags: &[&str]) -> Output {
    run_source(flags, SOURCE)
}

fn run_source(flags: &[&str], source: &str) -> Output {
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

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_copy_hints_match_off_and_clang_across_edges_and_calls() {
    use evocc_mir::emit::{CodegenOptions, FrameVariants};
    let fixture = include_str!("../../../tests/fixtures/codegen/n03c-copy-hints.c");
    let signatures = [
        ("U", "choose", "unsigned n, U x, U y", "n,x,y"),
        ("U", "rotate", "unsigned n, U a, U b, U c", "n%7,x,y,z"),
        ("U", "dispatch", "unsigned n, U a, U b", "n,x,y"),
        ("U", "shuffled", "U a, U b, U c", "x,y,z"),
        ("Wide", "wide_across", "Wide x, U y", "((Wide)x<<64)|y,z"),
        ("Vec", "vector_across", "Vec x, U y", "((Vec){x,y}),z"),
    ];
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = root.join(format!(
        ".evidence/codegen/n03c-t2/native-copies-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(directory.parent().unwrap()).unwrap();
    std::fs::create_dir(&directory).unwrap();
    let mut harness = String::from("typedef unsigned long U; typedef unsigned __int128 Wide; typedef U Vec __attribute__((vector_size(16)));\n");
    let mut assemblies = Vec::new();
    for prefix in ["off", "hint", "combined", "reference"] {
        let mut source = fixture.to_owned();
        for (ret, name, params, _) in signatures {
            source = source.replace(&format!("{name}("), &format!("{prefix}_{name}("));
            harness.push_str(&format!("extern {ret} {prefix}_{name}({params});\n"));
        }
        if prefix == "reference" {
            std::fs::write(directory.join("reference.c"), source).unwrap();
        } else {
            // The public C driver targets Linux. Reuse its canonical IR for
            // the existing Darwin native API, just as the native corpora do.
            let output = run_source(&["--emit=evoir", "-O2"], &source);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
            let assembly = evocc_mir::compile_module_with_options(
                evocc_mir::Target::Darwin,
                &module,
                &|_| true,
                evocc_mir::RelocationModel::PositionIndependent,
                &CodegenOptions {
                    ub_checks: evocc_mir::UbCheckProfile::Unchecked,
                    allocation: evocc_mir::generic::allocation::AllocationOptions {
                        copy_hints: prefix != "off",
                    },
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
            assemblies.push(assembly.replace(&format!("{prefix}_"), ""));
            std::fs::write(directory.join(format!("{prefix}.s")), assembly).unwrap();
        }
    }
    let moves = |text: &str| {
        text.split("_rotate:")
            .next()
            .unwrap()
            .lines()
            .filter(|line| line.starts_with("    mov "))
            .count()
    };
    assert!(
        moves(&assemblies[1]) < moves(&assemblies[0]),
        "the choose function must exercise hints, not just the header"
    );
    harness.push_str("__attribute__((noinline)) U boundary(U a,U b,U c,U d){ __asm__ volatile(\"movi v8.16b, #165\" ::: \"v8\"); return ((a+b)^c)+d; }\nint main(void){U x=1,y=~0UL,z=1UL<<63; for(unsigned n=0;n<4096;++n){\n");
    for prefix in ["off", "hint", "combined"] {
        for (index, (ret, name, _, args)) in signatures.iter().enumerate() {
            if *ret == "Vec" {
                harness.push_str(&format!("{{Vec a={prefix}_{name}({args}), b=reference_{name}({args}); if(a[0]!=b[0] || a[1]!=b[1]) return {};}}\n", index + 1));
            } else {
                harness.push_str(&format!(
                    "if({prefix}_{name}({args})!=reference_{name}({args})) return {};\n",
                    index + 1
                ));
            }
        }
    }
    harness.push_str("x=x*6364136223846793005UL+1;y^=x;z=(z<<1)|(z>>63);}return 0;}\n");
    std::fs::write(directory.join("harness.c"), harness).unwrap();
    let output = Command::new("clang")
        .args([
            "-O2",
            "off.s",
            "hint.s",
            "combined.s",
            "reference.c",
            "harness.c",
            "-o",
            "probe",
        ])
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "retained {}: {}",
        directory.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(directory.join("probe")).output().unwrap();
    assert!(
        output.status.success(),
        "exit {:?}; retained {}",
        output.status,
        directory.display()
    );
    std::fs::remove_dir_all(directory).unwrap();
}

fn assembly(flags: &[&str]) -> String {
    let mut args = vec!["--emit=asm", "-O2", "--ub-checks=unchecked"];
    args.extend_from_slice(flags);
    let output = run(&args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn n03c_copy_hint_driver_is_opt_in_and_records_the_request() {
    let off = assembly(&[]);
    assert!(!off.contains("evocc-native-regalloc="));
    let on = assembly(&["--regalloc-opt=copy-hints"]);
    assert!(on.contains("// evocc-native-regalloc=copy-hints\n"));
    assert_eq!(on, assembly(&["--regalloc-opt=copy-hints"]));
}

#[test]
fn n03c_copy_hint_protected_profiles_and_bad_requests_stay_closed() {
    for flag in [
        "-fno-omit-frame-pointer",
        "-mno-omit-leaf-frame-pointer",
        "-mgeneral-regs-only",
        "-fpatchable-function-entry=2",
    ] {
        let off = assembly(&[flag]);
        let on = assembly(&[flag, "--regalloc-opt=copy-hints"]);
        assert_eq!(
            off,
            on.replace("// evocc-native-regalloc=copy-hints\n", ""),
            "{flag}"
        );
    }
    for flags in [
        vec!["--emit=asm", "--regalloc-opt="],
        vec!["--emit=asm", "--regalloc-opt=unknown"],
        vec!["--emit=asm", "--regalloc-opt=copy-hints,copy-hints"],
        vec![
            "--emit=asm",
            "--regalloc-opt=copy-hints",
            "--regalloc-opt=copy-hints",
        ],
        vec!["--check", "--regalloc-opt=copy-hints"],
    ] {
        let output = run(&flags);
        assert!(!output.status.success(), "{flags:?}");
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("allocation")
                || String::from_utf8_lossy(&output.stderr).contains("regalloc")
        );
    }
}
