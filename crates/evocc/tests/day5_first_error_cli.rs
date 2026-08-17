use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn unique_source_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock is after the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "evocc-day5-first-error-{}-{nonce}.c",
        std::process::id()
    ))
}

#[test]
fn parse_only_stops_after_exact_first_error_before_large_valid_tail() {
    let source_path = unique_source_path();
    let mut source = String::from("__extension__ typedef signed long long extension_type;\n");
    for index in 0..20_000 {
        source.push_str(&format!("typedef int tail_{index};\n"));
    }
    fs::write(&source_path, source).expect("write large parser input");

    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg("--parse-only")
        .arg(&source_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evocc");
    let deadline = Instant::now() + Duration::from_secs(5);
    let output = loop {
        match child.try_wait().expect("poll evocc") {
            Some(_) => break child.wait_with_output().expect("collect evocc output"),
            None if Instant::now() >= deadline => {
                child.kill().expect("kill evocc after watchdog");
                let output = child.wait_with_output().expect("collect timed-out output");
                let _ = fs::remove_file(&source_path);
                panic!(
                    "evocc parsed the large recovery tail past the five-second watchdog; stdout={:?} stderr={:?}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    };
    fs::remove_file(&source_path).expect("remove large parser input");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic is UTF-8"),
        format!(
            "{}:1:1: error[E1001]: expected-declaration\n",
            source_path.to_string_lossy()
        )
    );
}
