use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use comrak::inline_fragment::{
    EMPTY_REFERENCE_SNAPSHOT, InlineFragmentRequest, InlineInputKind, InlineProfile,
    MAX_INLINE_FRAGMENT_BYTES, parse_inline_fragment,
};

struct CountingAllocator;

static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static DEALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding the allocator contract unchanged to System.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record_deallocation(layout.size());
        // SAFETY: pointer and layout came from this allocator and are forwarded
        // to the System allocator that created them.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarding the allocator contract unchanged to System.
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            record_deallocation(layout.size());
            record_allocation(new_size);
        }
        replacement
    }
}

fn record_allocation(bytes: usize) {
    ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK_LIVE_BYTES.fetch_max(live, Ordering::Relaxed);
}

fn record_deallocation(bytes: usize) {
    DEALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
    DEALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    LIVE_BYTES.fetch_sub(bytes, Ordering::Relaxed);
}

#[derive(Clone, Copy)]
struct AllocationSnapshot {
    calls: usize,
    dealloc_calls: usize,
    bytes: usize,
    deallocated_bytes: usize,
    live: usize,
}

fn allocation_snapshot() -> AllocationSnapshot {
    AllocationSnapshot {
        calls: ALLOC_CALLS.load(Ordering::Relaxed),
        dealloc_calls: DEALLOC_CALLS.load(Ordering::Relaxed),
        bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        live: LIVE_BYTES.load(Ordering::Relaxed),
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let document_bytes = parse_arg(&mut args, 10 * 1024 * 1024);
    let leaf_bytes = parse_arg(&mut args, 96);
    let shape = args.next().unwrap_or_else(|| "ordinary".into());
    let retention = args.next().unwrap_or_else(|| "drain".into());
    let retain = retention == "retain";
    assert!(leaf_bytes <= MAX_INLINE_FRAGMENT_BYTES);

    let document = generate_document(document_bytes, leaf_bytes, &shape);
    black_box(document.as_ptr());
    let before = allocation_snapshot();
    PEAK_LIVE_BYTES.store(before.live, Ordering::Relaxed);
    let rss_before = current_rss_bytes();

    let started = Instant::now();
    let mut first_leaf_ns = 0_u128;
    let mut leaves = 0_usize;
    let mut parsed_bytes = 0_usize;
    let mut facts = 0_usize;
    let mut output_bytes = 0_usize;
    let mut checksum = 0_u64;
    let mut errors = 0_usize;
    let mut retained_fragments = Vec::new();

    for logical in document.split("\n\n") {
        if logical.is_empty() {
            continue;
        }
        let leaf_started = Instant::now();
        let result = parse_inline_fragment(InlineFragmentRequest {
            logical,
            leaf_id: leaves as u64,
            kind: InlineInputKind::Paragraph,
            profile: InlineProfile::Gfm,
            reference_snapshot: &EMPTY_REFERENCE_SNAPSHOT,
            revision: 1,
            expected_revision: 1,
        });
        if leaves == 0 {
            first_leaf_ns = leaf_started.elapsed().as_nanos();
        }
        match result {
            Ok(fragment) => {
                let leaf_facts = fragment.facts.len() + fragment.projection_facts.len();
                let leaf_output = fragment.output_bytes();
                facts += leaf_facts;
                output_bytes += leaf_output;
                checksum = checksum
                    .wrapping_add(leaf_facts as u64)
                    .wrapping_add(leaf_output as u64);
                if retain {
                    retained_fragments.push(fragment);
                } else {
                    drop(fragment);
                }
            }
            Err(_) => errors += 1,
        }
        leaves += 1;
        parsed_bytes += logical.len();
    }
    let elapsed = started.elapsed();
    black_box(checksum);
    black_box(retained_fragments.len());

    let after = allocation_snapshot();
    let peak_live = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
    let rss_after = current_rss_bytes();
    let seconds = elapsed.as_secs_f64();
    println!(
        "backend=native-stream text_storage={} retention={retention} document_bytes={} parsed_bytes={parsed_bytes} leaf_bytes={leaf_bytes} shape={shape} leaves={leaves} elapsed_ns={} first_leaf_ns={first_leaf_ns} mib_per_s={:.3} leaves_per_s={:.3} facts={facts} output_bytes={output_bytes} errors={errors} alloc_calls={} dealloc_calls={} allocated_bytes={} deallocated_bytes={} max_transient_live_bytes={} retained_live_delta_bytes={} rss_before_bytes={rss_before} rss_after_bytes={rss_after} max_rss_bytes={} checksum={checksum}",
        if cfg!(feature = "research-owned-text") {
            "owned"
        } else {
            "source-backed"
        },
        document.len(),
        elapsed.as_nanos(),
        parsed_bytes as f64 / (1024.0 * 1024.0) / seconds,
        leaves as f64 / seconds,
        after.calls - before.calls,
        after.dealloc_calls - before.dealloc_calls,
        after.bytes - before.bytes,
        after.deallocated_bytes - before.deallocated_bytes,
        peak_live.saturating_sub(before.live),
        after.live as isize - before.live as isize,
        max_rss_bytes(),
    );
}

fn parse_arg(args: &mut impl Iterator<Item = String>, fallback: usize) -> usize {
    args.next()
        .map_or(fallback, |value| value.parse().expect("integer argument"))
}

fn generate_document(target_bytes: usize, leaf_bytes: usize, shape: &str) -> String {
    let atom = match shape {
        "plain" => "This is an ordinary prose sentence with no markup. ",
        "dense" => "**bold** *em* `code` [link](https://example.com) ~~strike~~ ",
        _ => "An ordinary *live* paragraph with **strong words**, `code`, and [a link](u). ",
    };
    let mut leaf = String::with_capacity(leaf_bytes);
    while leaf.len() + atom.len() <= leaf_bytes {
        leaf.push_str(atom);
    }
    leaf.push_str(&"x".repeat(leaf_bytes - leaf.len()));

    let mut document = String::with_capacity(target_bytes);
    while document.len() + leaf.len() + usize::from(!document.is_empty()) * 2 <= target_bytes {
        if !document.is_empty() {
            document.push_str("\n\n");
        }
        document.push_str(&leaf);
    }
    document
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn current_rss_bytes() -> u64 {
    let mut info = std::mem::MaybeUninit::<libc::mach_task_basic_info_data_t>::uninit();
    let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
    // SAFETY: task_info writes exactly the requested structure on success.
    let result = unsafe {
        libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            info.as_mut_ptr().cast(),
            &mut count,
        )
    };
    if result == 0 {
        // SAFETY: successful task_info initialized `info`.
        unsafe { info.assume_init().resident_size }
    } else {
        0
    }
}

#[cfg(not(target_os = "macos"))]
fn current_rss_bytes() -> u64 {
    0
}

#[cfg(target_os = "macos")]
fn max_rss_bytes() -> i64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes the pointed-to rusage on success.
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result == 0 {
        // SAFETY: successful getrusage initialized `usage`.
        unsafe { usage.assume_init().ru_maxrss }
    } else {
        -1
    }
}

#[cfg(not(target_os = "macos"))]
fn max_rss_bytes() -> i64 {
    -1
}
