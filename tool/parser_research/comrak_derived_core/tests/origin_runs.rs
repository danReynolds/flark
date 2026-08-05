use flark_comrak_derived_core_probe::origin_runs::{
    build_origin_mapped_leaf, HiddenKind, LeafLine, OriginRun, PhysicalHit, ProjectedSegment,
    TransformKind,
};

#[test]
fn crlf_container_joins_and_tabs_retain_exact_origin_classes() {
    let source = "> alpha\r\n>\tbeta\r\n";
    let mapped = build_origin_mapped_leaf(
        source,
        &[
            LeafLine {
                source: 0..9,
                visible: 2..7,
            },
            LeafLine {
                source: 9..17,
                visible: 10..15,
            },
        ],
        true,
    )
    .unwrap();

    assert_eq!(mapped.logical, "alpha\n    beta\n");
    assert!(mapped.validate(source.len()));
    assert_eq!(
        mapped.runs,
        [
            OriginRun::Hidden {
                logical_boundary: 0,
                physical: 0..2,
                kind: HiddenKind::ContainerPrefix,
            },
            OriginRun::Identity {
                logical: 0..5,
                physical: 2..7,
            },
            OriginRun::AtomicTransform {
                logical: 5..6,
                physical: 7..9,
                kind: TransformKind::CrLfToLf,
            },
            OriginRun::Hidden {
                logical_boundary: 6,
                physical: 9..10,
                kind: HiddenKind::ContainerPrefix,
            },
            OriginRun::AtomicTransform {
                logical: 6..10,
                physical: 10..11,
                kind: TransformKind::TabToSpaces { spaces: 4 },
            },
            OriginRun::Identity {
                logical: 10..14,
                physical: 11..15,
            },
            OriginRun::AtomicTransform {
                logical: 14..15,
                physical: 15..17,
                kind: TransformKind::CrLfToLf,
            },
        ]
    );
}

#[test]
fn multiline_inline_fact_projects_disjoint_source_without_stripping_prefixes() {
    let source = "> **alpha**\r\n>\t*beta*\r\n";
    let mapped = build_origin_mapped_leaf(
        source,
        &[
            LeafLine {
                source: 0..13,
                visible: 2..11,
            },
            LeafLine {
                source: 13..23,
                visible: 14..21,
            },
        ],
        true,
    )
    .unwrap();
    assert_eq!(mapped.logical, "**alpha**\n    *beta*\n");

    let projected = mapped.project_logical(0..mapped.logical.len());
    assert!(projected.iter().any(|segment| matches!(
        segment,
        ProjectedSegment::Atomic {
            physical,
            kind: TransformKind::CrLfToLf,
            ..
        } if physical == &(11..13)
    )));
    assert!(projected.iter().any(|segment| matches!(
        segment,
        ProjectedSegment::Atomic {
            physical,
            kind: TransformKind::TabToSpaces { .. },
            ..
        } if physical == &(14..15)
    )));
    assert!(!projected.iter().any(|segment| match segment {
        ProjectedSegment::Identity { physical, .. } | ProjectedSegment::Atomic { physical, .. } =>
            physical.contains(&0) || physical.contains(&13),
        ProjectedSegment::Synthetic { .. } => false,
    }));
    assert!(matches!(
        mapped.physical_hit(0),
        PhysicalHit::Hidden {
            kind: HiddenKind::ContainerPrefix,
            ..
        }
    ));
    assert!(matches!(
        mapped.physical_hit(11),
        PhysicalHit::Atomic {
            kind: TransformKind::CrLfToLf,
            ..
        }
    ));
}

#[test]
fn only_missing_physical_line_endings_are_synthetic() {
    let source = "aHIDDENb";
    let mapped = build_origin_mapped_leaf(
        source,
        &[
            LeafLine {
                source: 0..1,
                visible: 0..1,
            },
            LeafLine {
                source: 7..8,
                visible: 7..8,
            },
        ],
        false,
    )
    .unwrap();
    assert_eq!(mapped.logical, "a\nb");
    assert!(mapped.runs.iter().any(|run| matches!(
        run,
        OriginRun::Hidden {
            physical,
            kind: HiddenKind::InterLineGap,
            ..
        } if physical == &(1..7)
    )));
    assert!(mapped.runs.iter().any(|run| matches!(
        run,
        OriginRun::Synthetic { logical, .. } if logical == &(1..2)
    )));
}
