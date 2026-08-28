use flark_parser::m11_is_markdown_punctuation;

const DART_TABLES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../lib/src/markdown_scalar_tables.dart"
));

fn parse_range_bounds(name: &str) -> Vec<u32> {
    let marker = format!("static const List<int> {name} = <int>[");
    let body = DART_TABLES
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing generated Dart table {name}"))
        .1
        .split_once("];")
        .unwrap_or_else(|| panic!("unterminated generated Dart table {name}"))
        .0;
    let bounds = body
        .split(',')
        .filter_map(|value| {
            let value = value.trim();
            (!value.is_empty()).then(|| {
                u32::from_str_radix(
                    value
                        .strip_prefix("0x")
                        .unwrap_or_else(|| panic!("non-hex bound {value} in {name}")),
                    16,
                )
                .unwrap_or_else(|error| panic!("invalid bound {value} in {name}: {error}"))
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(bounds.len() % 2, 0, "{name} must contain range pairs");
    bounds
}

fn contains(bounds: &[u32], scalar: u32) -> bool {
    let mut low = 0;
    let mut high = bounds.len() / 2;
    while low < high {
        let middle = low + ((high - low) / 2);
        let start = bounds[middle * 2];
        let end = bounds[middle * 2 + 1];
        if scalar < start {
            high = middle;
        } else if scalar > end {
            low = middle + 1;
        } else {
            return true;
        }
    }
    false
}

#[test]
fn generated_dart_scalar_tables_match_parser_classifiers_exhaustively() {
    let whitespace = parse_range_bounds("_whitespaceRangeBounds");
    let punctuation = parse_range_bounds("_punctuationOrSymbolRangeBounds");

    for scalar in 0..=0x10_FFFF {
        let Some(character) = char::from_u32(scalar) else {
            continue;
        };
        assert_eq!(
            contains(&whitespace, scalar),
            character.is_whitespace(),
            "whitespace mismatch at U+{scalar:04X}",
        );
        assert_eq!(
            contains(&punctuation, scalar),
            m11_is_markdown_punctuation(character),
            "punctuation/symbol mismatch at U+{scalar:04X}",
        );
    }
}
