//! Small diagnostic bridge through the real validated native pipeline.
use std::io::{self, Read};

fn main() -> Result<(), String> {
    let mut input = Vec::new();
    io::stdin()
        .read_to_end(&mut input)
        .map_err(|e| e.to_string())?;
    let module = evocc_evoir::parse_section9_module(&input)?;
    let assembly = evocc_mir::compile_module_for(evocc_mir::Target::Linux, &module, &|_| true)
        .map_err(|e| e.detail())?;
    print!("{assembly}");
    Ok(())
}
