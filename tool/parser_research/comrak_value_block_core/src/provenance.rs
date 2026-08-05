use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceManifest {
    pub schema_version: u32,
    pub donor: DonorPin,
    pub functions: Vec<FunctionProvenance>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DonorPin {
    pub repository: String,
    pub version: String,
    pub commit: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionProvenance {
    pub upstream_path: String,
    pub upstream_name: String,
    pub upstream_sha256: String,
    pub local_correspondent: String,
    pub status: String,
    pub note: String,
}

pub fn embedded_manifest() -> ProvenanceManifest {
    serde_json::from_str(include_str!(
        "../provenance/comrak_0_54_block_functions.json"
    ))
    .expect("checked-in provenance manifest must be valid")
}
