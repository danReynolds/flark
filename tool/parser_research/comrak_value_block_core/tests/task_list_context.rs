use flark_comrak_value_block_core::{SyntaxProfile, normalized_html, parse_document};
use flark_gate_a_harness::{SyntaxProfile as OracleProfile, oracle_html_for};

#[test]
fn task_list_context_is_first_item_paragraph_not_list_tightness() {
    let cases = [
        "- [x] tight\n- [ ] peer\n",
        "- [x] loose\n\n  continuation\n",
        "- plain\n\n- [x] loose peer\n",
        "- [x] first paragraph\n\n  [ ] second paragraph stays literal\n",
        "- > quoted first child\n\n  [x] later paragraph stays literal\n",
        "- outer\n\n  - [x] nested loose task\n\n    continuation\n",
    ];

    for source in cases {
        let document = parse_document(source, SyntaxProfile::Gfm).expect("candidate parse");
        let actual = normalized_html(&document).expect("candidate render");
        let expected = oracle_html_for(OracleProfile::FlarkGfm, source).expect("Comrak render");
        assert_eq!(actual, expected, "{source:?}");
    }
}
