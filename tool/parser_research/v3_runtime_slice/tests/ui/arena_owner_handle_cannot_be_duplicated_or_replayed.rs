#[path = "../../src/arena.rs"]
mod arena;

fn duplicate_handle() {
    let mut arena = arena::PageArena::new();
    let mut transaction = arena::ArenaBuildTransaction::new(&mut arena);
    let (duplicated_handle, _) = transaction.allocate(b"duplicate", &[]).unwrap();
    let duplicate = duplicated_handle;
    let _ = transaction.id(&duplicated_handle);
    let _ = duplicate;
}

fn replay_release() {
    let mut arena = arena::PageArena::new();
    let mut transaction = arena::ArenaBuildTransaction::new(&mut arena);
    let (released_handle, _) = transaction.allocate(b"release", &[]).unwrap();
    transaction.release(released_handle).unwrap();
    transaction.release(released_handle).unwrap();
}

fn main() {
    duplicate_handle();
    replay_release();
}
