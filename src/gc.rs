use ::std::cell::RefCell;
use ::std::mem::size_of;
use ::std::ops::Add;
use ::std::ops::Mul;
use ::std::ops::Sub;

type AddrNr = u32;
const WORD_SIZE: AddrNr = 4;

#[derive(Debug)]
struct StackHeader {}

#[derive(Debug)]
struct YoungHeapHeader {}

#[derive(Debug)]
struct OldHeapHeader {}

const OFFSET: Pointer = Pointer((size_of::<GcConf>() + size_of::<GcState>()) as u32 + WORD_SIZE);

#[derive(Debug)]
struct GcConf {
    stack_capacity: WordSize,
    young_side_capacity: WordSize,
    old_capacity: WordSize,
}

impl GcConf {
    fn stack_start(&self) -> Pointer {
        OFFSET
    }

    fn young_side_start(&self) -> Pointer {
        self.stack_start() + self.stack_capacity.bytes()
    }

    fn old_start(&self) -> Pointer {
        self.young_side_start() + self.young_side_capacity.bytes() * 2
    }
}

#[derive(Debug)]
struct GcState {
    stack_top: Pointer,
    young_side: Side,
    young_top: Pointer,
    old_top: Pointer,
}

impl GcState {
    fn stack_len(&self) -> WordSize {
        unimplemented!()
    }

    fn young_side(&self) -> Side {
        unimplemented!()
    }

    fn young_len(&self) -> WordSize {
        unimplemented!()
    }

    fn old_len(&self) -> WordSize {
        unimplemented!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ByteSize(AddrNr);

#[derive(Debug, Clone, Copy, PartialEq)]
struct WordSize(AddrNr);

impl WordSize {
    fn bytes(self) -> ByteSize {
        ByteSize(WORD_SIZE *  self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pointer(AddrNr);

impl Sub for Pointer {
    type Output = ByteSize;

    fn sub(self, rhs: Self) -> Self::Output {
        ByteSize(rhs.0 - self.0)
    }
}

impl Mul<u32> for ByteSize {
    type Output = ByteSize;

    fn mul(self, rhs: u32) -> Self::Output {
        ByteSize(self.0 * rhs)
    }
}

impl Add<ByteSize> for Pointer {
    type Output = Pointer;

    fn add(self, rhs: ByteSize) -> Self::Output {
        Pointer(self.0 + rhs.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Side { Left, Right }

thread_local! {
    static GC_CONF: RefCell<GcConf> = {
        let stack_capacity = WordSize(1024);
        let young_side_capacity = WordSize(16384);
        let old_capacity = WordSize(16384);
        RefCell::new(GcConf {
            stack_capacity,
            young_side_capacity,
            old_capacity,
        })
    };
}  //TODO @mark: remove if possible
thread_local! {
    static GC_STATE: RefCell<GcState> = {
        GC_CONF.with_borrow(|conf|
            RefCell::new(GcState {
                stack_top: conf.stack_start(),
                young_side: Side::Left,
                young_top: conf.young_side_start(),
                old_top: conf.old_start(),
            })
        )
    };
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
