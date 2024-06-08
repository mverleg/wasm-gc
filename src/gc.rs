use ::std::cell::RefCell;
use ::std::mem::size_of;
use ::std::ops::Add;
use ::std::ops::Mul;
use ::std::ops::Sub;
use ::std::ops::Index;
use ::std::ops::IndexMut;

type AddrNr = i32;

const WORD_SIZE: ByteSize = ByteSize(4);
const STRUCT_BYTE: u8 = 1;

#[derive(Debug)]
struct StackHeader {}

#[derive(Debug, Clone, Copy, PartialEq)]
enum HeaderEnc { Small(AddrNr), Big(AddrNr, AddrNr) }

impl StackHeader {
    fn encode(self) -> HeaderEnc {
        unimplemented!()  //TODO @mark: use u32 instead of Addr?
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DataKind { Struct, Array, Forward }
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
        let pointer_cnt_u8: u8 = self.pointer_cnt.0.try_into().unwrap();
        let data_size_32_u8: u8 = self.data_size_32.0.try_into().unwrap();
        let flags: u8 = mask(self.gc_reachable, 0) & mask(self.pointers_mutable, 1);
        match self.data_kind {
            DataKind::Struct => HeaderEnc::Small(i32::from_le_bytes([
                STRUCT_BYTE,
                flags,
                pointer_cnt_u8,
                data_size_32_u8,
            ])),
            DataKind::Array => unimplemented!(),
            DataKind::Forward => unimplemented!(),
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

    fn young_side_start(&self) -> Pointer {
        self.stack_start() + self.stack_capacity.bytes()
    }

    fn old_start(&self) -> Pointer {
        self.young_side_start() + self.young_side_capacity.bytes() * 2
    }

    fn end_of_memory(&self) -> Pointer {
        self.old_start() + self.old_capacity.bytes()
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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct ByteSize(AddrNr);

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct WordSize(AddrNr);

impl WordSize {
    fn bytes(self) -> ByteSize {
        ByteSize(WORD_SIZE.0 * self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct Pointer(AddrNr);

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

impl Index<Pointer> for Data {
    type Output = AddrNr;

    fn index(&self, index: Pointer) -> &Self::Output {
        &self.mem[index.0 as usize]
    }
}

impl IndexMut<Pointer> for Data {
    fn index_mut(&mut self, index: Pointer) -> &mut Self::Output {
        &mut self.mem[index.0 as usize]
    }
}

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
    static DATA: RefCell<Data> = {
        GC_CONF.with_borrow(|conf|
            RefCell::new(Data { mem: vec![0; conf.end_of_memory().0 as usize] })
        )
    };
}

pub fn alloc_heap(
    pointer_cnt: WordSize,
    data_size_32: WordSize,
    pointers_mutable: bool,
) -> Pointer {
    GC_STATE.with_borrow_mut(|state| {
        DATA.with_borrow_mut(|data| {
            let p_init = state.young_top;
            let header = YoungHeapHeader {
                data_kind: DataKind::Struct,
                gc_reachable: false,
                pointers_mutable,
                pointer_cnt,
                data_size_32,
            };
            let p_return = match header.encode() {
                HeaderEnc::Small(w) => {
                    data[p_init] = w;
                    p_init + WORD_SIZE
                }
                HeaderEnc::Big(w1, w2) => {
                    data[p_init] = w1;
                    data[p_init + WORD_SIZE] = w2;
                    p_init + WORD_SIZE * 2
                }
            };
            let p_end = p_return + pointer_cnt.bytes() + data_size_32.bytes();
            state.young_top = p_end;
            println!("{p_init:?} , {p_return:?} , {p_end:?}"); //TODO @mark: TEMPORARY! REMOVE THIS!
            debug_assert!(p_end > p_return);
            debug_assert!(p_return > p_init);
            p_return
        })
    })
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


#[test]
fn alloc_data_on_heap() {
    let orig = alloc_heap(WordSize(1), WordSize(2), false);
    let subsequent = alloc_heap(WordSize(2), WordSize(1), false);
    DATA.with_borrow_mut(|data| assert_eq!(data[orig - WORD_SIZE], 0x02010001));
    assert_eq!(subsequent - orig, ByteSize(16));
}
