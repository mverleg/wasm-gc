use ::wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn allocate(byte_cnt: usize) -> usize {
    panic!("alloc: out of memory")
}

fn deallocate(addr: usize) {
    unimplemented!()
}

#[wasm_bindgen]
pub fn run_full_gc() {

}

#[wasm_bindgen]
pub fn run_young_gc() {

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let addr = allocate(4);
        deallocate(addr);
    }
}
