use std::{env, fs};
use ::wabt::Wat2Wasm;

fn main() {
    run(env::args().next())
}

fn run(wat_file: Option<String>) {
    let wat_pth = wat_file
        .unwrap_or_else(|| "main.wat".to_owned());
    let wat_data = match fs::read_to_string(wat_pth) {
        Ok(data) => data,
        Err(err) => panic!("could not read wat file '{}', error: {err}", &wat_pth),
    };
    let wasm_binary = Wat2Wasm::new()
        .canonicalize_lebs(false)
        .write_debug_names(true)
        .convert(wat_data).unwrap();

}
