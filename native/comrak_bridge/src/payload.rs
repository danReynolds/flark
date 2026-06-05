use serde::Serialize;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonRange {
    pub(crate) start_byte: u32,
    pub(crate) end_byte: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonBlock {
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    pub(crate) start_byte: u32,
    pub(crate) end_byte: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) payload: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonInlineToken {
    pub(crate) styles: Vec<&'static str>,
    pub(crate) start_byte: u32,
    pub(crate) end_byte: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) payload: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonReplacementRange {
    #[serde(rename = "type")]
    pub(crate) kind: &'static str,
    pub(crate) start_byte: u32,
    pub(crate) end_byte: u32,
    pub(crate) text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonDiagnostic {
    start_byte: u32,
    end_byte: u32,
    message: String,
    code: &'static str,
    is_error: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonParsePayload {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) blocks: Vec<JsonBlock>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) inline_tokens: Vec<JsonInlineToken>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) marker_ranges: Vec<JsonRange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) replacement_ranges: Vec<JsonReplacementRange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) exclusion_ranges: Vec<JsonRange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) diagnostics: Vec<JsonDiagnostic>,
}

pub(crate) fn diagnostic_payload(message: &str) -> Vec<u8> {
    let escaped = message
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!(
        "{{\"blocks\":[],\"inlineTokens\":[],\"markerRanges\":[],\"replacementRanges\":[],\"exclusionRanges\":[],\"diagnostics\":[{{\"startByte\":0,\"endByte\":0,\"message\":\"{}\",\"code\":\"COMRAK_BRIDGE_ERROR\",\"isError\":true}}]}}",
        escaped
    )
    .into_bytes()
}
