use super::fake::{self, FakeClient};
use super::{ConfigFile, InstanceState};
use pai_core::client::ClientError;

/// The manager `default` runs the server profile `big`, whose file holds
/// `Port = 8099`.
fn manager_with_big() -> (fake::Log, FakeClient) {
    let log = fake::log();
    let c = FakeClient::new(&log, InstanceState::Inactive)
        .with_instance("default", "big", 8080)
        .with_file("server@big.ini", "[Server]\nPort = 8099\n");
    (log, c)
}

/// The file names the fake was asked to read or write, in order: the
/// names the seam chose.
fn names_asked(log: &fake::Log) -> Vec<String> {
    log.lock()
        .unwrap()
        .iter()
        .filter_map(|e| {
            e.strip_prefix("read_file ")
                .or_else(|| e.strip_prefix("write_file "))
                .map(str::to_string)
        })
        .collect()
}

#[test]
fn the_server_read_is_the_profile_default_names() {
    let (log, c) = manager_with_big();
    let b = fake::backend(c);
    assert_eq!(
        b.read_config(ConfigFile::Server),
        Ok(Some("[Server]\nPort = 8099\n".to_string())),
        "the profile's file, not an empty read"
    );
    assert_eq!(
        names_asked(&log),
        ["server@big.ini"],
        "no call names server@default.ini"
    );
}

#[test]
fn the_server_save_is_the_profile_default_names() {
    let (log, c) = manager_with_big();
    let files = c.files.clone();
    let b = fake::backend(c);
    b.write_config(ConfigFile::Server, "[Server]\nPort = 8080\n")
        .expect("the write goes through");
    assert_eq!(
        fake::writes(&log),
        ["write_file server@big.ini"],
        "no call names server@default.ini"
    );
    assert_eq!(
        files
            .lock()
            .unwrap()
            .get("server@big.ini")
            .map(String::as_str),
        Some("[Server]\nPort = 8080\n")
    );
    assert!(
        !files.lock().unwrap().contains_key("server@default.ini"),
        "the file the instance does not use stays untouched"
    );
}

/// The path the user is shown is the file the seam reads.
#[test]
fn the_path_shown_is_the_profile_file() {
    let (_log, c) = manager_with_big();
    let b = fake::backend(c);
    let p = b.config_path(ConfigFile::Server);
    assert_eq!(
        p.file_name().and_then(std::ffi::OsStr::to_str),
        Some("server@big.ini"),
        "{p:?}"
    );
}

#[test]
fn an_instances_that_fails_reads_and_writes_nothing() {
    let (log, mut c) = manager_with_big();
    c.fail_instances = Some(ClientError::Unavailable);
    let c = c.with_file("server@default.ini", "[Server]\nPort = 8080\n");
    let files = c.files.clone();
    let b = fake::backend(c);
    assert_eq!(
        b.read_config(ConfigFile::Server),
        Err(ClientError::Unavailable.text()),
        "the manager's own error, no fall back"
    );
    assert_eq!(
        b.write_config(ConfigFile::Server, "[Server]\nPort = 8080\n"),
        Err(ClientError::Unavailable.text())
    );
    assert!(
        names_asked(&log).is_empty(),
        "the file is never asked: {:?}",
        names_asked(&log)
    );
    let files = files.lock().unwrap();
    assert_eq!(
        files.get("server@big.ini").map(String::as_str),
        Some("[Server]\nPort = 8099\n"),
        "nothing written"
    );
    assert_eq!(
        files.get("server@default.ini").map(String::as_str),
        Some("[Server]\nPort = 8080\n"),
        "never a fall back to the file of the id"
    );
}

/// `default` is not in the list: the same refusal, nothing read, nothing
/// written.
#[test]
fn an_absent_default_is_no_instance() {
    let (log, c) = manager_with_big();
    let c = c.with_instance("coder", "coder", 8081);
    let b = fake::backend(c);
    assert_eq!(
        b.read_config(ConfigFile::Server),
        Err(ClientError::NoInstance.text())
    );
    assert_eq!(
        b.write_config(ConfigFile::Server, "[Server]\nPort = 8080\n"),
        Err(ClientError::NoInstance.text())
    );
    assert!(
        names_asked(&log).is_empty(),
        "no file of any name is asked: {:?}",
        names_asked(&log)
    );
}

/// The model profiles files keep their names: the profile of an instance
/// says where the server file is, nothing else.
#[test]
fn the_model_profiles_files_keep_their_names() {
    let (log, c) = manager_with_big();
    let c = c.with_file("models.ini", "[Qwen3.8-27B]\nmodel = /m/q.gguf\n");
    let b = fake::backend(c);
    assert_eq!(
        b.read_config(ConfigFile::Models),
        Ok(Some("[Qwen3.8-27B]\nmodel = /m/q.gguf\n".to_string()))
    );
    assert_eq!(b.read_config(ConfigFile::ModelsDisabled), Ok(None));
    assert_eq!(names_asked(&log), ["models.ini", "models.ini.disabled"]);
}
