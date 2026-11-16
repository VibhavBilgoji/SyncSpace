//! Writes the TypeScript types of the API into `apps/web/src/types/generated/`.
//!
//! `cargo run -p server --bin gen-types`. The files are committed; CI regenerates them and fails
//! when anything changes (`docs/TESTING.md` section 3).

use std::process::ExitCode;

fn main() -> ExitCode {
    let dir = server::typegen::default_dir();
    match server::typegen::generate(&dir) {
        Ok(names) => {
            println!("wrote {} types to {}", names.len(), dir.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("could not generate the types: {error}");
            ExitCode::FAILURE
        }
    }
}
