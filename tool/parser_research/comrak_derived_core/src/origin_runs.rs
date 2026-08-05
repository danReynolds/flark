//! Exact logical-inline to physical-editor origin partitions.
//!
//! A boundary-only offset map cannot represent CRLF normalization, tab
//! expansion, or hidden block prefixes without losing edit semantics. This
//! prototype keeps those transformations explicit and atomic.

use std::ops::Range;

const TAB_STOP: usize = 4;
type PhysicalLineEnding = Option<(Range<usize>, Option<TransformKind>)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HiddenKind {
    ContainerPrefix,
    InterLineGap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransformKind {
    CrLfToLf,
    CrToLf,
    TabToSpaces { spaces: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntheticKind {
    MissingPhysicalLineJoin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginRun {
    Identity {
        logical: Range<usize>,
        physical: Range<usize>,
    },
    AtomicTransform {
        logical: Range<usize>,
        physical: Range<usize>,
        kind: TransformKind,
    },
    Hidden {
        logical_boundary: usize,
        physical: Range<usize>,
        kind: HiddenKind,
    },
    Synthetic {
        logical: Range<usize>,
        physical_boundary: usize,
        kind: SyntheticKind,
    },
}

impl OriginRun {
    pub fn logical_range(&self) -> Range<usize> {
        match self {
            Self::Identity { logical, .. }
            | Self::AtomicTransform { logical, .. }
            | Self::Synthetic { logical, .. } => logical.clone(),
            Self::Hidden {
                logical_boundary, ..
            } => *logical_boundary..*logical_boundary,
        }
    }

    pub fn physical_range(&self) -> Range<usize> {
        match self {
            Self::Identity { physical, .. }
            | Self::AtomicTransform { physical, .. }
            | Self::Hidden { physical, .. } => physical.clone(),
            Self::Synthetic {
                physical_boundary, ..
            } => *physical_boundary..*physical_boundary,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafLine {
    /// Complete physical line, including CR/LF when present.
    pub source: Range<usize>,
    /// Inline-bearing physical content after block markers/prefixes.
    pub visible: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginMappedLeaf {
    pub logical: String,
    pub runs: Vec<OriginRun>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginError {
    SourceOutOfBounds(Range<usize>),
    VisibleOutsideLine {
        source: Range<usize>,
        visible: Range<usize>,
    },
    OverlappingLines {
        previous: Range<usize>,
        next: Range<usize>,
    },
    NonCharacterBoundary(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectedSegment {
    Identity {
        logical: Range<usize>,
        physical: Range<usize>,
    },
    Atomic {
        logical: Range<usize>,
        physical: Range<usize>,
        kind: TransformKind,
    },
    Synthetic {
        logical: Range<usize>,
        physical_boundary: usize,
        kind: SyntheticKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhysicalHit {
    Identity {
        logical_offset: usize,
    },
    Atomic {
        logical: Range<usize>,
        physical: Range<usize>,
        kind: TransformKind,
    },
    Hidden {
        logical_boundary: usize,
        physical: Range<usize>,
        kind: HiddenKind,
    },
    Outside,
}

impl OriginMappedLeaf {
    /// Return disjoint physical source segments for a logical inline range.
    /// Hidden prefixes never silently join adjacent segments. Atomic
    /// transforms expand to their complete physical spelling.
    pub fn project_logical(&self, query: Range<usize>) -> Vec<ProjectedSegment> {
        assert!(query.start <= query.end && query.end <= self.logical.len());
        let mut result = Vec::new();
        for run in &self.runs {
            match run {
                OriginRun::Identity { logical, physical } => {
                    if let Some(overlap) = intersection(logical, &query) {
                        let start_delta = overlap.start - logical.start;
                        let end_delta = overlap.end - logical.start;
                        result.push(ProjectedSegment::Identity {
                            logical: overlap,
                            physical: physical.start + start_delta..physical.start + end_delta,
                        });
                    }
                }
                OriginRun::AtomicTransform {
                    logical,
                    physical,
                    kind,
                } => {
                    if intersection(logical, &query).is_some() {
                        result.push(ProjectedSegment::Atomic {
                            logical: logical.clone(),
                            physical: physical.clone(),
                            kind: *kind,
                        });
                    }
                }
                OriginRun::Synthetic {
                    logical,
                    physical_boundary,
                    kind,
                } => {
                    if intersection(logical, &query).is_some() {
                        result.push(ProjectedSegment::Synthetic {
                            logical: logical.clone(),
                            physical_boundary: *physical_boundary,
                            kind: *kind,
                        });
                    }
                }
                OriginRun::Hidden { .. } => {}
            }
        }
        result
    }

    /// Resolve a physical byte to an exact origin class. A byte in CRLF, a
    /// transformed tab, or a hidden prefix is never guessed onto one logical
    /// boundary.
    pub fn physical_hit(&self, offset: usize) -> PhysicalHit {
        for run in &self.runs {
            match run {
                OriginRun::Identity { logical, physical } if physical.contains(&offset) => {
                    return PhysicalHit::Identity {
                        logical_offset: logical.start + offset - physical.start,
                    };
                }
                OriginRun::AtomicTransform {
                    logical,
                    physical,
                    kind,
                } if physical.contains(&offset) => {
                    return PhysicalHit::Atomic {
                        logical: logical.clone(),
                        physical: physical.clone(),
                        kind: *kind,
                    };
                }
                OriginRun::Hidden {
                    logical_boundary,
                    physical,
                    kind,
                } if physical.contains(&offset) => {
                    return PhysicalHit::Hidden {
                        logical_boundary: *logical_boundary,
                        physical: physical.clone(),
                        kind: *kind,
                    };
                }
                _ => {}
            }
        }
        PhysicalHit::Outside
    }

    pub fn validate(&self, physical_source_len: usize) -> bool {
        let mut logical_cursor = 0;
        for run in &self.runs {
            let logical = run.logical_range();
            if logical.start != logical_cursor || logical.end > self.logical.len() {
                return false;
            }
            if !matches!(run, OriginRun::Hidden { .. }) {
                logical_cursor = logical.end;
            }
            let physical = run.physical_range();
            if physical.start > physical.end || physical.end > physical_source_len {
                return false;
            }
        }
        logical_cursor == self.logical.len()
    }
}

pub fn build_origin_mapped_leaf(
    source: &str,
    lines: &[LeafLine],
    expand_tabs: bool,
) -> Result<OriginMappedLeaf, OriginError> {
    validate_lines(source, lines)?;
    let mut mapped = OriginMappedLeaf {
        logical: String::new(),
        runs: Vec::new(),
    };
    let mut previous_end = None;
    for (index, line) in lines.iter().enumerate() {
        if let Some(end) = previous_end {
            if end < line.source.start {
                mapped.runs.push(OriginRun::Hidden {
                    logical_boundary: mapped.logical.len(),
                    physical: end..line.source.start,
                    kind: HiddenKind::InterLineGap,
                });
            }
        }
        if line.source.start < line.visible.start {
            mapped.runs.push(OriginRun::Hidden {
                logical_boundary: mapped.logical.len(),
                physical: line.source.start..line.visible.start,
                kind: HiddenKind::ContainerPrefix,
            });
        }

        let (content_end, ending) = split_physical_line_ending(source, line.source.clone());
        let visible_end = line.visible.end.min(content_end);
        append_visible(
            &mut mapped,
            source,
            line.visible.start..visible_end,
            expand_tabs,
        );
        if visible_end < content_end {
            mapped.runs.push(OriginRun::Hidden {
                logical_boundary: mapped.logical.len(),
                physical: visible_end..content_end,
                kind: HiddenKind::InterLineGap,
            });
        }

        if let Some((physical, kind)) = ending {
            let logical_start = mapped.logical.len();
            mapped.logical.push('\n');
            if let Some(kind) = kind {
                mapped.runs.push(OriginRun::AtomicTransform {
                    logical: logical_start..logical_start + 1,
                    physical,
                    kind,
                });
            } else {
                mapped.runs.push(OriginRun::Identity {
                    logical: logical_start..logical_start + 1,
                    physical,
                });
            }
        } else if index + 1 < lines.len() {
            let logical_start = mapped.logical.len();
            mapped.logical.push('\n');
            mapped.runs.push(OriginRun::Synthetic {
                logical: logical_start..logical_start + 1,
                physical_boundary: line.source.end,
                kind: SyntheticKind::MissingPhysicalLineJoin,
            });
        }
        previous_end = Some(line.source.end);
    }
    debug_assert!(mapped.validate(source.len()));
    Ok(mapped)
}

fn append_visible(
    mapped: &mut OriginMappedLeaf,
    source: &str,
    physical: Range<usize>,
    expand_tabs: bool,
) {
    let mut identity_start = physical.start;
    let mut cursor = physical.start;
    let mut column = mapped
        .logical
        .rsplit_once('\n')
        .map_or(mapped.logical.len(), |(_, tail)| tail.len());
    while cursor < physical.end {
        let character = source[cursor..physical.end]
            .chars()
            .next()
            .expect("validated character boundary");
        let width = character.len_utf8();
        if character == '\t' && expand_tabs {
            append_identity(mapped, source, identity_start..cursor);
            let spaces = TAB_STOP - (column % TAB_STOP);
            let logical_start = mapped.logical.len();
            mapped.logical.extend(std::iter::repeat_n(' ', spaces));
            mapped.runs.push(OriginRun::AtomicTransform {
                logical: logical_start..logical_start + spaces,
                physical: cursor..cursor + 1,
                kind: TransformKind::TabToSpaces {
                    spaces: spaces as u8,
                },
            });
            cursor += 1;
            identity_start = cursor;
            column += spaces;
        } else {
            cursor += width;
            column += width;
        }
    }
    append_identity(mapped, source, identity_start..physical.end);
}

fn append_identity(mapped: &mut OriginMappedLeaf, source: &str, physical: Range<usize>) {
    if physical.is_empty() {
        return;
    }
    let logical_start = mapped.logical.len();
    mapped.logical.push_str(&source[physical.clone()]);
    mapped.runs.push(OriginRun::Identity {
        logical: logical_start..mapped.logical.len(),
        physical,
    });
}

fn split_physical_line_ending(source: &str, line: Range<usize>) -> (usize, PhysicalLineEnding) {
    let bytes = source.as_bytes();
    if line.end >= line.start + 2 && &bytes[line.end - 2..line.end] == b"\r\n" {
        (
            line.end - 2,
            Some((line.end - 2..line.end, Some(TransformKind::CrLfToLf))),
        )
    } else if line.end > line.start && bytes[line.end - 1] == b'\r' {
        (
            line.end - 1,
            Some((line.end - 1..line.end, Some(TransformKind::CrToLf))),
        )
    } else if line.end > line.start && bytes[line.end - 1] == b'\n' {
        // LF already has identical physical/logical spelling.
        (line.end - 1, Some((line.end - 1..line.end, None)))
    } else {
        (line.end, None)
    }
}

fn validate_lines(source: &str, lines: &[LeafLine]) -> Result<(), OriginError> {
    let mut previous = None::<Range<usize>>;
    for line in lines {
        if line.source.start > line.source.end || line.source.end > source.len() {
            return Err(OriginError::SourceOutOfBounds(line.source.clone()));
        }
        if line.visible.start < line.source.start
            || line.visible.start > line.visible.end
            || line.visible.end > line.source.end
        {
            return Err(OriginError::VisibleOutsideLine {
                source: line.source.clone(),
                visible: line.visible.clone(),
            });
        }
        for boundary in [
            line.source.start,
            line.source.end,
            line.visible.start,
            line.visible.end,
        ] {
            if !source.is_char_boundary(boundary) {
                return Err(OriginError::NonCharacterBoundary(boundary));
            }
        }
        if let Some(previous) = previous {
            if line.source.start < previous.end {
                return Err(OriginError::OverlappingLines {
                    previous,
                    next: line.source.clone(),
                });
            }
        }
        previous = Some(line.source.clone());
    }
    Ok(())
}

fn intersection(left: &Range<usize>, right: &Range<usize>) -> Option<Range<usize>> {
    let start = left.start.max(right.start);
    let end = left.end.min(right.end);
    (start < end).then_some(start..end)
}
