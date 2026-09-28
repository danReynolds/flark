//! A small inventory service, written to exercise the highlighter.
#![allow(dead_code)]
#![cfg_attr(test, feature(custom_test_frameworks))]

use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

/* A block comment /* with a nested one */ that the mode
   ends at the first closing marker. */

/// Where an item is stored.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    Shelf { aisle: u8, row: u16 },
    Bin(u32),
    Unknown,
}

#[derive(Debug, Default)]
pub struct Item<'a, T: Clone + Default = u32> {
    pub name: &'a str,
    pub count: T,
    tags: Vec<&'static str>,
}

pub trait Describe {
    const KIND: &'static str;
    fn describe(&self) -> String;
    fn shout(&self) -> String where Self: Sized {
        self.describe().to_uppercase()
    }
}

impl<'a, T> Describe for Item<'a, T>
where
    T: Clone + Default + Display,
{
    const KIND: &'static str = "item";

    fn describe(&self) -> String {
        format!("{} x{} [{}]", self.name, self.count, self.tags.join(", "))
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Location::Shelf { aisle, row } if *aisle > 9 => write!(f, "far {aisle}/{row}"),
            Location::Shelf { aisle, row } => write!(f, "{}/{}", aisle, row),
            Location::Bin(n @ 0..=99) => write!(f, "small bin {n}"),
            Location::Bin(n) => write!(f, "bin {n}"),
            _ => f.write_str("?"),
        }
    }
}

mod literals {
    pub const NUMBERS: [f64; 4] = [1.5e3, 2.0f64, 0.25, 1e-9];
    pub const INTS: (u8, i64, usize, u32) = (0xFFu8, -1_000_000i64, 0o777usize, 0b1010_1010);
    pub static CHARS: [char; 5] = ['a', '\n', '\'', '\x7f', '\u{01F600}'];
    pub static BYTES: &[u8] = b"raw\x00bytes\"";
    pub const BYTE: u8 = b'\\';
    pub const RAW: &str = r"C:\path\without\escapes";
    pub const HASHED: &str = r#"He said "hi" and left"#;
    pub const RAW_BYTES: &[u8] = br##"a "#" inside"##;
    pub const SUFFIX: f32 = 1f32 + 2.5f32;
}

fn longest<'x>(a: &'x str, b: &'x str) -> &'x str {
    if a.len() >= b.len() { a } else { b }
}

pub async fn load(path: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let text = tokio::fs::read_to_string(path).await?;
    let lines: Vec<String> = text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|line| line.to_owned())
        .collect();
    Ok(lines)
}

fn tally(words: &[&str]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for word in words.iter().copied() {
        *counts.entry(word.to_lowercase()).or_insert(0) += 1;
    }
    counts
}

macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

fn main() {
    let mut store: BTreeMap<u32, Item<'_>> = BTreeMap::new();
    let shared = Arc::new(Location::Bin(42));
    let (first, second) = (Some(3), None::<i32>);
    let multiline = "first line
second line with an escaped \" quote and a trailing slash \\";
    let hashed = r#"one line
and "another" line"#;
    store.insert(1, Item { name: "bolt", count: 12, tags: vec!["m4", "steel"] });
    let doubled = |n: u32| -> u32 { n * 2 };
    let total: u32 = store.values().map(|it| doubled(it.count)).sum();
    if total != 0 && first.is_some() || second.is_none() {
        println!("total={} square={} at {}", total, square!(4), shared);
    }
    while let Some((key, _)) = store.pop_first() {
        eprintln!("removed {key:>4}");
        break;
    }
    let raw = unsafe { std::mem::transmute::<u32, f32>(0x3f80_0000) };
    assert_eq!(raw, 1.0, "bits");
    let _ = longest("abc", "de");
    let _ = (multiline, hashed, tally(&["a", "A", "b"]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes() {
        let item = Item { name: "nut", count: 3u32, tags: vec![] };
        assert!(item.describe().contains("nut"));
    }
}
