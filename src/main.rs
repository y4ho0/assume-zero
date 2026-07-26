use std::process::ExitCode;

fn main() -> ExitCode {
    match assume_zero::run_cli() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("AssumeZero could not complete.\n\nWhat happened:\n  {error:#}");
            eprintln!("\nOriginal project modified:\n  No source command was run in it.");
            eprintln!("\nNext:\n  Run `assumezero doctor`, then retry with `--verbose`.");
            ExitCode::from(4)
        }
    }
}
