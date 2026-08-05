#[path = "../../src/arena.rs"]
mod arena;

fn duplicate_ticket() {
    let mut arena = arena::PageArena::new();
    let duplicated_ticket = arena.begin_build().unwrap();
    let duplicate = duplicated_ticket;
    let _ = duplicated_ticket.id();
    let _ = duplicate;
}

fn replay_ticket() {
    let mut arena = arena::PageArena::new();
    let replayed_ticket = arena.begin_build().unwrap();
    let _ = arena.begin_build_abort(replayed_ticket).unwrap();
    let _ = arena.resume_build(replayed_ticket);
}

fn main() {
    duplicate_ticket();
    replay_ticket();
}
