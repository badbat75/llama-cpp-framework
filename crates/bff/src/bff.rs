//! The `pai-bff` entry point.

use crate::listen::{self, ListenError};
use crate::serve::{self, ServeError};
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
    let service = args.first().is_some_and(|a| a == "service");
    if let Some(arg) = args.first() {
        if arg == "--version" && args.len() == 1 {
            let version = env!("CARGO_PKG_VERSION");
            println!("{}", strings::fill("bff-version", &[&version]));
            return Ok(0);
        }
        if !(service && args.len() == 1) {
            let known = arg == "--version" || service;
            let unknown = if known { &args[1] } else { arg };
            return Err((2, strings::fill("bff-unknown-argument", &[unknown])));
        }
    }
    #[cfg(not(windows))]
    if service {
        return Err((1, strings::t("bff-service-unavailable")));
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
    #[cfg(windows)]
    if service {
        use crate::serve::windows::{self, ServiceError};
        return match windows::run(addr) {
            Ok(()) => Ok(0),
            Err(ServiceError::NotAService) => Err((1, strings::t("bff-service-unavailable"))),
            Err(ServiceError::Dispatcher(e)) => Err((1, strings::fill("bff-serve-failed", &[&e]))),
        };
    }
    serve::serve(addr, serve::stop_signal()).map_err(|e| {
        let text = match e {
            ServeError::Bind(e) => strings::fill("bff-bind-failed", &[&addr, &e]),
            ServeError::Runtime(e) | ServeError::Serve(e) => {
                strings::fill("bff-serve-failed", &[&e])
            }
        };
        (1, text)
    })?;
    Ok(0)
}
