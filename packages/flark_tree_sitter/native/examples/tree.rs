use std::io::{self, BufRead};

fn main() {
    for line in io::stdin().lock().lines() {
        let v: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let language = match v["language"].as_u64().unwrap() {
            1 => tree_sitter_dart::LANGUAGE,
            2 => tree_sitter_javascript::LANGUAGE,
            3 => tree_sitter_python::LANGUAGE,
            4 => tree_sitter_yaml::LANGUAGE,
            _ => panic!("language"),
        };
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language.into()).unwrap();
        let source = v["source"].as_str().unwrap();
        let tree = parser.parse(source, None).unwrap();
        println!("{source:?}\n{}", tree.root_node().to_sexp());
        fn leaves(node: tree_sitter::Node, source: &str) {
            if node.child_count() == 0 {
                println!(
                    "  {} {}..{} {:?} missing={}",
                    node.kind(),
                    node.start_byte(),
                    node.end_byte(),
                    &source[node.byte_range()],
                    node.is_missing()
                );
            } else {
                for i in 0..node.child_count() {
                    leaves(node.child(i).unwrap(), source);
                }
            }
        }
        leaves(tree.root_node(), source);
    }
}
