use ::std::cell::RefCell;
use ::std::fmt;
use ::std::fmt::Formatter;
use ::std::mem::size_of;
use ::std::ops::Add;
use ::std::ops::Index;
use ::std::ops::IndexMut;
use ::std::ops::Mul;
use ::std::ops::Sub;

type AddrNr = i32;

const WORD_SIZE: ByteSize = ByteSize(4);
const STRUCT_BYTE: u8 = 1;

#[derive(Debug)]
struct StackHeader {
    data_kind: DataKind,
    pointer_cnt: WordSize,
    data_size_32: WordSize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum HeaderEnc { Small(AddrNr), Big(AddrNr, AddrNr) }

impl HeaderEnc {
    fn of_struct(flags: u8, pointer_cnt: WordSize, data_size_32: WordSize) -> Self {
        let pointer_cnt_u8: u8 = pointer_cnt.0.try_into().unwrap();
        let data_size_32_u8: u8 = data_size_32.0.try_into().unwrap();
        HeaderEnc::Small(i32::from_le_bytes([
            STRUCT_BYTE,
            flags,
            pointer_cnt_u8,
            data_size_32_u8,
        ]))
    }

    fn len(self) -> ByteSize {
        match self {
            HeaderEnc::Small(_) => WORD_SIZE,
            HeaderEnc::Big(_, _) => WORD_SIZE * 2,
        }
    }

    fn write_to(self, ix: Pointer, data: &mut Data) {
        match self {
            HeaderEnc::Small(w) => {
                data[ix] = w;
            }
            HeaderEnc::Big(w1, w2) => {
                data[ix] = w1;
                data[ix + WORD_SIZE] = w2;
            }
        };
    }
}

impl StackHeader {
    fn encode(self) -> HeaderEnc {
        let flags: u8 = 0;
        HeaderEnc::of_struct(flags, self.pointer_cnt, self.data_size_32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DataKind { Struct, Array, Forward }
//TODO @mark: dynamic dispatch?
//TODO @mark: special kind for structs with more than 256 fields, and arrays of the same?

#[derive(Debug)]
struct YoungHeapHeader {
    data_kind: DataKind,
    gc_reachable: bool,
    pointers_mutable: bool,
    pointer_cnt: WordSize,
    data_size_32: WordSize,
}

fn mask(is_on: bool, ix: u8) -> u8 {
    assert!(ix < 8);
    if ! is_on {
        return 0
    } else {
        1 << ix
    }
}

impl YoungHeapHeader {
    fn encode(self) -> HeaderEnc {
        assert!(self.pointer_cnt.0 > 0 || !self.pointers_mutable);
        let flags: u8 = mask(self.gc_reachable, 0) & mask(self.pointers_mutable, 1);
        HeaderEnc::of_struct(flags, self.pointer_cnt, self.data_size_32)
    }
}

#[derive(Debug)]
struct OldHeapHeader {}

impl OldHeapHeader {
    fn encode(self) -> HeaderEnc {
        unimplemented!()  //TODO @mark: use u32 instead of Addr?
    }
}

const OFFSET: Pointer = Pointer((size_of::<GcConf>() + size_of::<GcState>()) as AddrNr + WORD_SIZE.0);

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

    fn stack_end(&self) -> Pointer {
        self.stack_start() + self.stack_capacity.bytes()
    }

    fn young_overall_start(&self) -> Pointer {
        self.stack_start() + self.stack_capacity.bytes()
    }

    fn young_side_start(&self, side: Side) -> Pointer {
        match side {
            Side::Left => self.young_overall_start(),
            Side::Right => self.young_overall_start() + self.young_side_capacity.bytes(),
        }
    }

    fn young_side_end(&self, side: Side) -> Pointer {
        self.young_side_start(side) + self.young_side_capacity.bytes()
    }

    fn old_start(&self) -> Pointer {
        self.young_overall_start() + self.young_side_capacity.bytes() * 2
    }

    fn old_end(&self) -> Pointer {
        self.old_start() + self.old_capacity.bytes()
    }

    fn end_of_memory(&self) -> Pointer {
        self.old_end()
    }
}

#[derive(Debug)]
struct GcState {
    stack_top_frame: Pointer,
    stack_top_data: Pointer,
    young_side: Side,
    young_top: Pointer,
    old_top: Pointer,
}

impl GcState {
    fn stack_len(&self, conf: &GcConf) -> WordSize {
        (self.stack_top_data - conf.stack_start()).whole_words()
    }

    fn young_len(&self, conf: &GcConf) -> WordSize {
        (self.young_top - conf.young_side_start(self.young_side)).whole_words()
    }

    fn old_len(&self) -> WordSize {
        unimplemented!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ByteSize(AddrNr);

impl ByteSize {
    fn whole_words(self) -> WordSize {
        debug_assert!(self.0 % 4 == 0);
        WordSize(self.0 / 4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct WordSize(AddrNr);

impl WordSize {
    fn bytes(self) -> ByteSize {
        ByteSize(WORD_SIZE.0 * self.0)
    }
}

impl fmt::Display for WordSize {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Pointer(AddrNr);

impl Pointer {
    fn as_data(self) -> AddrNr {
        self.0
    }

    fn null() -> Self {
        return Pointer(0)
    }

    fn aligned_down(self) -> Self {
        Pointer((self.0 / WORD_SIZE.0) * WORD_SIZE.0)
    }
}

impl fmt::Display for Pointer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "x{}", self.0)
    }
}

impl Sub<Pointer> for Pointer {
    type Output = ByteSize;

    fn sub(self, rhs: Self) -> Self::Output {
        ByteSize(self.0 - rhs.0)
    }
}

impl Sub<ByteSize> for Pointer {
    type Output = Pointer;

    fn sub(self, rhs: ByteSize) -> Self::Output {
        Pointer(self.0 - rhs.0)
    }
}

impl Mul<AddrNr> for ByteSize {
    type Output = ByteSize;

    fn mul(self, rhs: AddrNr) -> Self::Output {
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

struct Data {
    mem: Vec<AddrNr>,
}

impl Data {
    pub fn len(&self) -> WordSize {
        WordSize((self.mem.len() / 4).try_into().unwrap())
    }

}

impl Index<Pointer> for Data {
    type Output = AddrNr;

    fn index(&self, index: Pointer) -> &Self::Output {
        debug_assert!(index != Pointer::null(), "cannot read from null pointer");
        assert!(index.0 % WORD_SIZE.0 == 0, "unaligned read not impl yet (might not be needed even though wasm can do it)");
        &self.mem[(index.0 / WORD_SIZE.0) as usize]
    }
}

impl IndexMut<Pointer> for Data {
    fn index_mut(&mut self, index: Pointer) -> &mut Self::Output {
        debug_assert!(index != Pointer::null(), "cannot write to null pointer");
        assert!(index.0 % WORD_SIZE.0 == 0, "unaligned read not impl yet (might not be needed even though wasm can do it)");
        &mut self.mem[(index.0 / WORD_SIZE.0) as usize]
    }
}

thread_local! {
    static GC_CONF: RefCell<GcConf> =
        RefCell::new(GcConf {
            stack_capacity: WordSize(0),
            young_side_capacity: WordSize(0),
            old_capacity: WordSize(0),
        })
    ;
    static GC_STATE: RefCell<GcState> = {
        RefCell::new(GcState {
            stack_top_frame: Pointer(0),
            stack_top_data: Pointer(0),
            young_side: Side::Left,
            young_top: Pointer(0),
            old_top: Pointer(0),
        })
    };
    static DATA: RefCell<Data> = {
        RefCell::new(Data { mem: Vec::new() })
    };
}

pub fn alloc_heap(
    pointer_cnt: WordSize,
    data_size_32: WordSize,
    pointers_mutable: bool,
) -> Pointer {
    alloc0_heap(pointer_cnt, data_size_32, pointers_mutable)
        .expect("out of memory (heap)")
}

pub fn alloc0_heap(
    pointer_cnt: WordSize,
    data_size_32: WordSize,
    pointers_mutable: bool,
) -> Option<Pointer> {
    GC_STATE.with_borrow_mut(|state| {
        let young_side_end = GC_CONF.with_borrow(|conf|
            conf.young_side_end(state.young_side));
        DATA.with_borrow_mut(|data| {
            let p_init = state.young_top;
            let header = YoungHeapHeader {
                data_kind: DataKind::Struct,
                gc_reachable: false,
                pointers_mutable,
                pointer_cnt,
                data_size_32,
            };
            let header_enc = header.encode();
            let p_return = p_init + header_enc.len();
            let p_end = p_return + pointer_cnt.bytes() + data_size_32.bytes();
            if p_end > young_side_end {
                //TODO @mark: this should GC to cleanup / move to old heap
                println!("debug: young heap {:?} is full, {} > {}", state.young_side, p_end, young_side_end);
                return None
            }
            header_enc.write_to(p_init, data);
            state.young_top = p_end;
            debug_assert!(p_end > p_return);
            debug_assert!(p_return > p_init);
            Some(p_return)
        })
    })
}

pub fn alloc_stack(
    pointer_cnt: WordSize,
    data_size_32: WordSize,
) -> Pointer {
    alloc0_stack(pointer_cnt, data_size_32)
        .expect("stack overflow")
}

pub fn alloc0_stack(
    pointer_cnt: WordSize,
    data_size_32: WordSize,
) -> Option<Pointer> {
    GC_STATE.with_borrow_mut(|state| {
        let stack_end = GC_CONF.with_borrow_mut(|conf| conf.stack_end());
        DATA.with_borrow_mut(|data| {
            let p_init = state.stack_top_data;
            let header = StackHeader {
                data_kind: DataKind::Struct,
                pointer_cnt,
                data_size_32,
            };
            let header_enc = header.encode();
            let p_return = p_init + header_enc.len();
            let p_end = p_return + pointer_cnt.bytes() + data_size_32.bytes();
            if p_end > stack_end {
                println!("debug: stack overflowed, {} > {}", p_end, stack_end);
                return None
            }
            header_enc.write_to(p_init, data);
            state.stack_top_data = p_end;
            debug_assert!(p_end > p_return);
            debug_assert!(p_return > p_init);
            Some(p_return)
        })
    })
}

/// The first word of a stack frame is the address of the previous one (0x0 for bottom)
/// Note that it is _not_ assumed that stack frames have statically known size
pub fn stack_frame_push() {
    GC_STATE.with_borrow_mut(|state| {
        DATA.with_borrow_mut(|data| {
            data[state.stack_top_data] = state.stack_top_frame.as_data();
            state.stack_top_frame = state.stack_top_data;
            state.stack_top_data = state.stack_top_data + WORD_SIZE;
        });
    });
}

pub fn stack_frame_pop() {
    GC_STATE.with_borrow_mut(|state| {
        DATA.with_borrow_mut(|data| {
            let prev_frame = data[state.stack_top_frame];
            assert_ne!(state.stack_top_frame, Pointer::null(), "stack is empty, cannot pop frame");
            state.stack_top_data = state.stack_top_frame;
            state.stack_top_frame = Pointer(prev_frame);
        });
    });
}

pub fn young_heap_size() -> WordSize {
    GC_CONF.with_borrow(|conf| {
        GC_STATE.with_borrow(|state| {
            state.young_top - conf.young_side_start(state.young_side)
        })
    }).whole_words()
}

pub fn stack_size() -> WordSize {
    GC_CONF.with_borrow(|conf| {
        GC_STATE.with_borrow(|state| {
            state.stack_top_data - conf.stack_start()
        })
    }).whole_words()
}

#[cfg(test)]
fn reset() {
    GC_CONF.with_borrow_mut(|conf| *conf = GcConf {
        stack_capacity: WordSize(1024),
        young_side_capacity: WordSize(16384),
        old_capacity: WordSize(16384),
    });
    GC_CONF.with_borrow(|conf| {
        GC_STATE.with_borrow_mut(|state| *state = GcState {
            stack_top_frame: Pointer::null(),
            stack_top_data: conf.stack_start(),
            young_side: Side::Left,
            young_top: conf.young_side_start(Side::Left),
            old_top: conf.old_start(),
        });
        DATA.with_borrow_mut(|data| {
            if (Pointer::null() + data.len().bytes()) < conf.end_of_memory() {
                // in debug mode 0x0F0F0F0F, usually 0
                *data = Data { mem: vec![0x0F0F0F0F; conf.end_of_memory().0 as usize] };
            } else {
                data.mem.fill(0x0F0F0F0F);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print_memory() {
        fn print_4nrs(data: &Data, ix: Pointer) {
            let val = data[ix];
            if val == 252645135 {
                println!("{ix}:\tinit");
            } else {
                let bytes = i32::to_le_bytes(val);
                println!("{ix}:\t{}\t{}\t{}\t{}", bytes[0], bytes[1], bytes[2], bytes[3]);
            }
        }
        GC_CONF.with_borrow(|conf| {
            GC_STATE.with_borrow(|state| {
                DATA.with_borrow(|data| {
                    println!("stack ({} / {} words):", state.stack_len(conf), conf.stack_capacity);
                    let mut ws = conf.stack_start().aligned_down();
                    while ws < state.stack_top_data {
                        print_4nrs(data, ws);
                        ws = ws + WORD_SIZE;
                    }
                    println!("young heap ({:?}, {} / {} words):", state.young_side, state.young_len(conf), conf.young_side_capacity);
                    let mut ws = conf.young_side_start(state.young_side).aligned_down();
                    while ws < state.young_top {
                        print_4nrs(data, ws);
                        ws = ws + WORD_SIZE;
                    }
                });
            });
        });
    }

    #[test]
    fn alloc_data_on_heap() {
        reset();
        let orig = alloc_heap(WordSize(1), WordSize(2), false);
        let subsequent = alloc_heap(WordSize(2), WordSize(1), false);
        DATA.with_borrow_mut(|data| assert_eq!(data[orig - WORD_SIZE], 0x02010001));
        assert_eq!(subsequent - orig, ByteSize(16));
        assert_eq!(young_heap_size(), WordSize(8));
        assert_eq!(stack_size(), WordSize(0));
    }

    #[test]
    fn alloc_data_on_stack() {
        reset();
        stack_frame_push();
        let orig = alloc_stack(WordSize(1), WordSize(2));
        stack_frame_push();
        let subsequent = alloc_stack(WordSize(2), WordSize(1));
        DATA.with_borrow_mut(|data| assert_eq!(data[orig - WORD_SIZE], 0x02010001));
        assert_eq!(subsequent - orig, WORD_SIZE * 5);
        assert_eq!(stack_size(), WordSize(1 + 1 + 3 + 1 + 1 + 3));
        stack_frame_pop();
        assert_eq!(stack_size(), WordSize(1 + 1 + 3));
        stack_frame_pop();
        assert_eq!(stack_size(), WordSize(0));
        assert_eq!(young_heap_size(), WordSize(0));
    }
}