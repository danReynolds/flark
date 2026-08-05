use std::collections::BTreeSet;

use flark_comrak_value_block_core::provenance::embedded_manifest;

#[test]
fn manifest_pins_all_selected_functions_once() {
    let manifest = embedded_manifest();
    assert_eq!(manifest.schema_version, 1);
    assert_eq!(
        manifest.donor.commit,
        "172c2ee7d2c5c262a28be3e407aadf705daea2b7"
    );
    assert_eq!(manifest.functions.len(), 55);
    let keys = manifest
        .functions
        .iter()
        .map(|row| (&row.upstream_path, &row.upstream_name))
        .collect::<BTreeSet<_>>();
    assert_eq!(keys.len(), 55);
    assert!(manifest.functions.iter().all(|row| {
        row.upstream_sha256.len() == 64
            && !row.local_correspondent.is_empty()
            && row.status == "implemented"
    }));
}
