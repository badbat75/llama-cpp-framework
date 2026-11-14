//! The `pai-bff` entry point.

use crate::listen::{self, ListenError};
use pai_core::strings;

/// Runs the service, returning the exit code. A failure prints one error
/// line on stderr.
pub fn main() -> i32 {
    strings::set_reader(strings::Reader::Cli);
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err((code, message)) => {
            eprintln!("{}", strings::fill("error-line", &[&message]));
            code
        }
    }
}

/// What `main` does, its failure as an exit code and a message.
fn run(args: &[String]) -> Result<i32, (i32, String)> {
    if let Some(arg) = args.first() {
        if arg == "--version" && args.len() == 1 {
            let version = env!("CARGO_PKG_VERSION");
            println!("{}", strings::fill("bff-version", &[&version]));
            return Ok(0);
        }
        let unknown = if arg == "--version" { &args[1] } else { arg };
        return Err((2, strings::fill("bff-unknown-argument", &[unknown])));
    }
    let path = listen::config_path();
    let addr = listen::load(&path).map_err(|e| {
        let text = match e {
            ListenError::Invalid(value) => {
                strings::fill("bff-listen-invalid", &[&value, &path.display()])
            }
            ListenError::NotLoopback(addr) => {
                strings::fill("bff-listen-not-loopback", &[&addr, &path.display()])
            }
            ListenError::Unreadable(detail) => {
                strings::fill("bff-config-unreadable", &[&path.display(), &detail])
            }
        };
        (1, text)
    })?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
        .map_err(|e| (1, strings::fill("bff-serve-failed", &[&e])))?;
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| (1, strings::fill("bff-bind-failed", &[&addr, &e])))?;
        axum::serve(listener, crate::router())
            .await
            .map_err(|e| (1, strings::fill("bff-serve-failed", &[&e])))?;
        Ok(0)
    })
}
