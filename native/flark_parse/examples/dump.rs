//! Print blocks, content records, and runs for one Markdown string (argv[1]).
use flark_parse::model::Extractor;
use flark_parse::schema::{self, block, content, header, run};
fn main() {
    let src = std::env::args().nth(1).expect("markdown").replace("\\n", "\n").replace("\\t", "\t");
    let (buf, devs) = Extractor::extract_with_report(&src);
    let w: Vec<u32> = buf.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
    let (nl, nb, nc, nr) = (w[header::LINE_COUNT] as usize, w[header::BLOCK_COUNT] as usize, w[header::CONTENT_COUNT] as usize, w[header::RUN_COUNT] as usize);
    let bo = schema::HEADER_WORDS + nl * 2; let co = bo + nb * block::WORDS; let ro = co + nc * content::WORDS;
    println!("src {:?}", src);
    for i in 0..nb { let b = &w[bo + i * block::WORDS..]; let (s, e) = (b[block::START_BYTE] as usize, b[block::END_BYTE] as usize); println!("block {i} kind {} parent {} {s}..{e} {:?} lines {}+{} content {}+{} attrs {} {} {} flags {}", b[block::KIND], b[block::PARENT] as i32, &src[s.min(src.len())..e.min(src.len())], b[block::FIRST_LINE], b[block::LINE_COUNT], b[block::CONTENT_OFFSET], b[block::CONTENT_COUNT], b[block::ATTR0], b[block::ATTR1], b[block::ATTR2], b[block::FLAGS]); }
    for i in 0..nc { let c = &w[co + i * content::WORDS..]; let (s, e) = (c[content::START_BYTE] as usize, c[content::END_BYTE] as usize); println!("  content {i} line {} {s}..{e} {:?} virt {}", c[content::LINE], &src[s.min(src.len())..e.min(src.len())], c[content::VIRTUAL_LEADING_SPACES]); }
    for i in 0..nr { let r = &w[ro + i * run::WORDS..]; let (s, e, cs, ce) = (r[run::START_BYTE] as usize, r[run::END_BYTE] as usize, r[run::CONTENT_START_BYTE] as usize, r[run::CONTENT_END_BYTE] as usize); println!("  run {i} kind {} block {} parent {} {s}..{e} {:?} content {cs}..{ce} {:?}", r[run::KIND], r[run::BLOCK], r[run::PARENT] as i32, &src[s..e], &src[cs..ce]); }
    for d in devs { println!("DEV {} {}", d.rule, d.detail); }
}
