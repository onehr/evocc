//! Packaged independent checker. This binary does not link the optimizer.
fn main() -> std::process::ExitCode {
    evocc_proofcheck::main()
}
