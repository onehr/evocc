/// Finish the platform's first-launch assessment before timing compiler work.
pub fn warm_evocc() {
    #[cfg(target_os = "macos")]
    {
        static WARM: std::sync::Once = std::sync::Once::new();
        WARM.call_once(|| {
            // macOS assesses a freshly built executable on its first launch;
            // the watchdog measures the interpreter, not that assessment.
            let output = std::process::Command::new(env!("CARGO_BIN_EXE_evocc"))
                .args(["evoir-emit", "--probe"])
                .stdin(std::process::Stdio::null())
                .output()
                .expect("launch evocc before starting the watchdog");
            assert!(
                output.status.success(),
                "evocc warm-up failed: status={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr),
            );
        });
    }
}
