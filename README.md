
# Wasm Garbage Collector (GC)

Implement an allocator and garbage collector in WebAssembly (wasm).

Not finished yet. Not really designed for reusability.


## Build locally

To convert text to binary (uses [wabt](https://github.com/webassembly/wabt)):

```shell
wat2wasm --debug-names gc.wat -o gc.wasm
```

