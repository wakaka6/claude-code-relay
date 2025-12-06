use std::path::Path;
use std::process::Command;

fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = Path::new(&manifest_dir).parent().unwrap().parent().unwrap();

    println!(
        "cargo::rerun-if-changed={}",
        workspace_root.join(".githooks").display()
    );

    if workspace_root.join(".git").exists() {
        let _ = Command::new("git")
            .current_dir(workspace_root)
            .args(["config", "core.hooksPath", ".githooks"])
            .status();
    }
}
