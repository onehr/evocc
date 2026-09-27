//! Section 1's second target, and Section 8's platform differences.
//!
//! "Its first target is the AArch64 scalar subset, first hosted as Darwin arm64
//! ... and then as Linux AArch64 for the kernel. The two targets share every
//! rule below except the ones the calling convention and the symbol syntax
//! name."
//!
//! Those are the symbol prefix, the page-relative spelling (`@PAGE`/`@PAGEOFF`
//! against `:pg_hi21:`/`:lo12:`), the section names and the local-label prefix.
//! The differential oracle is the sealed container's GCC-14: the same C corpus
//! that runs natively on Darwin is compiled by this pipeline for Linux, and
//! GCC-14 assembles, links and runs it inside the container beside its own
//! build of the same source.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, Driver, NativeStop, Target};

/// The runtime and image the build-environment contract seals.
const CONTAINER_BIN: &str = "/usr/local/bin/container";
const IMAGE: &str = "evo-flight-lab:sealed";

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories below the workspace root")
        .to_path_buf()
}

/// Whether the sealed image is present and its runtime answers.
///
/// The container is a shared resource this test does not own: when it is not
/// running, or the image is absent, the reason is printed and nothing is
/// approximated.
fn container() -> Result<String, String> {
    if !Path::new(CONTAINER_BIN).exists() {
        return Err(format!("{CONTAINER_BIN} is absent"));
    }
    let listed = Command::new(CONTAINER_BIN)
        .args(["image", "list"])
        .output()
        .map_err(|error| format!("{CONTAINER_BIN}: {error}"))?;
    if !listed.status.success() {
        return Err(format!(
            "the container runtime is not started: {}",
            String::from_utf8_lossy(&listed.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&listed.stdout).into_owned();
    if !text.contains("evo-flight-lab") {
        return Err(format!("{IMAGE} is not present"));
    }
    Ok(IMAGE.to_owned())
}

/// The Day 7 memory corpus, materialized by its own authority.
fn corpus(with_scalar: bool) -> Result<Vec<PathBuf>, String> {
    let directory = std::env::temp_dir().join(format!("evocc-linux-corpus-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let output = Command::new("python3")
        .arg(workspace().join("tests/acceptance/day7-memory-corpus.py"))
        .arg("materialize-defined")
        .arg(&directory)
        .output()
        .map_err(|error| format!("python3 is absent: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|suffix| suffix == "c"))
        .collect();
    sources.sort();

    if !with_scalar {
        return Ok(sources);
    }

    // The Day 5 scalar corpus too, so the Linux table is the Darwin one's
    // counterpart rather than a smaller sample. Its rejected cases stay out for
    // the same reason: an undefined program has no exit status to compare.
    let output = Command::new("python3")
        .arg(workspace().join("tests/acceptance/day5-scalar-interpreter.py"))
        .arg("emit-jsonl")
        .output()
        .map_err(|error| format!("python3 is absent: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut scalar = Vec::new();
    for line in text.lines() {
        let Some((id, body)) = json_id_and_source(line) else {
            continue;
        };
        if !id.starts_with("defined.") {
            continue;
        }
        let path = directory.join(format!("{id}.c"));
        std::fs::write(&path, body).map_err(|error| error.to_string())?;
        scalar.push(path);
    }
    scalar.sort();
    sources.extend(scalar);
    Ok(sources)
}

/// The `id` and `source` of one JSON line, without a JSON dependency.
fn json_id_and_source(line: &str) -> Option<(String, String)> {
    let field = |key: &str| -> Option<String> {
        let start = line.find(&format!("\"{key}\": \""))? + key.len() + 5;
        let mut text = String::new();
        let mut escaped = false;
        for character in line[start..].chars() {
            if escaped {
                text.push(match character {
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                });
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                return Some(text);
            } else {
                text.push(character);
            }
        }
        None
    };
    Some((field("id")?, field("source")?))
}

/// One batch run inside the container: GCC-14 builds and runs both sides.
///
/// Every program is built twice, once from this pipeline's assembly and once
/// from its own C source, and both are run. The script writes one line per
/// program so the outcome is data rather than an exit code.
const RUNNER: &str = r#"#!/bin/sh
cd /work || exit 2
for s in *.s; do
  base="${s%.s}"
  if ! gcc-14 -o "$base.mine" "$s" 2>"$base.link.err"; then
    echo "$base link $(head -c 200 "$base.link.err" | tr '\n' ' ')"
    continue
  fi
  ./"$base.mine" >/dev/null 2>&1
  mine=$?
  if ! gcc-14 -std=c11 -O0 -o "$base.theirs" "$base.c" 2>"$base.cc.err"; then
    echo "$base reference $(head -c 200 "$base.cc.err" | tr '\n' ' ')"
    continue
  fi
  ./"$base.theirs" >/dev/null 2>&1
  theirs=$?
  echo "$base run $mine $theirs"
done
"#;

struct Row {
    programs: usize,
    front_end_refused: BTreeMap<String, usize>,
    lowering_refused: BTreeMap<String, usize>,
    stopped: BTreeMap<&'static str, usize>,
    compiled: usize,
    linked: usize,
    ran: usize,
    agreeing: usize,
    disagreements: Vec<(String, u8, u8)>,
}

/// The first diagnostic in a complaint, so refusals rank by what stopped them.
fn first_diagnostic(text: &str) -> String {
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if word.len() >= 5 && word.starts_with('E') && word[1..].chars().all(|c| c.is_ascii_digit())
        {
            return word.to_owned();
        }
    }
    "unnamed".to_owned()
}

/// The default the workspace gate runs: the memory corpus alone.
///
/// The container is a shared resource with a runtime to start, so the gate asks
/// it for the smaller corpus and the standing campaign below asks for both.
#[test]
fn the_linux_profile_agrees_with_gcc_in_the_sealed_container() {
    measure_against_gcc(false);
}

/// Both corpora, the counterpart of the Darwin table.
#[test]
#[ignore = "standing campaign: the whole C corpus through the sealed container"]
fn the_whole_c_corpus_agrees_with_gcc_in_the_sealed_container() {
    measure_against_gcc(true);
}

fn measure_against_gcc(with_scalar: bool) {
    let image = match container() {
        Ok(image) => image,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    let driver = match Driver::locate() {
        Ok(driver) => driver,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    let sources = match corpus(with_scalar) {
        Ok(sources) => sources,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("target={}", Target::Linux.mnemonic());
    println!("image={image}");
    println!("driver={}", driver.binary().display());

    let mut row = Row {
        programs: sources.len(),
        front_end_refused: BTreeMap::new(),
        lowering_refused: BTreeMap::new(),
        stopped: BTreeMap::new(),
        compiled: 0,
        linked: 0,
        ran: 0,
        agreeing: 0,
        disagreements: Vec::new(),
    };

    // Everything the container sees lives in one directory it mounts.
    let work = std::env::temp_dir().join(format!("evocc-linux-work-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("a working directory");
    let mut names = Vec::new();
    for source in &sources {
        let name = source
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let evoir = match driver.emit_evoir(source, true) {
            Ok(text) => text,
            Err(complaint) => {
                *row.front_end_refused
                    .entry(first_diagnostic(&complaint))
                    .or_default() += 1;
                continue;
            }
        };
        let Ok(module) = parse_section9_module(evoir.as_bytes()) else {
            *row.front_end_refused
                .entry("unparsed".to_owned())
                .or_default() += 1;
            continue;
        };
        let text = match compile_module_for(Target::Linux, &module, &|_| true) {
            Ok(text) => text,
            Err(stop) => {
                match &stop {
                    NativeStop::Lowering(codes) => {
                        for code in codes {
                            *row.lowering_refused.entry((*code).to_owned()).or_default() += 1;
                        }
                    }
                    other => *row.stopped.entry(other.component()).or_default() += 1,
                }
                continue;
            }
        };
        row.compiled += 1;
        std::fs::write(work.join(format!("{name}.s")), text).expect("the emitted text");
        std::fs::copy(source, work.join(format!("{name}.c"))).expect("the source");
        names.push(name);
    }

    let runner = work.join("runner.sh");
    std::fs::write(&runner, RUNNER).expect("the runner");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o755))
            .expect("an executable runner");
    }

    let output = Command::new(CONTAINER_BIN)
        .arg("run")
        .arg("--rm")
        .arg("--platform")
        .arg("linux/arm64")
        .arg("--network")
        .arg("none")
        .arg("--volume")
        .arg(format!("{}:/work", work.display()))
        .arg(&image)
        .arg("/work/runner.sh")
        .output()
        .expect("run the sealed container");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "the container run failed:\n{}\n{stdout}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut seen = BTreeMap::new();
    for line in stdout.lines() {
        let mut fields = line.split_whitespace();
        let (Some(name), Some(kind)) = (fields.next(), fields.next()) else {
            continue;
        };
        match kind {
            "link" => {
                *row.stopped.entry("link").or_default() += 1;
                seen.insert(name.to_owned(), ());
            }
            "reference" => {
                *row.stopped.entry("reference").or_default() += 1;
                seen.insert(name.to_owned(), ());
            }
            "run" => {
                let mine: u8 = fields.next().and_then(|v| v.parse().ok()).unwrap_or(255);
                let theirs: u8 = fields.next().and_then(|v| v.parse().ok()).unwrap_or(255);
                row.linked += 1;
                row.ran += 1;
                if mine == theirs {
                    row.agreeing += 1;
                } else {
                    row.disagreements.push((name.to_owned(), theirs, mine));
                }
                seen.insert(name.to_owned(), ());
            }
            _ => {}
        }
    }
    for name in &names {
        if !seen.contains_key(name) {
            *row.stopped.entry("noOutcome").or_default() += 1;
        }
    }

    let render = |map: &BTreeMap<String, usize>| -> String {
        map.iter()
            .map(|(code, count)| format!("{code}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    println!("corpus={}", if with_scalar { "day7+day5" } else { "day7" });
    println!("programs={}", row.programs);
    println!("frontEndRefused={}", render(&row.front_end_refused));
    println!("loweringRefused={}", render(&row.lowering_refused));
    println!(
        "stoppedByComponent={}",
        row.stopped
            .iter()
            .map(|(component, count)| format!("{component}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    println!("compiled={}", row.compiled);
    println!("linked={}", row.linked);
    println!("ran={}", row.ran);
    println!("agreeing={}", row.agreeing);
    for (name, theirs, mine) in &row.disagreements {
        println!("disagreed={name} gcc={theirs} evocc={mine}");
    }

    let _ = std::fs::remove_dir_all(&work);
    if let Some((name, theirs, mine)) = row.disagreements.first() {
        panic!("{name}: GCC-14 said {theirs}, EvoCC said {mine}");
    }
    assert!(row.ran > 0, "no program reached the container");
    assert_eq!(row.agreeing, row.ran);
}

/// The two profiles differ in exactly the four places Section 8 names.
#[test]
fn the_two_profiles_differ_where_the_platforms_do() {
    let source = "\
global @counter origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = global_addr @counter : ptr<generic>
    %v3 = access_in_bounds %v0, %v2 [size=4, align=4]
    (%v4, %v5, %v6) = load %v3, %v1, %v2 : bits<32>
    ret %v4, %v5, %v6
}
";
    let module = parse_section9_module(source.as_bytes()).expect("canonical EvoIR");
    let darwin = compile_module_for(Target::Darwin, &module, &|_| true).expect("darwin");
    let linux = compile_module_for(Target::Linux, &module, &|_| true).expect("linux");

    // The symbol prefix.
    assert!(darwin.contains("\n.globl _main\n"), "{darwin}");
    assert!(linux.contains("\n.globl main\n"), "{linux}");
    // The page-relative pair. Default-visible ELF definitions remain
    // interposable under PIC; defining counter in this module does not make
    // the final executable/DSO binding local.
    assert!(darwin.contains("_counter@PAGE"), "{darwin}");
    assert!(darwin.contains("_counter@PAGEOFF"), "{darwin}");
    assert!(linux.contains("adrp x"), "{linux}");
    assert!(linux.contains(":got:counter"), "{linux}");
    assert!(linux.contains("#:got_lo12:counter"), "{linux}");
    assert!(!linux.contains("@PAGE"), "{linux}");
    let hidden = parse_section9_module(
        source
            .replace("visibility=external", "visibility=hidden")
            .as_bytes(),
    )
    .expect("hidden EvoIR");
    let hidden = compile_module_for(Target::Linux, &hidden, &|_| true).expect("hidden Linux");
    assert!(
        hidden
            .lines()
            .any(|line| line.trim_start().starts_with("adrp ") && line.ends_with(", counter")),
        "{hidden}"
    );
    assert!(!hidden.contains(":pg_hi21:"), "{hidden}");
    assert!(hidden.contains(":lo12:counter"), "{hidden}");
    assert!(!hidden.contains(":got:counter"), "{hidden}");
    // The section names.
    assert!(darwin.starts_with(".section __TEXT,__text"), "{darwin}");
    assert!(linux.starts_with(".text\n"), "{linux}");
    assert!(darwin.contains(".section __DATA,__data"), "{darwin}");
    assert!(linux.contains("\n.data\n"), "{linux}");
    // The local-label prefix.
    assert!(darwin.contains("\nLmain_block0:"), "{darwin}");
    assert!(linux.contains("\n.Lmain_block0:"), "{linux}");

    // ELF consumers need the subject's symbol extents, not a reference
    // compiler's different function length. Mach-O has no such directives.
    assert!(linux.contains(".type main, %function\n"), "{linux}");
    assert!(linux.contains(".size main, .-main\n"), "{linux}");
    assert!(linux.contains(".type counter, %object\n"), "{linux}");
    assert!(linux.contains(".size counter, .-counter\n"), "{linux}");
    assert!(!darwin.contains(".type "), "{darwin}");
    assert!(!darwin.contains(".size "), "{darwin}");

    println!("darwinBytes={}", darwin.len());
    println!("linuxBytes={}", linux.len());
}
