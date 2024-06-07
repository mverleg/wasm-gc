use std::cell::RefCell;
use std::ops::Sub;

#[derive(Debug)]
struct StackHeader {}

#[derive(Debug)]
struct YoungHeapHeader {}

#[derive(Debug)]
struct OldHeapHeader {}

#[derive(Debug)]
struct GcState {
    stack_capacity: WordSize,
    young_capacity: WordSize,
    old_capacity: WordSize,
    stack_len: WordSize,
    young_side: Side,
    young_len: WordSize,
    old_len: WordSize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ByteSize(u32);

#[derive(Debug, Clone, Copy, PartialEq)]
struct WordSize(u32);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pointer(u32);

impl Sub for Pointer {
    type Output = ByteSize;

    fn sub(self, rhs: Self) -> Self::Output {
        ByteSize(rhs.0 - self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Side { Left, Right }

thread_local! {
    static STATE: RefCell<GcState> = RefCell::new(GcState {
        stack_capacity: WordSize(1024),
        young_capacity: WordSize(16384),
        old_capacity: WordSize(16384),

        stack_len: WordSize(0),
        young_side: Side::Left,
        young_len: WordSize(0),
        old_len: WordSize(0),
        //TODO @mark: translate this to top addresse and derive lengths from them
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
        assert_eq!(subsequent - orig, ByteSize(12));
    }
}
