use std::process::Command;

#[test]
fn arena_owner_handle_cannot_be_duplicated_or_replayed() {
    let source = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/ui/arena_owner_handle_cannot_be_duplicated_or_replayed.rs"
    );
    let output_path = std::env::temp_dir().join(format!(
        "flark_arena_owner_handle_linearity_{}",
        std::process::id()
    ));
    let output = Command::new(option_env!("RUSTC").unwrap_or("rustc"))
        .args([
            "--edition=2024",
            "--crate-name",
            "arena_owner_handle_cannot_be_duplicated_or_replayed",
            source,
            "-o",
        ])
        .arg(&output_path)
        .output()
        .expect("run rustc for the transaction-owner compile-fail fixture");
    let _ = std::fs::remove_file(output_path);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "fixture unexpectedly compiled");
    assert!(
        stderr.matches("error[E0382]").count() >= 2,
        "expected move errors for duplication and replay, got:\n{stderr}"
    );
    assert!(stderr.contains("duplicated_handle"), "{stderr}");
    assert!(stderr.contains("released_handle"), "{stderr}");
}
