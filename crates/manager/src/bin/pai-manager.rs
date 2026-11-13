//! pai-manager: the runtime service binary. `run-instance <id>` runs
//! llama-server with instance `id`'s configuration (`manager.rs`).

fn main() {
    std::process::exit(pai_manager::manager::main());
}
