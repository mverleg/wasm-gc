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

thread_local! {
    static STATE: RefCell<GcState> = RefCell::new(GcState {

    });
}

fn alloc_heap() {}

fn alloc0_heap() {}

fn alloc_stack() {}

fn alloc0_stack() {}


