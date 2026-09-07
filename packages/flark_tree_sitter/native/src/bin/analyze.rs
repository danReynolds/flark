use std::io::{self, BufRead};

fn main() {
    // A line-oriented diagnostic also used by the native/Wasm parity runner.
    for line in io::stdin().lock().lines() {
        let input: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let result = flark_tree_sitter::analyze(
            input["source"].as_str().unwrap(),
            input["language"].as_u64().unwrap() as u32,
        )
        .unwrap();
        println!("{}", serde_json::to_string(&result).unwrap());
    }
}
