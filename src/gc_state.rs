#![allow(unused)]  //TODO @mark:

use ::std::cell::RefCell;
use ::std::mem::size_of;
use ::std::ops::Index;
use ::std::ops::IndexMut;

use crate::gc::ByteSize;
use crate::gc::Pointer;
use crate::gc::WordSize;

pub type Nr = i32;

pub const WORD_SIZE: ByteSize = ByteSize(4);

const OFFSET: Pointer = Pointer((size_of::<GcConf>() + size_of::<GcState>()) as Nr + WORD_SIZE.0);

#[derive(Debug)]
pub struct GcConf {
    pub stack_capacity: WordSize,
    pub young_side_capacity: WordSize,
    pub old_capacity: WordSize,
}

impl GcConf {
    pub fn stack_start(&self) -> Pointer {
        OFFSET
    }

    pub(crate) fn stack_end(&self) -> Pointer {
        self.stack_start() + self.stack_capacity.bytes()
    }

    fn young_overall_start(&self) -> Pointer {
        self.stack_start() + self.stack_capacity.bytes()
    }

    pub fn young_side_start(&self, side: Side) -> Pointer {
        match side {
            Side::Left => self.young_overall_start(),
            Side::Right => self.young_overall_start() + self.young_side_capacity.bytes(),
        }
    }

    pub fn young_side_end(&self, side: Side) -> Pointer {
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
pub struct GcState {
    stack_top_frame: Pointer,
    pub stack_top_data: Pointer,
    pub young_side: Side,
    pub young_top: Pointer,
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
    pub static GC_CONF: RefCell<GcConf> =
        RefCell::new(GcConf {
            stack_capacity: WordSize(0),
            young_side_capacity: WordSize(0),
            old_capacity: WordSize(0),
        })
    ;
    pub static GC_STATE: RefCell<GcState> = {
        RefCell::new(GcState {
            stack_top_frame: Pointer(0),
            stack_top_data: Pointer(0),
            young_side: Side::Left,
            young_top: Pointer(0),
            old_top: Pointer(0),
        })
    };
    pub static DATA: RefCell<Data> = {
        RefCell::new(Data { mem: Vec::new() })
    };
}