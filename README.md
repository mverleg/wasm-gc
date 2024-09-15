
# Wasm Garbage Collector (GC)

Implement an allocator and garbage collector in (Rust compiled to) WebAssembly.

**Status: only young generation is usable, no arrays**

This GC is optimized for the [Tel](https://github.com/mverleg/tel) language, and makes some assumptions for that:

* Separate heap per thread.
* It's known whether a reference is (shallowly) mutable, and ideally most long-lived memory is immutable.
* Like most garbage collectors, works best if most memory is short-lived.
* If there is a reference to a field, there is also a reference to the object itself, to prevent it being GC'ed (probably references to fields aren't a language construct, but just an impl detail, so this can be ensured).

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

Having two young sides adds 50% overhead, but makes it fast to clean: we can move data it to the other side in a compacted way as soon as it is encountered, replacing it by a pointer to update references as they are encountered. Dead memory is never touched. Objects are in tree walking order, they don't preserve order.

The old regions are slower to collect. We must first mark every object starting from the roots. Then we have two choices:
- We create a break table for live memory, preserving memory order. We can use the break table to do compacting moves without touching dead memory, and re-scan from roots to update all pointers using the break table. Break table needs memory, and takes O(n log n) to sort.
- We scan the old heap (including dead memory) to put new addresses in object headers. We re-scan from roots to update all pointers, then we re-scan the old heap (including dead memory) to perform the compacting moves. We need 1 word overhead per object, no sorting needed, but needs to scan full heap twice (instead of live heap once).

The meta region can share space with the old region, triggering GC when they grow towards eachother. The meta size to guarantee it won't run out of memory must include:
- A stack or queue or similar to track work in the root scanning. If we do DFS the max size is the object graph depth, which may be as high as the number of objects (dead or alive) in all heap regions. This happens for one huge singly-linked-list, which we cannot rule out, but it's very unlikely.
- The break table for the old heap, if applicable. This is as large as the number of objects (dead or alive) in the current old heap region. We're likely to reach a substantial fraction of this in practise.
- A few words for 'state', such as current region boundaries, since they can be resized.

Since the young region is compacted, memory locality is good, and allocations are very fast unless OOM (simple bump). Since old regions are also compacted, they also have good locality, no fragmentation, and moving young data to old is fast.

## Questions

- What if an old region runs out of space during small GC? Or big?
- How can regions be resized? Both bigger and smaller
- Should roots be scanned depth-first or breadth-first, or something else? Depth is probably shallower. Scanning whole objects at a time means only storing 1 return pointer, without offset.

## Build locally

To convert text to binary (uses [wabt](https://github.com/webassembly/wabt)):

```shell
wat2wasm --debug-names gc.wat -o gc.wasm
```

