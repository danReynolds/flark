// Edge cases: literals next to words, strings and comments across lines.
/*
 * A block comment that really spans lines,
 *     with indented text inside it.
 */
/** Doc block */ fn documented() {}

const SUFFIXED: [f32; 3] = [1f32, 1e5f32, 2.5e-3f64 as f32];
const MIXED: u64 = 0x1Fu64 + 0o17 + 0b11 + 1_000 + 3usize as u64;
const BAD: i32 = 1.5x + 12abc + 0xZZ;
const ODD: bool = 1as u8 == 2u8as;

fn tuple_index(t: ((u8, u8), u8)) -> u8 {
    t.0.1 + t.1
}

fn chars() {
    let quote = '\'';
    let slash = '\\';
    let emoji = '\u{1F600}';
    let six = '\u{01F600}';
    let byte = b'\x00';
    let tick = b'\'';
    let lifetime: &'static str = "x";
    let label = 'outer: loop { break 'outer; };
    let under: &'_ str = lifetime;
}

fn strings() -> String {
    let a = "ends with a backslash \
             and continues here";
    let b = "unterminated
    until this line";
    let c = r"raw with \n inside";
    let d = r##"raw "# still raw"##;
    let e = b"bytes";
    let f = br"raw bytes";
    format!("{a}{b}{c}{d}{:?}{:?}", e, f)
}

fn macros_and_operators(x: i32, y: i32) -> bool {
    let v = vec![1, 2, 3];
    let s = format!("{}", x!=y);
    let t = x != y;
    assert!(x <= y && !t || x >> 2 == y << 1);
    matches!(v.len(), 0..=3) && s.is_empty()
}

let (a, b) = (1, 2);
let mut counter = 0;
letter = fnord + r#type + café;

#[derive(Debug)] struct Unit;
#[cfg(all(unix, not(target_os = "macos")))] fn unix_only() {}
#![doc = "inner [with] brackets"]

impl Unit {
    pub fn new() -> Self { Self }
    fn get(&self) -> Option<Self> { Some(Self) }
    fn check(&self) -> Result<(), String> {
        if true { Ok(()) } else { Err(String::from("no")) }
    }
}

fn nesting() {
    let v = [
        (1, 2),
        (3, 4),
    ];
    call(|x| {
        x + 1
    });
    if let [first, ..] = v {
        match first {
            (1, _) => {}
            _ => (),
        }
    }
}

	fn tabbed() {
		let x = 1;
	}

type Callback<'a> = Box<dyn Fn(&'a str) -> bool + Send + 'a>;
union Bits { i: u32, f: f32 }
extern "C" { fn abs(x: i32) -> i32; }
unsafe impl Send for Unit {}
async fn later() { later_still().await }
