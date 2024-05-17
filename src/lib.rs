use ::wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn allocate(byte_cnt: usize) -> usize {
    panic!("alloc: out of memory")
}

#[wasm_bindgen]
pub fn deallocate(addr: usize) {
    unimplemented!()
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
