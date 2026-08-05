use entities::ENTITIES;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() {
    let mut map = phf_codegen::Map::new();
    let mut maximum_output_bytes = 0_usize;
    let mut maximum_encoded_bytes = 0_usize;
    let mut expansion_numerator = 1_usize;
    let mut expansion_denominator = 1_usize;

    for entity in ENTITIES
        .iter()
        .filter(|entity| entity.entity.starts_with('&') && entity.entity.ends_with(';'))
    {
        let name = &entity.entity[1..entity.entity.len() - 1];
        map.entry(name, format!("{:?}", entity.characters));
        maximum_output_bytes = maximum_output_bytes.max(entity.characters.len());
        maximum_encoded_bytes = maximum_encoded_bytes.max(entity.entity.len());
        if entity.characters.len() * expansion_denominator
            > expansion_numerator * entity.entity.len()
        {
            expansion_numerator = entity.characters.len();
            expansion_denominator = entity.entity.len();
        }
    }

    let mut generated = String::new();
    writeln!(
        generated,
        "pub static ENTITY_MAP: phf::Map<&'static str, &'static str> = {};",
        map.build()
    )
    .unwrap();
    writeln!(
        generated,
        "pub const MAX_NAMED_ENTITY_OUTPUT_BYTES: usize = {maximum_output_bytes};"
    )
    .unwrap();
    writeln!(
        generated,
        "pub const MAX_NAMED_ENTITY_SOURCE_BYTES: usize = {maximum_encoded_bytes};"
    )
    .unwrap();
    writeln!(
        generated,
        "pub const MAX_ENTITY_EXPANSION_NUMERATOR: usize = {expansion_numerator};"
    )
    .unwrap();
    writeln!(
        generated,
        "pub const MAX_ENTITY_EXPANSION_DENOMINATOR: usize = {expansion_denominator};"
    )
    .unwrap();

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    fs::write(output.join("entity_map.rs"), generated).expect("write generated entity map");
}
