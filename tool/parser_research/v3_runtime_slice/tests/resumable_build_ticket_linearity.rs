use std::process::Command;

#[test]
fn resumable_build_ticket_cannot_be_duplicated_or_replayed() {
    let source = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/ui/resumable_build_ticket_cannot_be_duplicated_or_replayed.rs"
    );
    let output_path = std::env::temp_dir().join(format!(
        "flark_resumable_build_ticket_linearity_{}",
        std::process::id()
    ));
    let output = Command::new(option_env!("RUSTC").unwrap_or("rustc"))
        .args([
            "--edition=2024",
            "--crate-name",
            "resumable_build_ticket_cannot_be_duplicated_or_replayed",
            source,
            "-o",
        ])
        .arg(&output_path)
        .output()
        .expect("run rustc for the resumable-build compile-fail fixture");
    let _ = std::fs::remove_file(output_path);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "fixture unexpectedly compiled");
    assert!(
        stderr.matches("error[E0382]").count() >= 2,
        "expected move errors for duplication and replay, got:\n{stderr}"
    );
    assert!(stderr.contains("duplicated_ticket"), "{stderr}");
    assert!(stderr.contains("replayed_ticket"), "{stderr}");
}
