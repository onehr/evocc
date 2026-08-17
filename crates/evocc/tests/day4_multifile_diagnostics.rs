use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_TEMP_TREE: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "evocc-day4-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create isolated source tree");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.join(relative);
        fs::write(&path, contents).expect("write source fixture");
        path
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("remove isolated source tree");
    }
}

fn run_parse_only(input: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg("--parse-only")
        .arg(input)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evocc");

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait().expect("poll evocc") {
            Some(_) => return child.wait_with_output().expect("collect evocc output"),
            None if Instant::now() >= deadline => {
                child.kill().expect("kill timed-out evocc");
                let output = child.wait_with_output().expect("collect timed-out evocc");
                panic!(
                    "evocc exceeded the two-second watchdog; stdout={:?} stderr={:?}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

#[test]
fn valid_header_and_main_parse_silently() {
    let tree = TempTree::new("cli-valid-include");
    tree.write("model.h", "struct Model {\n    int field;\n};\n");
    let main = tree.write(
        "consumer.c",
        "#include \"model.h\"\nstruct Model current;\n",
    );

    let output = run_parse_only(&main);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
}

#[test]
fn malformed_header_diagnostic_names_header_path_and_exact_position() {
    let tree = TempTree::new("cli-broken-include");
    let header = tree.write("broken.h", "int okay;\nint broken = !;\n");
    let main = tree.write("consumer.c", "#include \"broken.h\"\nint after;\n");

    let output = run_parse_only(&main);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic is UTF-8"),
        format!(
            "{}:2:15: error[E1003]: expected-expression\n",
            header.to_string_lossy()
        )
    );
}
