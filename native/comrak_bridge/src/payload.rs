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
    pub(crate) payload: serde_json::Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsonInlineToken {
    pub(crate) styles: Vec<&'static str>,
    pub(crate) start_byte: u32,
    pub(crate) end_byte: u32,
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
    pub(crate) blocks: Vec<JsonBlock>,
    pub(crate) inline_tokens: Vec<JsonInlineToken>,
    pub(crate) marker_ranges: Vec<JsonRange>,
    pub(crate) exclusion_ranges: Vec<JsonRange>,
    pub(crate) diagnostics: Vec<JsonDiagnostic>,
}

pub(crate) fn diagnostic_payload(message: &str) -> Vec<u8> {
    let escaped = message
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!(
        "{{\"blocks\":[],\"inlineTokens\":[],\"markerRanges\":[],\"exclusionRanges\":[],\"diagnostics\":[{{\"startByte\":0,\"endByte\":0,\"message\":\"{}\",\"code\":\"COMRAK_BRIDGE_ERROR\",\"isError\":true}}]}}",
        escaped
    )
    .into_bytes()
}
