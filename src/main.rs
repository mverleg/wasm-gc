use ::std::env;
use ::std::fs;

use ::wat;

use ::wasmer::Cranelift;
use ::wasmer::Function as HostFunction;
use ::wasmer::Imports;
use ::wasmer::Instance;
use ::wasmer::Module;
use ::wasmer::Store;
use ::wasmer::sys::EngineBuilder;
use ::wasmer::sys::Features;
use ::wasmer::Value;

fn main() {
    run(env::args().skip(1).next())
}

fn run(wat_file: Option<String>) {
    let mut prog = WasmProg::load(&wat_file.unwrap_or_else(|| "main.wat".to_owned()));
    prog.run("tests", &[]);
    println!("done")
}

fn log_i32(nr: i32) {
    println!("log_i32: {nr}")
}

fn log_i32x4(a: i32, b: i32, c: i32, d: i32) {
    println!("\t{a}\t{b}\t{c}\t{d}")
}

fn log_err_code(nr: i32) {
    println!("errcode: {nr}")
}

struct WasmProg {
    name: String,
    store: Store,
    instance: Instance,
}

impl WasmProg {
    fn load(wat_pth: &str) -> Self {
        // based on try-wasm-gen repo

        let wat_data = fs::read_to_string(&wat_pth)
            .unwrap_or_else(|err| panic!("could not read wat file '{}' (cli arg), error: {err}", &wat_pth));
        let wasm_code = wat::parse_str(wat_data).unwrap();

        let mut features = Features::new();
        features.multi_memory(true).tail_call(true);
        let engine = EngineBuilder::new(Cranelift::new())
            .set_features(Some(features));
        let mut store = Store::new(engine);
        let module = Module::from_binary(&store, &wasm_code).unwrap();

        let mut imports = Imports::new();
        imports.define("host", "log_i32", HostFunction::new_typed(&mut store, log_i32));
        imports.define("host", "log_i32x4", HostFunction::new_typed(&mut store, log_i32x4));
        imports.define("host", "log_err_code", HostFunction::new_typed(&mut store, log_err_code));
        let instance = Instance::new(&mut store, &module, &imports).unwrap();
        
        WasmProg {
            name: wat_pth.to_owned(),
            store,
            instance,
        }
    }

    fn run(&mut self, func: &str, args: &[Value]) -> Box<[Value]> {
        self.instance.exports.get_function(func)
            .unwrap_or_else(|err| panic!("could not find {func} in wasm module {}, err: {}", &self.name, &err))
            .call(&mut self.store, args)
            .unwrap_or_else(|err| panic!("could not execute {func} in wasm module {}, err: {}", &self.name, &err))
    }
}
