use std::sync::Arc;

use flark_comrak_value_block_core::{
    DirectExternalWork, DirectPollStatus, DirectReferenceDefinition,
    DirectReferencePrefixPollStatus, DirectReferencePrefixSource, DirectValueBlockParser,
    SyntaxProfile,
};
use flark_pulldown_inline_gate::{parse_to_completion, FactKind, LogicalLeaf, ReferenceTable};

struct LogicalSource<'a> {
    bytes: &'a [u8],
    next: usize,
}

impl DirectReferencePrefixSource for LogicalSource<'_> {
    type Identity = u64;
    type Error = ();

    fn identity(&self) -> Self::Identity {
        17
    }

    fn available_len(&self) -> usize {
        self.bytes.len()
    }

    fn is_final(&self) -> bool {
        true
    }

    fn access_budget(&self) -> usize {
        1
    }

    fn read_byte(&mut self, relative_offset: usize) -> Result<u8, Self::Error> {
        if relative_offset != self.next {
            return Err(());
        }
        self.next += 1;
        Ok(self.bytes[relative_offset])
    }

    fn raw_codepoint_contribution(&self, _logical_scalar_end_offset: usize) -> u8 {
        1
    }
}

fn block_definition(input: &str) -> DirectReferenceDefinition {
    let mut parser = DirectValueBlockParser::new(SyntaxProfile::CommonMark).unwrap();
    parser.acknowledge_command().unwrap();
    parser.begin_line(input.to_owned()).unwrap();
    loop {
        match parser.poll_line(1).unwrap().status {
            DirectPollStatus::Pending => {}
            DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
            DirectPollStatus::Complete => break,
            DirectPollStatus::ExternalWorkReady => panic!("finalizer waits for Paragraph close"),
        }
    }
    parser.begin_finish().unwrap();
    loop {
        match parser.poll_finish(1).unwrap().status {
            DirectPollStatus::Pending => {}
            DirectPollStatus::CommandReady => parser.acknowledge_command().unwrap(),
            DirectPollStatus::ExternalWorkReady => break,
            DirectPollStatus::Complete => panic!("reference candidate requests finalization"),
        }
    }
    let request = match parser.pending_external_work().unwrap() {
        DirectExternalWork::ReferencePrefixFinalizer { request } => *request,
    };
    let mut work = parser.begin_reference_prefix_work(request, 17).unwrap();
    let mut source = LogicalSource {
        bytes: input.as_bytes(),
        next: 0,
    };
    loop {
        match work.poll_source(&mut source, 1, false).unwrap().status {
            DirectReferencePrefixPollStatus::NeedMore => {}
            DirectReferencePrefixPollStatus::OutputReady => {
                return work.take_output().unwrap().acknowledge().0;
            }
            DirectReferencePrefixPollStatus::Complete => panic!("definition was rejected"),
            DirectReferencePrefixPollStatus::Cancelled => panic!("work was cancelled"),
        }
    }
}

#[test]
fn block_definition_and_inline_use_share_one_canonical_label_authority() {
    for (definition_source, inline_source, expected) in [
        ("[Straẞe]: /u\n", "[road][STRASSE]", "strasse"),
        (
            "[Foo\u{a0}BAR]: /u\n",
            "[road][FOO\u{a0}bar]",
            "foo\u{a0}bar",
        ),
        ("[Foo\u{b}BAR]: /u\n", "[road][FOO\u{b}bar]", "foo\u{b}bar"),
        ("[Foo\u{c}BAR]: /u\n", "[road][FOO\u{c}bar]", "foo\u{c}bar"),
    ] {
        let definition = block_definition(definition_source);
        assert_eq!(definition.normalized_label, expected);
        let mut references = ReferenceTable::new();
        references.define_normalized(definition.normalized_label, 41);
        let engine = parse_to_completion(
            LogicalLeaf::contiguous(inline_source),
            Arc::new(references),
            1,
        );
        assert!(engine.facts().iter().any(|fact| matches!(
            &fact.kind,
            FactKind::ReferenceLink {
                normalized_label,
                dependency_id: 41,
                ..
            } if normalized_label == expected
        )));
    }
}
