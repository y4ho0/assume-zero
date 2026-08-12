#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{self, Command};
use std::thread;
use std::time::Duration;

fn main() {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let Some(mode) = arguments.first().map(String::as_str) else {
        process::exit(64);
    };
    let success = match mode {
        "pass" | "child" => true,
        "fail" => false,
        "required-env" => arguments
            .get(1)
            .is_some_and(|name| env::var_os(name).is_some()),
        "hidden-path-tool" => arguments.get(1).is_some_and(|name| {
            Command::new(name)
                .arg("child")
                .status()
                .is_ok_and(|s| s.success())
        }),
        "home-config-dependent" => {
            home_directory().is_some_and(|home| home.join(".assumezero-fixture-config").is_file())
        }
        "cache-dependent" => arguments.get(1).is_some_and(|name| {
            env::var_os(name).is_some_and(|directory| {
                Path::new(&directory)
                    .join("assumezero-fixture-cache")
                    .is_file()
            })
        }),
        "fail-on-space" => {
            !env::current_dir().is_ok_and(|path| path.to_string_lossy().contains(' '))
        }
        "fail-on-unicode" => {
            !env::current_dir().is_ok_and(|path| !path.to_string_lossy().is_ascii())
        }
        "fail-on-deep" => !env::current_dir().is_ok_and(|path| path.to_string_lossy().len() >= 160),
        "fixed-temp-dependent" => arguments.get(1).is_some_and(|expected| {
            env::var_os("TMPDIR").is_some_and(|value| value == expected.as_str())
        }),
        "flaky-baseline" => arguments.get(1).is_some_and(|state| {
            let previous = fs::read_to_string(state)
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(0);
            let _ = fs::write(state, (previous + 1).to_string());
            previous % 2 == 0
        }),
        "timeout" => {
            thread::sleep(Duration::from_secs(30));
            true
        }
        "large-output" => {
            let chunk = vec![b'x'; 16_384];
            for _ in 0..128 {
                let _ = io::stdout().write_all(&chunk);
            }
            true
        }
        "secret-output" => arguments.get(1).is_some_and(|name| {
            if let Ok(value) = env::var(name) {
                println!("fixture-secret={value}");
                true
            } else {
                false
            }
        }),
        "echo-args" => {
            for argument in arguments.iter().skip(1) {
                println!("stdout-argument={argument}");
                eprintln!("stderr-argument={argument}");
            }
            true
        }
        "create-file" => arguments
            .get(1)
            .is_some_and(|path| fs::write(path, "created").is_ok()),
        "replace-cwd-with-symlink" => arguments
            .get(1)
            .is_some_and(|target| replace_cwd_with_symlink(Path::new(target))),
        _ => false,
    };
    process::exit(i32::from(!success));
}

fn replace_cwd_with_symlink(target: &Path) -> bool {
    #[cfg(unix)]
    {
        let Ok(current) = env::current_dir() else {
            return false;
        };
        let Some(parent) = current.parent() else {
            return false;
        };
        let moved = parent.join("moved-project");
        fs::rename(&current, &moved).is_ok() && std::os::unix::fs::symlink(target, &current).is_ok()
    }
    #[cfg(not(unix))]
    {
        let _ = target;
        false
    }
}

fn home_directory() -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    {
        env::var_os("USERPROFILE").map(Into::into)
    }
    #[cfg(not(windows))]
    {
        env::var_os("HOME").map(Into::into)
    }
}
