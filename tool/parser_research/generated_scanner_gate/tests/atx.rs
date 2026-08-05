use comrak::block_spine_facade as facade;
use generated_scanner_gate::{AtxScanner, ScanResult};

fn run(input: &[u8], grant: usize) -> (ScanResult, usize) {
    let mut scanner = AtxScanner::new(input);
    loop {
        let result = scanner.poll(grant);
        if result != ScanResult::NeedMore {
            return (result, scanner.cursor());
        }
    }
}

#[test]
fn resumes_at_every_grant_size() {
    let cases: &[(&[u8], ScanResult)] = &[
        (b"# heading", ScanResult::Matched(2)),
        (b"######\ttext", ScanResult::Matched(7)),
        (b"######", ScanResult::Matched(6)),
        (b"####### text", ScanResult::NoMatch),
        (b"#no-space", ScanResult::NoMatch),
        (b"not a heading", ScanResult::NoMatch),
    ];

    for &(input, expected) in cases {
        for grant in 1..=input.len().max(1) + 1 {
            let (actual, _) = run(input, grant);
            assert_eq!(actual, expected, "input={input:?}, grant={grant}");
        }
    }
}

#[test]
fn one_megabyte_space_run_is_resumable() {
    let mut input = vec![b'#', b' '];
    input.extend(std::iter::repeat_n(b' ', 1024 * 1024));
    input.push(b'x');

    let mut scanner = AtxScanner::new(&input);
    let mut polls = 0;
    loop {
        let before = scanner.cursor();
        let result = scanner.poll(4096);
        let inspected = scanner.cursor() - before;
        assert!(inspected <= 4096, "inspected {inspected} bytes in one poll");
        polls += 1;
        if result != ScanResult::NeedMore {
            assert_eq!(result, ScanResult::Matched(input.len() - 1));
            break;
        }
    }
    assert!(polls > 200);
}

#[test]
fn generated_storable_dfa_matches_the_pinned_comrak_facade() {
    let alphabet = b"# abcXYZ09\t";
    let mut rng = Lcg(0x6d61_726b_646f_776e);

    for case in 0..50_000 {
        let len = rng.usize(192);
        let mut input: String = (0..len)
            .map(|_| alphabet[rng.usize(alphabet.len())] as char)
            .collect();
        match case % 4 {
            0 => input.push('\n'),
            1 => input.push_str("\r\n"),
            _ => {}
        }

        let expected = facade::atx_heading_start(&input).unwrap();
        for grant in [1, 2, 3, 7, 4096] {
            let actual = match run(input.as_bytes(), grant).0 {
                ScanResult::Matched(bytes) => Some(bytes),
                ScanResult::NoMatch => None,
                ScanResult::NeedMore => unreachable!(),
            };
            assert_eq!(
                actual, expected,
                "case={case}, grant={grant}, input={input:?}"
            );
        }
    }
}

struct Lcg(u64);

impl Lcg {
    fn usize(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 32) as usize) % upper
    }
}
