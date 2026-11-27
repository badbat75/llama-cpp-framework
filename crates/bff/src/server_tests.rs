use super::*;

/// The CPU count the step is handed. The test picks it, so the bound a
/// refusal pins does not move with the machine running the test.
const CPUS: usize = 16;

/// A form body's revision: an opaque string this step never reads.
const REV: &str = "0123456789abcdef";

/// A config with a non-default value in every field, each of them a value
/// the body hands back unchanged (a base URL already stored without the
/// `/v1` suffix, an AllReduce mode already canonical), so the file
/// `render_for` gives for it is the file its body must render.
fn rich_cfg() -> ServerConfig {
    ServerConfig {
        port: Some(8580),
        hostname: Some("127.0.0.1".into()),
        threads: Some(8),
        threads_batch: Some(4),
        models_max: Some(2),
        models_memory_margin: Some(1024),
        // The GPUs of the profile: the route reads and saves the key, and
        // the page has no field for it yet. Unset here, as the shipped
        // profile: every GPU of the node, so the render carries no line.
        devices: None,
        models_dir: Some("/srv/models".into()),
        rocblas_use_hipblaslt: Some(false),
        // The GTT switch: the explicit on (unset is the same for the
        // launch), the state a "write it only when false" collapse would
        // swallow.
        rocm_host_memory_gtt: Some(true),
        cuda_allreduce: Some("nccl".into()),
        cuda_disable_graphs: true,
        attn_rot_disable: true,
        rocprof_attach: true,
        webui: Some(false),
        webui_mcp_proxy: Some(true),
        log_verbosity: Some(2),
        save_state_on_shutdown: Some(true),
        state_dir: Some("/srv/state".into()),
        opencode_base_url: Some("http://127.0.0.1:4096".into()),
        opencode_api_key: Some("sk-secret".into()),
        tags: vec!["edge".into(), "v2".into()],
    }
}

/// The body the form sends for `cfg`: every key of the read, with the JSON
/// type the read gives it (a number, a bool, `"on"`/`"off"`, a string,
/// `null` for an unset key), as `document` writes them.
fn body_of(cfg: &ServerConfig) -> Value {
    json!({
        "revision": REV,
        "values": {
            "Port": cfg.port,
            "Hostname": cfg.hostname,
            "Threads": cfg.threads,
            "ThreadsBatch": cfg.threads_batch,
            "ModelsMax": cfg.models_max,
            "ModelsMemoryMargin": cfg.models_memory_margin,
            "ModelsDir": cfg.models_dir,
            "RocblasUseHipblaslt": cfg
                .rocblas_use_hipblaslt
                .map(|on| if on { "on" } else { "off" }),
            "RocmHostMemoryGtt": cfg.rocm_host_memory_gtt,
            "CudaAllreduce": cfg.cuda_allreduce,
            "CudaDisableGraphs": cfg.cuda_disable_graphs,
            "AttnRotDisable": cfg.attn_rot_disable,
            "RocprofAttach": cfg.rocprof_attach,
            "Webui": cfg.webui,
            "WebuiMcpProxy": cfg.webui_mcp_proxy,
            "LogVerbosity": cfg.log_verbosity,
            "SaveStateOnShutdown": cfg.save_state_on_shutdown,
            "StateDir": cfg.state_dir,
            "OpencodeBaseUrl": cfg.opencode_base_url,
            "OpencodeApiKey": cfg.opencode_api_key,
            "Tags": cfg.tags,
        },
    })
}

/// The rich body, with the value of the one key `key` replaced by `value`:
/// every other key keeps the rich config's own value, so the answer can
/// only be about the key the test changed.
fn body_with(key: &str, value: Value) -> Value {
    let mut body = body_of(&rich_cfg());
    values_of(&mut body).insert(key.to_string(), value);
    body
}

/// The same, for a key of the body itself (`revision`, `values`).
fn body_top(key: &str, value: Value) -> Value {
    let mut body = body_of(&rich_cfg());
    body.as_object_mut()
        .expect("the body is an object")
        .insert(key.to_string(), value);
    body
}

fn values_of(body: &mut Value) -> &mut serde_json::Map<String, Value> {
    body["values"]
        .as_object_mut()
        .expect("the body's values are an object")
}

/// The file `body_to_file` renders, panicking on any other answer.
fn file(body: Value) -> String {
    match body_to_file(&body, CPUS) {
        BodyAnswer::File(text) => text,
        other => panic!("expected the rendered file, got {other:?}"),
    }
}

/// The 422 `body_to_file` answers, as `{key, args, field}`, panicking on any
/// other answer.
fn refused(body: Value) -> (String, Vec<String>, String) {
    match body_to_file(&body, CPUS) {
        BodyAnswer::Refused { key, args, field } => (key, args, field),
        other => panic!("expected 422, got {other:?}"),
    }
}

/// The 400 `body_to_file` answers for `why`, panicking on any other answer.
fn assert_malformed(body: Value, why: &str) {
    match body_to_file(&body, CPUS) {
        BodyAnswer::Malformed => {}
        other => panic!("expected 400 server-malformed for {why}, got {other:?}"),
    }
}

/// The form's whole body, well typed, is the file `render_for(cfg, true)`
/// renders for the same config: every value lands in the config, and the
/// text is the renderer's, never the body's.
#[test]
fn body_to_file_renders_the_file_render_for_gives() {
    let cfg = rich_cfg();
    let got = file(body_of(&cfg));
    assert_eq!(
        got,
        server_cfg::render_for(&cfg, true),
        "a well-typed body renders exactly render_for(cfg, true)"
    );
}

/// A `Port` of a type the read never gives (a JSON string, not a number) is
/// a body the bff cannot read: 400, nothing rendered, the manager not asked.
#[test]
fn body_to_file_refuses_a_string_port_as_malformed() {
    assert_malformed(body_with("Port", json!("8580")), "a string Port");
}

/// Every body shape the bff cannot read is 400: the document it expects is
/// `{revision: string, values: object}`, with every value the JSON type the
/// read gives it.
#[test]
fn body_to_file_refuses_every_body_shape_it_cannot_read() {
    assert_malformed(json!([]), "a body that is not an object");
    assert_malformed(json!("values"), "a body that is a string");
    assert_malformed(json!(null), "a body that is null");
    let mut no_revision = body_of(&rich_cfg());
    no_revision
        .as_object_mut()
        .expect("the body")
        .remove("revision");
    assert_malformed(no_revision, "a body with no revision");
    assert_malformed(
        body_top("revision", json!(7)),
        "a revision that is not a string",
    );
    assert_malformed(body_top("revision", Value::Null), "a null revision");
    let mut no_values = body_of(&rich_cfg());
    no_values
        .as_object_mut()
        .expect("the body")
        .remove("values");
    assert_malformed(no_values, "a body with no values");
    assert_malformed(
        body_top("values", json!([])),
        "values that is not an object",
    );
    assert_malformed(body_top("values", json!("x")), "values that is a string");
    // A value of the wrong JSON type, one for each type the read gives.
    assert_malformed(
        body_with("Hostname", json!(true)),
        "a bool where a string is read",
    );
    assert_malformed(
        body_with("ModelsDir", json!(4)),
        "a number where a string is read",
    );
    assert_malformed(
        body_with("Port", json!(true)),
        "a bool where a number is read",
    );
    assert_malformed(
        body_with("Threads", json!(4.5)),
        "a fraction where a whole number is read",
    );
    assert_malformed(
        body_with("Webui", json!("on")),
        "a string where a bool is read",
    );
    assert_malformed(
        body_with("SaveStateOnShutdown", json!(1)),
        "a number where a bool is read",
    );
    assert_malformed(
        body_with("RocblasUseHipblaslt", json!(true)),
        "a bool where the tri-state's on/off string is read",
    );
}

/// `Threads` above this machine's CPU count is a check the bff runs itself
/// (the manager cannot): 422 with the check's key and args, and `field` the
/// key it refuses. The bound is the `cpus` argument, not the test's machine.
#[test]
fn body_to_file_refuses_threads_above_the_cpu_count() {
    let over = (CPUS + 1) as i64;
    let (key, args, field) = refused(body_with("Threads", json!(over)));
    assert_eq!(key, "not-whole-number-in-range");
    assert_eq!(field, "Threads", "the 422 marks the control it refuses");
    let want = vec![
        "Threads".to_string(),
        over.to_string(),
        "1".to_string(),
        CPUS.to_string(),
    ];
    assert_eq!(args, want, "the 422 carries the check's own args");
    let (_, _, field) = refused(body_with("ThreadsBatch", json!(over)));
    assert_eq!(field, "ThreadsBatch", "the same rule on the batch count");
    let text = file(body_with("Threads", json!(CPUS as i64)));
    assert!(
        text.contains("Threads = 16"),
        "Threads AT the CPU count is written: {text}"
    );
}

/// A `;` in `ModelsDir`, or a `#` in any of the free-text keys (the INI
/// format has no escaping): 422 `comment-markers`, `field` the key the
/// refusal names, which is its `args[0]`.
#[test]
fn body_to_file_refuses_comment_markers_in_a_text_value() {
    let (key, args, field) = refused(body_with("ModelsDir", json!("/srv/mod;els")));
    assert_eq!(key, "comment-markers");
    assert_eq!(
        field, "ModelsDir",
        "field is args[0], the key the refusal names"
    );
    assert_eq!(args, vec!["ModelsDir".to_string()]);
    let (key, args, field) = refused(body_with("StateDir", json!("/srv/st#ate")));
    assert_eq!(key, "comment-markers");
    assert_eq!(field, "StateDir");
    assert_eq!(args, vec!["StateDir".to_string()]);
    let (key, _, field) = refused(body_with("OpencodeApiKey", json!("sk-;secret")));
    assert_eq!(key, "comment-markers");
    assert_eq!(field, "OpencodeApiKey", "the same mapping on the API key");
    // The GPUs list is free text too: its list rule never sees a marker
    // (`ROCm0;x` is one entry of the shape rule), and the file would reload
    // it as `ROCm0`.
    for value in ["ROCm0;x", "ROCm0#y"] {
        let (key, args, field) = refused(body_with("Devices", json!(value)));
        assert_eq!(key, "comment-markers", "Devices = {value:?}");
        assert_eq!(args, vec!["Devices".to_string()], "Devices = {value:?}");
        assert_eq!(field, "Devices", "the 422 marks the GPUs control");
    }
}

/// The profile's tags obey the manager's rule (`checks::tags_rule`) on the
/// web save too: a list the rule refuses is 422 with the rule's key and
/// args, and `field` is the form's control, `tags` (lowercase, the one key
/// of the list, as the decision of 2026-11-20 names it). The save writes
/// nothing (the 422 answers before the manager is asked).
#[test]
fn body_to_file_refuses_a_tags_list_the_rule_refuses() {
    let want = |reason: &str, list: &str| {
        [
            reason.to_string(),
            list.to_string(),
            checks::TAGS_MAX.to_string(),
            checks::TAG_MIN.to_string(),
            checks::TAG_MAX.to_string(),
        ]
        .to_vec()
    };
    for (tags, reason, list) in [
        (json!(["Edge"]), "bad characters", "Edge".to_string()),
        (
            json!(["gpu", "gpu"]),
            "the same tag twice",
            "gpu,gpu".to_string(),
        ),
        (
            json!(["gpu edge"]),
            "bad characters",
            "gpu edge".to_string(),
        ),
        (json!([""]), "empty tag", String::new()),
    ] {
        let (key, args, field) = refused(body_with("Tags", tags));
        assert_eq!(key, "server-bad-tags", "the rule's one key");
        assert_eq!(args, want(reason, &list), "the rule's own args");
        assert_eq!(field, "tags", "the 422 marks the tags control");
    }
}

/// A tags list of the rule's shape is written, in the body's order; an
/// empty list is no tags and the file carries no `Tags` key.
#[test]
fn body_to_file_writes_a_tags_list_of_the_rules_shape() {
    let text = file(body_with("Tags", json!(["edge", "v2"])));
    assert!(text.contains("Tags = edge,v2"), "{text}");
    let text = file(body_with("Tags", json!([])));
    assert!(!text.contains("Tags ="), "no tags writes no key: {text}");
}

/// A tags list holding something that is not a string is a body the bff
/// cannot read: 400 `server-malformed`, the same answer as a value of the
/// wrong type of any other key.
#[test]
fn body_to_file_refuses_a_tags_list_that_is_not_strings() {
    assert_malformed(
        body_with("Tags", json!(["gpu", 7])),
        "a number among the tags",
    );
    assert_malformed(
        body_with("Tags", json!({"a": "gpu"})),
        "an object where a list is read",
    );
    assert_malformed(body_with("Tags", json!("gpu")), "a bare string");
    assert_malformed(body_with("Tags", Value::Null), "a null tags list");
}

/// The base URL is stored as the GUI stores it: a trailing `/` and a
/// trailing `/v1` are dropped, so a typed `/v1` is never appended twice.
#[test]
fn body_to_file_stores_the_base_url_without_its_v1_suffix() {
    let want = server_cfg::render_for(&rich_cfg(), true);
    for suffix in ["/v1", "/v1/", "/"] {
        let url = format!("http://127.0.0.1:4096{suffix}");
        let got = file(body_with("OpencodeBaseUrl", json!(url)));
        assert_eq!(got, want, "a base URL ending {suffix} is stored without it");
    }
}

/// A blank `ModelsDir` or `StateDir` is left unset: the file carries the
/// commented hint, never a folder of the bff's own account (an instance
/// whose file leaves them unset uses the engine's folders).
#[test]
fn body_to_file_leaves_a_blank_directory_unset() {
    let mut cfg = rich_cfg();
    cfg.models_dir = None;
    cfg.state_dir = None;
    let mut body = body_with("ModelsDir", json!("   "));
    values_of(&mut body).insert("StateDir".to_string(), json!(""));
    let got = file(body);
    assert_eq!(
        got,
        server_cfg::render_for(&cfg, true),
        "a blank ModelsDir or StateDir is left unset"
    );
    for key in ["ModelsDir", "StateDir"] {
        let named: Vec<&str> = got.lines().filter(|l| l.starts_with(key)).collect();
        assert!(
            named.is_empty(),
            "{key} stays out of the file, its hint is a comment: {named:?}"
        );
    }
}

/// `OpencodeApiKey: null` writes no key (the line stays a commented hint, so
/// the file reloads with no key); a string writes that key.
#[test]
fn body_to_file_writes_the_api_key_only_when_the_body_carries_one() {
    let got = file(body_with("OpencodeApiKey", Value::Null));
    let lines: Vec<&str> = got
        .lines()
        .filter(|l| l.starts_with("OpencodeApiKey"))
        .collect();
    assert!(lines.is_empty(), "a null key writes no key line: {lines:?}");
    assert!(
        got.lines().any(|l| l.starts_with("; OpencodeApiKey")),
        "the key stays commented, so it reloads unset"
    );
    let set = file(body_of(&rich_cfg()));
    assert!(
        set.lines().any(|l| l == "OpencodeApiKey = sk-secret"),
        "a string sets the key"
    );
}

/// A key the body does not carry at all is read as the body's `null` for it: the setting
/// is left unset, its line rendered as the commented hint. The form sends every key of the
/// read, so this is the shape of a body written by hand: it must neither keep a setting by
/// accident, nor clear one silently. Pinned as the code stands today: a `read_setting` that
/// reads a missing key as "leave the file alone", which is the design's intent
/// (ARCHITECTURE.md, the saving step), has to show up here as a deliberate change.
#[test]
fn body_to_file_treats_a_key_absent_from_the_body_as_unset() {
    let mut missing = body_of(&rich_cfg());
    values_of(&mut missing).remove("Port");
    let got = file(missing);
    assert_eq!(
        got,
        file(body_with("Port", Value::Null)),
        "a key the body does not carry renders as the body's null for it"
    );
    assert!(
        !got.lines().any(|l| l.starts_with("Port")),
        "a Port the body does not carry is not set: {got}"
    );
    assert!(
        got.lines().any(|l| l.starts_with("; Port")),
        "an unset Port renders the commented hint, so the file reloads at the default"
    );

    let mut missing = body_of(&rich_cfg());
    values_of(&mut missing).remove("OpencodeApiKey");
    let got = file(missing);
    assert!(
        !got.lines().any(|l| l.starts_with("OpencodeApiKey")),
        "an OpencodeApiKey the body does not carry writes no key: {got}"
    );
    assert!(
        got.lines().any(|l| l.starts_with("; OpencodeApiKey")),
        "an absent key leaves the hint commented, so the file reloads with no key"
    );
}

/// `0.0.0.0` bound with another address is the bff's check (the manager runs
/// it again): 422, with `field` the design maps this key to, `Hostname`.
#[test]
fn body_to_file_refuses_a_bind_all_with_another_address() {
    let (key, args, field) = refused(body_with("Hostname", json!("0.0.0.0,127.0.0.1")));
    assert_eq!(key, "bind-all-with-others");
    assert_eq!(args, Vec::<String>::new(), "this refusal carries no args");
    assert_eq!(field, "Hostname", "the design maps this key to Hostname");
}

/// An integer out of its range is 422 with `field` the key the refusal
/// names, as on `Threads`: the mapping is `args[0]`, not one field per key.
#[test]
fn body_to_file_refuses_an_integer_out_of_range_on_its_own_field() {
    for (key, value) in [
        ("Port", json!(70000)),
        ("ModelsMemoryMargin", json!(-1)),
        ("LogVerbosity", json!(6)),
    ] {
        let (refusal_key, args, field) = refused(body_with(key, value));
        assert_eq!(
            refusal_key, "not-whole-number-in-range",
            "{key} out of range"
        );
        assert_eq!(field, key, "field is args[0] of the refusal, naming {key}");
        assert_eq!(args.first().map(String::as_str), Some(key));
    }
}

/// The bounds of the thread check are the CPU count the step is handed,
/// not one it computes: fed `CPUS`, it refuses `CPUS + 1` and names `CPUS`
/// as the bound. Which count the route hands it (this machine's) is pinned
/// by `a_value_the_bff_check_refuses_is_422_naming_its_field` in
/// `tests/server.rs`, through the `limits` of the read the form saved.
#[test]
fn the_thread_bound_of_the_check_is_the_cpu_count_it_is_handed() {
    let mut cfg = rich_cfg();
    cfg.threads = Some(CPUS as i32 + 1);
    let (key, args, field) = refused(body_of(&cfg));
    assert_eq!(key, "not-whole-number-in-range");
    assert_eq!(
        args.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["Threads", &(CPUS + 1).to_string(), "1", &CPUS.to_string()],
        "the refusal's bounds are the check's own"
    );
    assert_eq!(field, "Threads");
}

/// The shapes that cannot name a setting, each with the body's own
/// `revision` so the refusal is about the shape and not about a missing
/// member: no `values`, a `values` that is a string, a `values` that is an
/// array, and a field value that is an object. Every one is malformed, and
/// the route refuses them before any manager call.
#[test]
fn body_to_file_refuses_a_body_that_cannot_name_a_setting() {
    assert_malformed(json!({ "revision": REV }), "no values member");
    assert_malformed(
        json!({ "revision": REV, "values": "Threads = 4" }),
        "values not an object",
    );
    assert_malformed(
        json!({ "revision": REV, "values": [{ "Threads": 4 }] }),
        "values an array",
    );
    assert_malformed(
        json!({ "revision": REV, "values": { "Threads": { "nested": 4 } } }),
        "a value that is an object",
    );
    assert_malformed(
        json!({ "revision": 4, "values": { "Threads": 4 } }),
        "a revision that is no string",
    );
}

/// A key the bff has no rule for is not a setting it writes: the file is
/// rendered from the known fields alone, so an extra key, of any name and
/// any type, leaves the file byte for byte what the same body without it
/// renders. The manager only ever sees the rendered text.
#[test]
fn body_to_file_writes_nothing_for_a_key_that_is_not_a_known_setting() {
    let plain = body_of(&rich_cfg());
    let want = file(plain.clone());
    for (key, value) in [
        ("Threads of the future", json!(4)),
        ("", json!(4)),
        ("  Threads", json!(4)),
        ("threads", json!(4)),
        ("ModelDir", json!({ "nested": 4 })),
    ] {
        let mut body = plain.clone();
        values_of(&mut body).insert(key.to_string(), value);
        assert_eq!(file(body), want, "the key {key:?} writes nothing");
    }
}

/// A number that is not the whole number the read gives is malformed, and
/// so is a float that is a whole number: the read sends an integer for a
/// numeric key, never a float, and the body sends back what it read. The
/// whole number itself renders.
#[test]
fn body_to_file_refuses_a_number_that_is_not_whole_for_an_integer_setting() {
    assert_malformed(body_with("Threads", json!(1.5)), "a fraction");
    assert_malformed(body_with("Threads", json!(4.0)), "a whole float");
    let text = file(body_with("Threads", json!(4)));
    assert!(text.contains("Threads = 4"), "{text}");
}

/// `null` clears a setting, and the blank and the all-blank string clear
/// it the same way: no line of the file starts with the key (the hint of
/// an unset one is a comment, and so starts with `;`).
#[test]
fn body_to_file_takes_the_blank_string_as_a_clear() {
    for blank in [Value::Null, json!(""), json!("   ")] {
        let text = file(body_with("ModelsDir", blank.clone()));
        assert!(
            !text.lines().any(|l| l.starts_with("ModelsDir")),
            "the blank {blank} leaves ModelsDir out of the file: {text}"
        );
    }
}

/// Every field name the web's body uses is a setting the bff knows, and
/// all of them are there: this is what the route hands the manager for the
/// manager's own check, and what `field_of` reads back, so a rename on one
/// side is a break on both. The names are compared as a set: the order of
/// a JSON object here is the bff's map's (the read's `values` and the
/// body's are both sorted by it) and the form reads by name. Which names
/// the read sends is pinned on the read side by
/// `unset_keys_are_null_in_the_shipped_file` in `tests/server.rs`.
#[test]
fn the_values_keys_are_the_web_s_field_names() {
    let body = body_of(&rich_cfg());
    let values = body["values"]
        .as_object()
        .expect("the body carries an object");
    let mut keys: Vec<&str> = values.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(
        keys,
        vec![
            "AttnRotDisable",
            "CudaAllreduce",
            "CudaDisableGraphs",
            "Hostname",
            "LogVerbosity",
            "ModelsDir",
            "ModelsMax",
            "ModelsMemoryMargin",
            "OpencodeApiKey",
            "OpencodeBaseUrl",
            "Port",
            "RocblasUseHipblaslt",
            "RocmHostMemoryGtt",
            "RocprofAttach",
            "SaveStateOnShutdown",
            "StateDir",
            "Tags",
            "Threads",
            "ThreadsBatch",
            "Webui",
            "WebuiMcpProxy",
        ]
    );
}
