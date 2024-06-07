use std::cell::RefCell;

#[derive(Debug)]
struct StackHeader {}

#[derive(Debug)]
struct YoungHeapHeader {}

#[derive(Debug)]
struct OldHeapHeader {}

#[derive(Debug)]
struct GcState {

}

#[derive(Debug, Clone, Copy)]
struct WordSize(u32);

#[derive(Debug, Clone, Copy)]
struct Pointer(u32);

thread_local! {
    static STATE: RefCell<GcState> = RefCell::new(GcState {

    });
}

pub fn alloc_heap(
    pointer_cnt: WordSize,
    data_size_32: WordSize,
    pointers_mutable: bool,
) -> Pointer {
    unimplemented!()
}

pub fn alloc0_heap(
    pointer_cnt: WordSize,
    data_size_32: WordSize,
    pointers_mutable: bool,
) -> Option<Pointer> {
    unimplemented!()
}

pub fn alloc_stack(
    pointer_cnt: WordSize,
    data_size_32: WordSize,
    pointers_mutable: bool,
) -> Pointer {
    unimplemented!()
}

pub fn alloc0_stack(
    pointer_cnt: WordSize,
    data_size_32: WordSize,
    pointers_mutable: bool,
) -> Option<Pointer> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_data_on_heap() {
        let orig = alloc_heap(WordSize(0), WordSize(2), false);
        let subsequent = alloc_heap(WordSize(0), WordSize(2), false);
        assert!(subsequent - orig == 12);
    }
}
