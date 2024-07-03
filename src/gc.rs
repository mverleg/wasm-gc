#![allow(unused)]  //TODO @mark:

use ::std::cell::RefCell;
use ::std::fmt;
use ::std::fmt::Formatter;
use ::std::mem::size_of;
use ::std::ops::Add;
use ::std::ops::Index;
use ::std::ops::IndexMut;
use ::std::ops::Mul;
use ::std::ops::Sub;
use ::std::io::SeekFrom::Start;
use ::std::ops::Range;

type Nr = i32;

const WORD_SIZE: ByteSize = ByteSize(4);
const STRUCT_BYTE: u8 = 1;
const GC_REACHABLE_FLAG_BIT: u8 = 0;
const POINTER_MUTABLE_FLAG_BIT: u8 = 1;

// TODO how to handle 0-byte allocations? is there reference equality anywhere?
// TODO have some post-GC handler?
// TODO we need to read headers from end (following roots) and from start (compacting old heap), but they are variable length, so must be able to know the length from first and from last byte
//   TODO ^ would it be easier to just return pointer to second word, and e.g. put array length there?

#[derive(Debug)]
struct StackHeader {
    data_kind: DataKind,
    pointer_cnt: WordSize,
    size_32: WordSize,
    //TODO @mark: might be more efficient to store pointer cnt and total size; fewer additions - however it also limits total fields to 256 instead of just pointers or just data
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum HeaderEnc { Small(Nr), Big(Nr, Nr) }

impl HeaderEnc {
    fn of_struct(flags: u8, pointer_cnt: WordSize, size_32: WordSize) -> Self {
        debug_assert!(pointer_cnt <= size_32, "pointer size cannot exceed total size");
        let size_32_u8: u8 = size_32.0.try_into().unwrap_or_else(|_| panic!("struct too large: {size_32}"));
        let pointer_cnt_u8: u8 = pointer_cnt.0.try_into().expect("should never be reached since pointer <= total");
        HeaderEnc::Small(i32::from_le_bytes([
            STRUCT_BYTE,
            flags,
            pointer_cnt_u8,
            size_32_u8,
        ]))
    }

    fn decode_struct(self, data: Nr) -> (u8, WordSize, WordSize) {
        let [typ, flags, pointer_cnt_u8, size_32_u8] = data.to_le_bytes();
        debug_assert!(typ == STRUCT_BYTE);
        (flags, WordSize(pointer_cnt_u8.into()), WordSize(size_32_u8.into()))
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

fn mark_reachable(header: &mut Nr) {
    //TODO @mark: could probably mutate i32 directly
    let [typ, mut flags, other1, other2] = header.to_le_bytes();
    debug_assert!(typ == STRUCT_BYTE);
    flags |= mask(true, GC_REACHABLE_FLAG_BIT);
    *header = Nr::from_le_bytes([
        STRUCT_BYTE,
        flags,
        other1,
        other2,
    ])

}

impl StackHeader {
    fn encode(self) -> HeaderEnc {
        let flags: u8 = 0;
        HeaderEnc::of_struct(flags, self.pointer_cnt, self.size_32)
    }

    fn decode(data: Nr) -> Self {
        let (flags, pointer_cnt, size_32) = HeaderEnc::Small(data).decode_struct(data);
        StackHeader {
            data_kind: DataKind::Struct,
            pointer_cnt,
            size_32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DataKind { Struct, Array, Forward }
//TODO @mark: dynamic dispatch?
//TODO @mark: special kind for structs with more than 256 fields, and arrays of the same?

#[derive(Debug)]
struct YoungHeapHeader {
    data_kind: DataKind,
    pointers_mutable: bool,
    pointer_cnt: WordSize,
    size_32: WordSize,
}

const fn mask(is_on: bool, ix: u8) -> u8 {
    assert!(ix < 8);
    if !is_on {
        return 0;
    } else {
        1 << ix
    }
}

impl YoungHeapHeader {
    fn encode(self) -> HeaderEnc {
        assert!(self.pointer_cnt.0 > 0 || !self.pointers_mutable);
        // let flags: u8 = mask(self.gc_reachable, GC_REACHABLE_FLAG_BIT) &
        //     mask(self.pointers_mutable, POINTER_MUTABLE_FLAG_BIT);
        let flags: u8 = mask(self.pointers_mutable, POINTER_MUTABLE_FLAG_BIT);
        HeaderEnc::of_struct(flags, self.pointer_cnt, self.size_32)
    }

    fn decode(data: Nr) -> Self {
        let (flags, pointer_cnt, size_32) = HeaderEnc::Small(data).decode_struct(data);
        YoungHeapHeader {
            data_kind: DataKind::Struct,
            pointers_mutable: false,
            pointer_cnt,
            size_32,
        }
    }
}

#[derive(Debug)]
struct OldHeapHeader {}

impl OldHeapHeader {
    fn encode(self) -> HeaderEnc {
        unimplemented!()  //TODO @mark: use u32 instead of Addr?
    }
}

const OFFSET: Pointer = Pointer((size_of::<GcConf>() + size_of::<GcState>()) as Nr + WORD_SIZE.0);

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
pub struct ByteSize(Nr);

impl ByteSize {
    fn whole_words(self) -> WordSize {
        debug_assert!(self.0 % 4 == 0);
        WordSize(self.0 / 4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct WordSize(Nr);

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
pub struct Pointer(Nr);

impl Pointer {
    fn as_data(self) -> Nr {
        self.0
    }

    fn null() -> Self {
        return Pointer(0);
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

impl Mul<Nr> for ByteSize {
    type Output = ByteSize;

    fn mul(self, rhs: Nr) -> Self::Output {
        ByteSize(self.0 * rhs)
    }
}

impl Add<ByteSize> for Pointer {
    type Output = Pointer;

    fn add(self, rhs: ByteSize) -> Self::Output {
        Pointer(self.0 + rhs.0)
    }
}

impl Add<ByteSize> for ByteSize {
    type Output = ByteSize;

    fn add(self, rhs: ByteSize) -> Self::Output {
        ByteSize(self.0 + rhs.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Side { Left, Right }

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

struct Data {
    mem: Vec<Nr>,
}

impl Data {
    pub fn len(&self) -> WordSize {
        WordSize((self.mem.len() / 4).try_into().unwrap())
    }
}

impl Index<Pointer> for Data {
    type Output = Nr;

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
    size_32: WordSize,
    pointers_mutable: bool,
) -> Pointer {
    alloc0_heap(pointer_cnt, size_32, pointers_mutable)
        .expect("out of memory (heap)")
}

pub fn alloc0_heap(
    pointer_cnt: WordSize,
    size_32: WordSize,
    pointers_mutable: bool,
) -> Option<Pointer> {
    GC_STATE.with_borrow_mut(|state| {
        let young_side_end = GC_CONF.with_borrow(|conf|
            conf.young_side_end(state.young_side));
        DATA.with_borrow_mut(|data| {
            let p_init = state.young_top;
            let header = YoungHeapHeader {
                data_kind: DataKind::Struct,
                pointers_mutable,
                pointer_cnt,
                size_32,
            };
            let header_enc = header.encode();
            let p_return = p_init + header_enc.len();
            let p_end = p_return + size_32.bytes();
            if p_end > young_side_end {
                //TODO @mark: this should GC to cleanup / move to old heap
                println!("debug: young heap {:?} is full, {} > {}", state.young_side, p_end, young_side_end);
                return None;
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
    size_32: WordSize,
) -> Pointer {
    alloc0_stack(pointer_cnt, size_32)
        .expect("stack overflow")
}

//TODO @mark: maybe at least pointers should be initialized as 0? otherwise calling code must initialize all pointers before doing another alloc, lest it triggers GC
pub fn alloc0_stack(
    pointer_cnt: WordSize,
    size_32: WordSize,
) -> Option<Pointer> {
    GC_STATE.with_borrow_mut(|state| {
        let stack_end = GC_CONF.with_borrow_mut(|conf| conf.stack_end());
        DATA.with_borrow_mut(|data| {
            let p_init = state.stack_top_data;
            let header = StackHeader {
                data_kind: DataKind::Struct,
                pointer_cnt,
                size_32,
            };
            let header_enc = header.encode();
            let p_return = p_init + header_enc.len();
            let p_end = p_return + size_32.bytes();
            if p_end > stack_end {
                println!("debug: stack overflowed, {} > {}", p_end, stack_end);
                return None;
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

pub struct FastCollectStats {
    pub initial_young_capacity: WordSize,
    pub initial_young_len: WordSize,
    pub final_young_capacity: WordSize,
    pub final_young_len: WordSize,
    //TODO @mark: use ^
}

struct TaskStack {
    start: Pointer,
    top: Pointer,
}

//TODO @mark: not yet needed for young-only
impl TaskStack {
    fn new_empty_at(start: Pointer) -> Self {
        TaskStack { start, top: start }
    }

    /// push all pointers in one object before popping anything; this probably leads to higher
    /// stack size than DFS, but it means we only need to store header pointers, not field ones.
    fn push_all(young_only: bool) {
        todo!()
    }

    //TODO @mark: if mutable objects stay young forever, that means young heap can have objects older than old heap, which in turn means old heap can have references to young heap and we need to scan everything all the time
    fn push(young_only: bool) {
        todo!()
    }

    fn pop() {
        todo!()
    }
}

fn collect_fast_handle_pointer(data: &mut Data, pointer_ix: Pointer, young_from_range: Range<Pointer>, new_young_top: &mut Pointer) {
    // Stop if stack or old heap, or if already moved to opposite young heap side
    if !young_from_range.contains(&pointer_ix) {
        println!("not young heap {}, stop (not in range {:?})", pointer_ix, young_from_range);
        return;
    }

    // Stop if already moved
    todo!();

    // If old enough, move to old heap, and leave a pointer
    // TODO: not yet supported

    // Otherwise (if not old), move to other side of young heap, and leave a pointer
    todo!();
    // increase: new_young_top

    // We do not handle pointers directly, instead we'll do so while walking the young heap
}

pub fn collect_fast() -> FastCollectStats {
    GC_CONF.with_borrow(|conf| {
        GC_STATE.with_borrow_mut(|state| {
            DATA.with_borrow_mut(|data| {
                let young_from_range = conf.young_side_start(state.young_side) .. conf.young_side_end(state.young_side);
                let new_young_start =  conf.young_side_start(state.young_side.opposite());
                let mut new_young_top = new_young_start;
                let init_young_size = state.young_top - conf.young_side_start(state.young_side);

                // First walk the stack for roots
                let mut frame_start = state.stack_top_frame;
                let mut frame_after = state.stack_top_data;
                while frame_start != Pointer::null() {
                    println!("stack frame {}", frame_start);  //TODO @mark:
                    let mut header_ix = frame_start + WORD_SIZE;
                    while header_ix < frame_after {
                        let header = StackHeader::decode(data[header_ix]);
                        println!("stack object {}, header {:?}", header_ix, header);  //TODO @mark:
                        let mut pointer_ix = header_ix + WORD_SIZE;
                        let mut pointer_end = header.pointer_cnt.bytes() + WORD_SIZE;
                        while pointer_ix < header_ix + pointer_end {
                            println!("stack pointer {}", pointer_ix);  //TODO @mark:
                            pointer_ix = pointer_ix + WORD_SIZE;
                            collect_fast_handle_pointer(data, pointer_ix, young_from_range.clone(), &mut new_young_top);
                        }
                        header_ix = header_ix + header.size_32.bytes() + WORD_SIZE;
                    }
                    frame_after = frame_start;
                    frame_start = Pointer(data[frame_start]);
                }
                println!("stack END {}", frame_start);  //TODO @mark:

                // Having found all stack roots, handle the young heap by scanning flip side
                // Note that the young heap still grows (new_young_top)
                let mut header_ix = new_young_start;
                println!("young {:?} {} -> {} ({:?})", state.young_side.opposite(), header_ix, new_young_top, new_young_top - header_ix);  //TODO @mark:
                while header_ix < new_young_top {
                    let header = YoungHeapHeader::decode(data[header_ix]);
                    let mut pointer_ix = header_ix + WORD_SIZE;
                    let mut pointer_end = header.pointer_cnt.bytes() + WORD_SIZE;
                    while pointer_ix < header_ix + pointer_end {
                        pointer_ix = pointer_ix + WORD_SIZE;
                        collect_fast_handle_pointer(data, pointer_ix, young_from_range.clone(), &mut new_young_top);
                    }
                    header_ix = header_ix + header.size_32.bytes() + WORD_SIZE;
                }

                state.young_side = state.young_side.opposite();
                state.young_top = new_young_top;
                FastCollectStats {
                    initial_young_capacity: conf.young_side_capacity,
                    initial_young_len: init_young_size.whole_words(),
                    final_young_capacity: conf.young_side_capacity,
                    final_young_len: WordSize(0),
                }
            })
        })
    })
}

pub fn collect_full() {
    todo!();
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
mod tests {
    use super::*;

    const NO_WORDS: WordSize = WordSize(0);
    const ONE_WORD: WordSize = WordSize(1);
    const TWO_WORDS: WordSize = WordSize(2);
    const THREE_WORDS: WordSize = WordSize(3);

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

    fn fill_zeros(obj_addr: Pointer) -> Pointer {
        DATA.with_borrow_mut(|data| {
            let hdr = data[obj_addr - WORD_SIZE];
            let pointer_cnt: WordSize = read_pointer_cnt(hdr);
            let size_32: WordSize = read_data_size(hdr);
            let mut i = obj_addr;
            let end = obj_addr + size_32.bytes();
            while i < end {
                data[i] = 0;
                i = i + WORD_SIZE;
            }
        });
        obj_addr
    }

    fn read_pointer_cnt(header: Nr) -> WordSize {
        WordSize(header.to_le_bytes()[2] as Nr)
    }

    fn read_data_size(header: Nr) -> WordSize {
        WordSize(header.to_le_bytes()[3] as Nr)
    }

    #[test]
    fn alloc_heap_out_of_space() {
        reset();
        for _ in 0 .. 64 {
            let addr1 = alloc0_heap(WordSize(0), WordSize(255), false);
            assert!(addr1.is_some());
        }
        let addr2 = alloc0_heap(WordSize(0), WordSize(255), false);
        assert!(addr2.is_none());
    }

    #[test]
    fn alloc_stack_out_of_space() {
        reset();
        for _ in 0 .. 4 {
            let addr1 = alloc0_stack(WordSize(0), WordSize(255));
            assert!(addr1.is_some());
        }
        let addr2 = alloc0_stack(WordSize(0), WordSize(255));
        assert!(addr2.is_none());
    }

    #[test]
    fn alloc_data_on_heap() {
        reset();
        let orig = alloc_heap(ONE_WORD, THREE_WORDS, false);
        let subsequent = alloc_heap(TWO_WORDS, THREE_WORDS, false);
        DATA.with_borrow_mut(|data| assert_eq!(data[orig - WORD_SIZE], 0x03010001));
        assert_eq!(subsequent - orig, ByteSize(16));
        assert_eq!(young_heap_size(), WordSize(8));
        assert_eq!(stack_size(), NO_WORDS);
    }

    #[test]
    fn alloc_data_on_stack() {
        reset();
        stack_frame_push();
        let orig = alloc_stack(ONE_WORD, THREE_WORDS);
        stack_frame_push();
        let subsequent = alloc_stack(TWO_WORDS, THREE_WORDS);
        DATA.with_borrow_mut(|data| assert_eq!(data[orig - WORD_SIZE], 0x03010001));
        assert_eq!(subsequent - orig, WORD_SIZE * 5);
        assert_eq!(stack_size(), WordSize(1 + 1 + 3 + 1 + 1 + 3));
        stack_frame_pop();
        assert_eq!(stack_size(), WordSize(1 + 1 + 3));
        stack_frame_pop();
        assert_eq!(stack_size(), NO_WORDS);
        assert_eq!(young_heap_size(), NO_WORDS);
    }

    #[test]
    fn fast_gc_simple_referenced_young_value() {
        reset();
        let cap = GC_CONF.with_borrow(|conf| conf.young_side_capacity);
        // let orig = fill_zeros(alloc_heap(ONE_WORD, THREE_WORDS, false));
        stack_frame_push();
        stack_frame_push();
        fill_zeros(alloc_stack(TWO_WORDS, THREE_WORDS));
        let stack = fill_zeros(alloc_stack(TWO_WORDS, THREE_WORDS));
        fill_zeros(alloc_stack(ONE_WORD, ONE_WORD));
        fill_zeros(alloc_heap(ONE_WORD, TWO_WORDS, false));
        let heap1_orig = fill_zeros(alloc_heap(NO_WORDS, ONE_WORD, false));
        let heap2_orig = fill_zeros(alloc_heap(NO_WORDS, ONE_WORD, false));
        fill_zeros(alloc_heap(ONE_WORD, TWO_WORDS, false));
        DATA.with_borrow_mut(|data| {
            data[stack] = heap1_orig.0;
            data[stack + WORD_SIZE] = heap2_orig.0;
            data[stack + WORD_SIZE * 2] = 333_333;
            data[heap1_orig] = 444_444;
            data[heap2_orig] = 555_555;
        });
        print_memory();  //TODO @mark: TEMPORARY! REMOVE THIS!
        assert_eq!(stack_size(), WordSize(12));
        assert_eq!(young_heap_size(), WordSize(10));

    }

    #[test]
    fn fast_gc_cleans_young_if_unreferenced() {
        reset();
        let cap = GC_CONF.with_borrow(|conf| conf.young_side_capacity);
        let orig = fill_zeros(alloc_heap(ONE_WORD, THREE_WORDS, false));
        stack_frame_push();
        fill_zeros(alloc_stack(ONE_WORD, THREE_WORDS));
        fill_zeros(alloc_stack(NO_WORDS, ONE_WORD));
        stack_frame_push();
        fill_zeros(alloc_stack(TWO_WORDS, THREE_WORDS));
        fill_zeros(alloc_heap(TWO_WORDS, THREE_WORDS, true));
        assert_eq!(young_heap_size(), WordSize(8));
        assert_eq!(stack_size(), WordSize(12));
        print_memory();  //TODO @mark: TEMPORARY! REMOVE THIS!
        let stats = collect_fast();
        assert_eq!(young_heap_size(), NO_WORDS);
        assert_eq!(stack_size(), WordSize(12));
        assert_eq!(stats.initial_young_capacity, cap);
        assert_eq!(stats.initial_young_len, WordSize(8));
        assert_eq!(stats.final_young_capacity, cap);
        assert_eq!(stats.final_young_len, NO_WORDS);
        let swap = fill_zeros(alloc_heap(ONE_WORD, THREE_WORDS, false));
        let self1 = WordSize(100);
        assert!(swap - orig > ByteSize(500), "young sides do not look swapped");
    }

    #[test]
    fn fast_gc_mutable_data_ref_from_old() {
        reset();

        // allocate mutable data, and immutable heap data referencing it
        stack_frame_push();
        let heap_mut = fill_zeros(alloc_heap(ONE_WORD, ONE_WORD, true));
        let heap_immut = fill_zeros(alloc_heap(ONE_WORD, ONE_WORD, false));
        let stack_ref = fill_zeros(alloc_stack(ONE_WORD, ONE_WORD));
        DATA.with_borrow_mut(|data| {
            data[stack_ref] = heap_immut.0;
            data[heap_immut] = heap_mut.0;
        });

        // do a few GC rounds to move immutable data to old heap
        for _ in 0 .. 20 {
            collect_fast();
        }
        DATA.with_borrow_mut(|data| {
            assert_ne!(heap_immut.0, data[stack_ref], "immutable not moved from young to old");
            let mut_addr = Pointer(data[stack_ref]);
            assert_eq!(heap_mut.0, data[mut_addr], "mutable data moved, it should stay young");
        });

        // The problem to test for is this: if collect_fast does not scan old heap,
        // then it does not see the pointer to heap_mut, and that gets collected.
        assert_ne!(young_heap_size(), NO_WORDS, "mutable data got collected but was reachable");
        assert_eq!(young_heap_size(), TWO_WORDS, "young size incorrect");
    }

    #[test]
    fn fast_gc_young_data_ref_from_old_mutable() {
        todo!()  //TODO @mark:
    }

    #[ignore]
    #[test]
    fn maximum_heap_depth_gc() {
        todo!("test case where te whole heap is a single linked list, for maximum scan depth")  //TODO @mark:
    }
}
