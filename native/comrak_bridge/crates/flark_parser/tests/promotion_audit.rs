use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

#[derive(Clone, Copy)]
struct ModuleSnapshot {
    path: &'static str,
    lines: usize,
    sha256: &'static str,
}

#[derive(Clone, Copy)]
struct FunctionSnapshot {
    path: &'static str,
    start_line: usize,
    end_line: usize,
    sha256: &'static str,
}

const RESEARCH_CONTROLLER_CLOSURE: &[ModuleSnapshot] = &[
    ModuleSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/parser.rs",
        lines: 8_955,
        sha256: "51cd7a4ed2d53304c1724fb5133317cca384abe46dadff427b010fe880cb240e",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/reference_prefix.rs",
        lines: 2_118,
        sha256: "2116bc7416162cad4b4c7e37505e054befbe6ac606aa10495f32ea54d74f28a8",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/tree.rs",
        lines: 647,
        sha256: "732717e293deea0792743ed9eab7f55e4557dd27aaa1cbdcebab2be74e877cad",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/source.rs",
        lines: 543,
        sha256: "97cf8ceaee602aba319f6f03e5622da52b8d395eab87ae48a0c3555d6dec5737",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/table.rs",
        lines: 345,
        sha256: "7d5e5052a4a968bb7ca5f61da41dcf90aa58d70b1c098b861e94464d5789f10c",
    },
];

const SOURCE_SEAM_FUNCTIONS: &[FunctionSnapshot] = &[
    FunctionSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/parser.rs",
        start_line: 473,
        end_line: 490,
        sha256: "73aeb1b5b33711afd6001da565f7297ed4426226de387d9f3185066cd17398ea",
    },
    FunctionSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/parser.rs",
        start_line: 1_566,
        end_line: 1_624,
        sha256: "3a1897a0721ef4c717ed2ef5ee3e7acbb1d91fdee33bc77c7714f967db52af73",
    },
    FunctionSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/parser.rs",
        start_line: 1_626,
        end_line: 1_741,
        sha256: "b354aaad7daec608ddd8d64b4eafde589ddc162ec80b65883f9fbf0d75ffadf4",
    },
    FunctionSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/parser.rs",
        start_line: 1_929,
        end_line: 2_013,
        sha256: "03d8d8484dc1c3dc8ed0ee049bced56785748ca7b1666decfa761e16d3f6a9dd",
    },
    FunctionSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/parser.rs",
        start_line: 2_015,
        end_line: 2_088,
        sha256: "51a83db37ce2c32cd87c15d7122b2a25be4220055adf0b3ff988ee203002b510",
    },
    FunctionSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/parser.rs",
        start_line: 2_090,
        end_line: 2_160,
        sha256: "066d160d269f94da23d0a04b08b5e1279b5e3ff7f1fd3712f5f26aa07efeee38",
    },
    FunctionSnapshot {
        path: "tool/parser_research/comrak_value_block_core/src/parser.rs",
        start_line: 2_163,
        end_line: 2_171,
        sha256: "536a5f987110178ad9b765ac34cd0cf0173a28dfda7507e1cd6633cd323054e7",
    },
];

const VENDORED_DONOR_FUNCTIONS: &[FunctionSnapshot] = &[
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/parser/mod.rs",
        start_line: 1_442,
        end_line: 1_477,
        sha256: "b39e1d65357f6cbfe953baf9cb769ebff2158f08fe1eaa60bc2fdcc4cfe4a675",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/parser/mod.rs",
        start_line: 2_767,
        end_line: 2_882,
        sha256: "25ea7a12c9bb73b50116acd4437c50e5dff78a8a21d2fafaf0075a95bc9e852b",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/parser/block_spine_facade.rs",
        start_line: 53,
        end_line: 69,
        sha256: "faab22f00db25ea0be33e7631606a8e5c3aa74a81aa095eabd1beba6d988a14d",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/parser/block_spine_facade.rs",
        start_line: 80,
        end_line: 140,
        sha256: "d03c68594b0aa798fbe29be69c12af58239fe53bac4ade348671ee8ae4f62728",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/parser/inlines.rs",
        start_line: 2_518,
        end_line: 2_547,
        sha256: "602f1fddd1d583de3673c9c15a5859c8f5a8ae9a8c2770cbbb884aa3f5a558ba",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/parser/inlines.rs",
        start_line: 2_549,
        end_line: 2_585,
        sha256: "64199398c8aae89be47363b91c8d45c1f149a3c418de36f2449bd55eaec05ae5",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/parser/block_spine_facade.rs",
        start_line: 249,
        end_line: 260,
        sha256: "65278eaf43e53537a8a7297b6da6e73babf2f2c823cd670e877f172363576c4c",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/parser/block_spine_facade.rs",
        start_line: 263,
        end_line: 319,
        sha256: "27e1ab06fed594827581fbded0cbb400118575e800054c2b4a685a42764cd5cd",
    },
];

const SEGMENTED_LEXICAL_DONOR_MODULES: &[ModuleSnapshot] = &[ModuleSnapshot {
    path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/scanners.re",
    lines: 769,
    sha256: "91507602a7634291b9d980347beee58056e0d717af0ddb0e9d2a23a214c3b7ce",
}];

const GENERATED_ATX_CLOSURE: &[ModuleSnapshot] = &[
    ModuleSnapshot {
        path: "tool/parser_research/generated_scanner_gate/src/atx_cursor_generated.rs",
        lines: 396,
        sha256: "54bfedd5e449aadfdabeade40499dbf90b4bae2822fb7930b1a9f35dfea07efe",
    },
    ModuleSnapshot {
        path: "tool/parser_research/generated_scanner_gate/src/atx_fused_cursor.rs",
        lines: 934,
        sha256: "2707a8b946898e6c6f67746729c5e863d5451680cbec48a6b9a3f8724d54008a",
    },
    ModuleSnapshot {
        path: "tool/parser_research/generated_scanner_gate/src/atx_tail_cursor.rs",
        lines: 403,
        sha256: "6d16de94137be93fa8b9c69dbf1e44f4650a278f28a5bc22a18824a64b05f92e",
    },
];

const REFERENCE_LABEL_CLOSURE: &[ModuleSnapshot] = &[ModuleSnapshot {
    path: "tool/parser_research/reference_label_service/src/lib.rs",
    lines: 253,
    sha256: "20be2f68193ac89dde9008f9f844cc7ff7716c3763c62f7837bdd25e84efa539",
}];

const REFERENCE_VALUE_SERVICE_CLOSURE: &[ModuleSnapshot] = &[ModuleSnapshot {
    path: "tool/parser_research/reference_value_service/src/lib.rs",
    lines: 726,
    sha256: "9aff94e22e9fd46f6811ff26cb348bd9b093ed6320dbbf435408e95bb8b46273",
}];

const REFERENCE_VALUE_DONOR_MODULES: &[ModuleSnapshot] = &[ModuleSnapshot {
    path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/entity.rs",
    lines: 112,
    sha256: "e5319d83e3d0492c1dab0bdc1fe51a02110eb01669f75ff1caf62748947d8af0",
}];

const CUSTOM_COMRAK_CALLABLE_CLOSURE: &[ModuleSnapshot] = &[
    ModuleSnapshot {
        path: "tool/parser_research/comrak_inline_fragment_gate/vendor/comrak/src/parser/block_spine_facade.rs",
        lines: 455,
        sha256: "211131334adcdc4608326f5d8fbbc08ca6ac59f009669dc34668cfba55fbf315",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_inline_fragment_gate/vendor/comrak/src/scanners.rs",
        lines: 19_174,
        sha256: "5e6b6419d1568f5a6de8df6cd38cb4b6f6bfac0513e8549586c766e8c5dc55f0",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_inline_fragment_gate/vendor/comrak/src/parser/inlines.rs",
        lines: 2_912,
        sha256: "2f419765662e42dcba97677a9f7114abf6b9f527820060fd9cdac38348564997",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_inline_fragment_gate/vendor/comrak/src/strings.rs",
        lines: 625,
        sha256: "432723bf6b7c0d07297f1d3367a5ee3190f2cbd4335f639b91a7fc3aacc7ece3",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_inline_fragment_gate/vendor/comrak/src/parser/table.rs",
        lines: 392,
        sha256: "bb40cb77534e0d1edb1c7e22d7ba0e8206ec061043f7b43d8fdcb28625e7849b",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_inline_fragment_gate/vendor/comrak/src/nodes.rs",
        lines: 1_236,
        sha256: "71fff15a1a5516862052acad4e72854220c514c075f90048c5e4a0e9d58a4a3f",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_inline_fragment_gate/vendor/comrak/src/entity.rs",
        lines: 112,
        sha256: "e5319d83e3d0492c1dab0bdc1fe51a02110eb01669f75ff1caf62748947d8af0",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_inline_fragment_gate/vendor/comrak/src/ctype.rs",
        lines: 48,
        sha256: "79802a1e41dbca4d676ff9956f6c136ebfc657ff4f1e4a4a9798f0d4fa0ad06c",
    },
    ModuleSnapshot {
        path: "tool/parser_research/comrak_inline_fragment_gate/vendor/comrak/src/character_set.rs",
        lines: 18,
        sha256: "0bcf4198c5a2cf036980b2c710f7f946bec9df7eede34a5baeaf058dcacf3445",
    },
];

const REFERENCE_VALUE_DONOR_FUNCTIONS: &[FunctionSnapshot] = &[
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/strings.rs",
        start_line: 17,
        end_line: 52,
        sha256: "f341333da441c4b969bf8aacd8e4ef8162a42411814a1130f85916b01f01eca5",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/strings.rs",
        start_line: 293,
        end_line: 303,
        sha256: "920d220917a5a219e29a96055b208b21ef0c31d3799a9ff71865da7a48fbc62e",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/strings.rs",
        start_line: 305,
        end_line: 327,
        sha256: "bcdafd1004bd203d28de23707152e0c2588e7cc77ce245dad14ea02f23dd3dec",
    },
    FunctionSnapshot {
        path: "native/comrak_bridge/crates/flark_parser/vendor/comrak/src/parser/block_spine_facade.rs",
        start_line: 381,
        end_line: 406,
        sha256: "7e839db266d72ba295d746d907f7770437ee895bacc24bedcee66dc78c4bf995",
    },
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .canonicalize()
        .expect("workspace root")
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verify_module(root: &Path, snapshot: ModuleSnapshot) {
    let bytes = fs::read(root.join(snapshot.path)).expect("audited source file");
    let line_count = std::str::from_utf8(&bytes)
        .expect("audited Rust source is UTF-8")
        .lines()
        .count();
    assert_eq!(line_count, snapshot.lines, "line drift: {}", snapshot.path);
    assert_eq!(
        digest(&bytes),
        snapshot.sha256,
        "hash drift: {}",
        snapshot.path
    );
}

fn verify_function(root: &Path, snapshot: FunctionSnapshot) {
    let bytes = fs::read(root.join(snapshot.path)).expect("audited function source");
    let mut hasher = Sha256::new();
    let mut selected = 0;
    for (index, line) in bytes.split_inclusive(|byte| *byte == b'\n').enumerate() {
        let line_number = index + 1;
        if (snapshot.start_line..=snapshot.end_line).contains(&line_number) {
            hasher.update(line);
            selected += 1;
        }
    }
    assert_eq!(selected, snapshot.end_line - snapshot.start_line + 1);
    assert_eq!(
        format!("{:x}", hasher.finalize()),
        snapshot.sha256,
        "function drift: {}:{}-{}",
        snapshot.path,
        snapshot.start_line,
        snapshot.end_line,
    );
}

#[test]
fn recorded_compile_boundary_matches_the_live_research_sources() {
    let root = repository_root();
    for snapshot in RESEARCH_CONTROLLER_CLOSURE
        .iter()
        .chain(GENERATED_ATX_CLOSURE)
        .chain(REFERENCE_LABEL_CLOSURE)
        .chain(REFERENCE_VALUE_SERVICE_CLOSURE)
        .chain(CUSTOM_COMRAK_CALLABLE_CLOSURE)
        .copied()
    {
        verify_module(&root, snapshot);
    }
    for snapshot in SOURCE_SEAM_FUNCTIONS.iter().copied() {
        verify_function(&root, snapshot);
    }

    assert_eq!(
        RESEARCH_CONTROLLER_CLOSURE
            .iter()
            .map(|module| module.lines)
            .sum::<usize>(),
        12_608,
    );
    assert_eq!(
        GENERATED_ATX_CLOSURE
            .iter()
            .map(|module| module.lines)
            .sum::<usize>(),
        1_733,
    );
    assert_eq!(
        REFERENCE_LABEL_CLOSURE
            .iter()
            .map(|module| module.lines)
            .sum::<usize>(),
        253,
    );
    assert_eq!(
        REFERENCE_VALUE_SERVICE_CLOSURE
            .iter()
            .map(|module| module.lines)
            .sum::<usize>(),
        726,
    );
    assert_eq!(
        CUSTOM_COMRAK_CALLABLE_CLOSURE
            .iter()
            .map(|module| module.lines)
            .sum::<usize>(),
        24_972,
    );
}

#[test]
fn manually_mirrored_lexical_helpers_have_function_level_drift_receipts() {
    let root = repository_root();
    for snapshot in SEGMENTED_LEXICAL_DONOR_MODULES
        .iter()
        .chain(REFERENCE_VALUE_DONOR_MODULES)
        .copied()
    {
        verify_module(&root, snapshot);
    }
    for snapshot in VENDORED_DONOR_FUNCTIONS
        .iter()
        .chain(REFERENCE_VALUE_DONOR_FUNCTIONS)
        .copied()
    {
        verify_function(&root, snapshot);
    }
}
