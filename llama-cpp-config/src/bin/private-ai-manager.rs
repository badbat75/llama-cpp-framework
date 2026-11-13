//! private-ai-manager: the runtime service binary. `run-instance <id>` runs
//! llama-server with instance `id`'s configuration (`manager.rs`).

fn main() {
    std::process::exit(llama_cpp_config::manager::main());
}
