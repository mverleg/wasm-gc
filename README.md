
# Wasm Garbage Collector (GC)

Implement an allocator and garbage collector in WebAssembly (wasm).

Not finished yet. Not really designed for reusability.

## Design (tentative)

Memory is arranged into various regions:

- Wasm stack - this is outside wasm linear memory, and we cannot scan it for roots so cannot contain heap pointers.
- Shadow stack - we keep this inside linear memory, for any pointers and dynamically sized objects.
- Two young regions - all heap memory starts in the active half of this. During every GC, all reachable young memory moves to the other half if young, or to the old regions, and active half is swapped. Objects here are marked as having or not having mutable pointers. 
- Mutable old region - this is the old heap for mutable memory. During every GC, this region is scanned for roots, but during small GC it is assumed everything here is reachable.
- Immutable old region - this is the old heap for immutable memory. During small GC this is ignored, it is only scanned during large GC.
- GC metadata region - this contains metadata for use during GC.

Because everything that survives the young region stays for the same number of cycles, we can assume that the immutable old region cannot reference the young region, because the young memory didn't exist when those objects were created.

The mutable old region is needed because it is different from both others. Because it is mutable, it can reference data newer than itself, so must be scanned every GC. But it must not stay in the young region forever, because then old region memory may be younger than young region and have pointers to it.

Having two young sides adds 50% overhead, but makes it fast to clean: we can move data it to the other side in a compacted way as soon as it is encountered, replacing it by a pointer to update references as they are encountered. Dead memory is never touched. Objects preserve their memory order, so old objects are at the start (this provides little benefit).

The old regions are slower to collect. We must first mark every object starting from the roots. Then we have two choices:
- We create a relocation table for live memory, preserving memory order. We can scan the old heap (including dead memory) once to do compacting moves, and re-scan from roots to update all pointers using the relocation table.
- We scan the old heap (including dead memory) to put new addresses in object headers. We re-scan from roots to update all pointers, then we re-scan the old heap (including dead memory) to perform the compacting moves.

Since the young region is compacted, memory locality is good, and allocations are very fast unless OOM (simple bump). Since old regions are also compacted, they also have good locality, no fragmentation, and moving young data to old is fast.

## Build locally

To convert text to binary (uses [wabt](https://github.com/webassembly/wabt)):

```shell
wat2wasm --debug-names gc.wat -o gc.wasm
```

