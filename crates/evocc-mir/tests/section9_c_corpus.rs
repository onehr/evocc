//! Section 9, instrument three, over C sources rather than generated EvoIR.
//!
//! "The assembled program is run on the host and its output compared with the
//! reference compiler's build of the same C source ... the host toolchain for
//! Darwin, recorded by version."
//!
//! The path is the one a user takes: the driver binary compiles the C source to
//! EvoIR, this crate lowers, allocates, verifies and emits it, the host
//! assembles, links and runs it, and its exit status is compared with the
//! status of the host compiler's own build of the same source. Every program
//! the path declines is counted by the first diagnostic that declined it, so
//! the refusals are a ranked frontier rather than a number.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use evocc_evoir::parse_section9_module;
use evocc_mir::{Driver, NativeStop, Toolchain};
#[path = "support/frame_variants.rs"]
mod frame_variants;
use frame_variants::compile_module;

#[path = "support/object_capture.rs"]
mod object_capture;
use object_capture::{memory_case_ids, CaptureConfig, ObjectCapture};

/// One corpus of C sources, materialized by its own authority script.
struct Corpus {
    name: &'static str,
    script: &'static str,
    sources: Vec<PathBuf>,
    capture_ids: BTreeMap<PathBuf, String>,
}

/// The workspace root, found from this file's own location.
fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories below the workspace root")
        .to_path_buf()
}

/// A fresh directory nothing else in this process writes to.
fn scratch(name: &str, capture: Option<&ObjectCapture>) -> PathBuf {
    if let Some(capture) = capture {
        return capture.scratch_dir().join(name);
    }
    let directory =
        std::env::temp_dir().join(format!("evocc-c-corpus-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    directory
}

/// The Day 7 memory corpus: 128 defined programs, each an `int main(void)`.
fn day7(capture: Option<&ObjectCapture>) -> Result<Corpus, String> {
    let script = "tests/acceptance/day7-memory-corpus.py";
    let directory = scratch("day7", capture);
    let output = Command::new("python3")
        .arg(workspace().join(script))
        .arg("materialize-defined")
        .arg(&directory)
        .output()
        .map_err(|error| format!("python3 is absent: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{script} refused: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let mut sources: Vec<PathBuf> = std::fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|suffix| suffix == "c"))
        .collect();
    sources.sort();
    let capture_ids = if capture.is_some() {
        memory_case_ids(&directory)?
    } else {
        BTreeMap::new()
    };
    Ok(Corpus {
        name: "day7",
        script,
        sources,
        capture_ids,
    })
}

/// The Day 5 scalar corpus: 256 defined programs, emitted as JSON lines.
fn day5(capture: Option<&ObjectCapture>) -> Result<Corpus, String> {
    let script = "tests/acceptance/day5-scalar-interpreter.py";
    let output = Command::new("python3")
        .arg(workspace().join(script))
        .arg("emit-jsonl")
        .output()
        .map_err(|error| format!("python3 is absent: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{script} refused: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let directory = scratch("day5", capture);
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut sources = Vec::new();
    let mut capture_ids = BTreeMap::new();
    for line in text.lines() {
        let Some((id, source)) = json_id_and_source(line) else {
            continue;
        };
        // The authority emits its rejected cases beside its defined ones. A
        // program whose behaviour the standard leaves undefined has no exit
        // status to compare: the reference build reads whatever the stack held
        // and EvoCC's checked profile traps, and both are correct.
        if !id.starts_with("defined.") {
            continue;
        }
        let path = directory.join(format!("{id}.c"));
        std::fs::write(&path, source).map_err(|error| error.to_string())?;
        if capture.is_some() && capture_ids.insert(path.clone(), id).is_some() {
            return Err("duplicate Day 5 case ID".into());
        }
        sources.push(path);
    }
    sources.sort();
    Ok(Corpus {
        name: "day5",
        script,
        sources,
        capture_ids,
    })
}

/// The `id` and `source` of one JSON line, without a JSON dependency.
///
/// The authority writes both as plain strings whose only escapes are `\n` and
/// `\"`, which is what this undoes.
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

/// The exit status of the host compiler's own build of one source.
fn reference(toolchain: &Toolchain, source: &Path) -> Result<u8, String> {
    let directory = std::env::temp_dir().join(format!("evocc-reference-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let program = directory.join("reference");
    let built = Command::new("clang")
        .arg("-std=c11")
        .arg("-O0")
        .arg("-o")
        .arg(&program)
        .arg(source)
        .output()
        .map_err(|error| error.to_string())?;
    if !built.status.success() {
        return Err(String::from_utf8_lossy(&built.stderr).into_owned());
    }
    let run = Command::new(&program).output().map_err(|error| {
        let _ = toolchain.version();
        error.to_string()
    })?;
    let _ = std::fs::remove_file(&program);
    let code = run
        .status
        .code()
        .ok_or_else(|| "the reference build did not exit normally".to_owned())?;
    u8::try_from(code & 0xff).map_err(|error| error.to_string())
}

/// The first diagnostic in a driver's or a lowering's complaint.
///
/// A refusal is ranked by the code that stopped it, so the table names what the
/// next window has to reach rather than how many messages a run printed.
fn first_diagnostic(text: &str) -> String {
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if word.len() >= 5
            && (word.starts_with('E') || word.starts_with("W"))
            && word[1..].chars().all(|c| c.is_ascii_digit())
        {
            return word.to_owned();
        }
    }
    "unnamed".to_owned()
}

struct Row {
    corpus: &'static str,
    programs: usize,
    front_end_refused: BTreeMap<String, usize>,
    lowering_refused: BTreeMap<String, usize>,
    stopped: BTreeMap<&'static str, usize>,
    compiled: usize,
    ran: usize,
    agreeing: usize,
    disagreements: Vec<(String, u8, u8)>,
    executed_ids: Vec<String>,
    agreeing_ids: Vec<String>,
}

fn measure(
    corpus: &Corpus,
    driver: &Driver,
    toolchain: &Toolchain,
    optimize: bool,
    mut capture: Option<&mut ObjectCapture>,
) -> Row {
    let mut row = Row {
        corpus: corpus.name,
        programs: corpus.sources.len(),
        front_end_refused: BTreeMap::new(),
        lowering_refused: BTreeMap::new(),
        stopped: BTreeMap::new(),
        compiled: 0,
        ran: 0,
        agreeing: 0,
        disagreements: Vec::new(),
        executed_ids: Vec::new(),
        agreeing_ids: Vec::new(),
    };

    for source in &corpus.sources {
        let name = source
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let evoir = match driver.emit_evoir(source, optimize) {
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
        let text = match compile_module(&module) {
            Ok(text) => text,
            Err(stop) => {
                match &stop {
                    NativeStop::Lowering(refusals) => {
                        for code in refusals
                            .iter()
                            .map(|r| r.code)
                            .collect::<std::collections::BTreeSet<_>>()
                        {
                            *row.lowering_refused.entry(code.to_owned()).or_default() += 1;
                        }
                    }
                    other => *row.stopped.entry(other.component()).or_default() += 1,
                }
                continue;
            }
        };
        row.compiled += 1;
        let result = if let Some(capture) = capture.as_deref_mut() {
            let id = corpus.capture_ids.get(source).expect("a corpus case ID");
            let assembled_name = format!("corpus-{name}");
            let first_path = toolchain
                .assemble(&assembled_name, &text)
                .expect("first object");
            capture.remaining_bytes().expect("bounded first object");
            let first = std::fs::read(&first_path).expect("first object bytes");

            // Rebuild through the same frontend and backend, then use exactly
            // the same assembler input/output paths. Do not strip metadata.
            let second_ir = driver
                .emit_evoir(source, optimize)
                .expect("second frontend build");
            let second_module = parse_section9_module(second_ir.as_bytes()).expect("second EvoIR");
            let second_text = compile_module(&second_module).expect("second native compilation");
            let second_path = toolchain
                .assemble(&assembled_name, &second_text)
                .expect("second object");
            capture.remaining_bytes().expect("bounded second object");
            let second = std::fs::read(&second_path).expect("second object bytes");
            capture
                .observe_pair(id, &first, &second)
                .expect("identical object twins/baseline");
            capture.remaining_bytes().expect("bounded retained object");
            toolchain
                .link(&assembled_name, &second_path)
                .and_then(|program| toolchain.run(&program))
        } else {
            toolchain.exit_status(&format!("corpus-{name}"), &text)
        };
        let observed = match result {
            Ok(status) => status,
            Err(stop) => {
                *row.stopped.entry(stop.component()).or_default() += 1;
                continue;
            }
        };
        row.ran += 1;
        if capture.is_some() {
            row.executed_ids.push(corpus.capture_ids[source].clone());
        }
        match reference(toolchain, source) {
            Ok(expected) if expected == observed => {
                row.agreeing += 1;
                if capture.is_some() {
                    row.agreeing_ids.push(corpus.capture_ids[source].clone());
                }
            }
            Ok(expected) => row.disagreements.push((name, expected, observed)),
            Err(_) => *row.stopped.entry("reference").or_default() += 1,
        }
    }
    row
}

fn report(row: &Row) {
    let render = |map: &BTreeMap<String, usize>| -> String {
        map.iter()
            .map(|(code, count)| format!("{code}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    println!("corpus={}", row.corpus);
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
    println!("ran={}", row.ran);
    println!("agreeing={}", row.agreeing);
    for (name, expected, observed) in &row.disagreements {
        println!("disagreed={name} reference={expected} evocc={observed}");
    }
}

#[test]
fn the_c_corpora_reach_the_host_through_the_driver() {
    let mut capture = CaptureConfig::from_env()
        .expect("valid object capture configuration")
        .map(|config| config.start("darwin"))
        .transpose()
        .expect("start corpus object capture");
    let located = capture.as_ref().map_or_else(Toolchain::locate, |capture| {
        Toolchain::locate_in(&capture.scratch_dir().join("asm"))
    });
    let toolchain = match located {
        Ok(toolchain) => toolchain,
        Err(reason) => {
            assert!(capture.is_none(), "capture cannot skip toolchain: {reason}");
            println!("skipped={reason}");
            return;
        }
    };
    let driver = match Driver::locate() {
        Ok(driver) => driver,
        Err(reason) => {
            assert!(capture.is_none(), "capture cannot skip driver: {reason}");
            println!("skipped={reason}");
            return;
        }
    };
    println!("toolchain={}", toolchain.version());
    // This gate explicitly trusts the package companion from its workspace build.
    let checker = driver
        .binary()
        .parent()
        .unwrap()
        .join(format!("evocc-checker{}", std::env::consts::EXE_SUFFIX));
    let checker_digest = evocc_proof::sha256_hex(&std::fs::read(checker).unwrap());
    println!("checker={checker_digest}");
    let driver = driver.with_checker_digest(checker_digest);
    println!("driver={}", driver.binary().display());

    let mut total_ran = 0;
    let mut total_agreeing = 0;
    let mut disagreements = Vec::new();
    let mut expected_ids = Vec::new();
    let mut executed_ids = Vec::new();
    let mut agreeing_ids = Vec::new();
    for corpus in [day7(capture.as_ref()), day5(capture.as_ref())] {
        let corpus = match corpus {
            Ok(corpus) => corpus,
            Err(reason) => {
                assert!(capture.is_none(), "capture cannot skip corpus: {reason}");
                println!("corpusSkipped={reason}");
                continue;
            }
        };
        if capture.is_some() {
            let expected_count = if corpus.name == "day7" { 128 } else { 256 };
            assert_eq!(corpus.sources.len(), expected_count);
            assert_eq!(corpus.capture_ids.len(), expected_count);
            expected_ids.extend(corpus.capture_ids.values().cloned());
        }
        println!("corpusScript={}", corpus.script);
        let row = measure(&corpus, &driver, &toolchain, true, capture.as_mut());
        report(&row);
        assert!(
            row.programs > 0,
            "{} materialized no sources",
            corpus.script
        );
        total_ran += row.ran;
        total_agreeing += row.agreeing;
        executed_ids.extend(row.executed_ids);
        agreeing_ids.extend(row.agreeing_ids);
        disagreements.extend(row.disagreements);
    }

    println!("totalRan={total_ran}");
    println!("totalAgreeing={total_agreeing}");
    if let Some((name, expected, observed)) = disagreements.first() {
        panic!("{name}: the reference said {expected}, EvoCC said {observed}");
    }
    assert!(
        total_ran > 0,
        "no program of either corpus reached the host"
    );
    assert_eq!(total_agreeing, total_ran);
    if let Some(capture) = capture {
        capture
            .finish(&expected_ids, &executed_ids, &agreeing_ids)
            .expect("complete 384-case object and native comparison");
    }
}
