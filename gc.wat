;; This wasm GC makes some important assumptions:

;;TODO @mark: edit: also allocate stack values through this, those are used for roots

;; - allocations are N pointers followed by M bytes if non-pointer data
        ;;TODO @mark: how does this work with arrays? ^
;; - code only reads/writes allocated memory, and only while reachable from either roots or allocated pointers
;; - roots don't change during GC
;; - there is a single thread (or in the future perhaps one heap per thread)
;; - most data is immutable, and only mutable data can mutate
;;   (this is important for e.g. the assumption that old gen cannot point to young gen)
;; - all allocations are multiples of 32 bytes
;; - for now (can be lifted): at most 128 pointers and words of data
;; - objects do not know when they get GC'ed (no finalize/drop methods)
;;
;; priorities for the GC are:
;; - small size
;; - easy to configure
;; - decent performance in common cases
;;   performance encompasses everything: allocation, GC, memory locality, etc
;;TODO @mark: ^ is all this still correct?
;;
;; Layout (per thread, 1 for now):
;; - metadata:
;;   - 0: empty
;;   - 4: end of stack
;;   - 8: 0 if first half of young gen is active, 1 otherwise
;;   - 12: end of young gen active half
;;   - 16: end of old gen heap
;; - stack (partial one for things with pointers or dynamically-sized objects)
;; - young gen heap, x2 active and GC-target
;; - old gen heap
;;
;; Metadata:
;; - pointer cnt
;; - data word size
;; - is mutable
;; - reachable in current GC
;; - generation count
;; - is redirect in current GC
;; - is array? length?
;; Some of this is per-type instead of per-object, but might still be efficient to duplicate

;; TODO switch to globals https://augustus-pash.gitbook.io/wasm/types/globals
;; TODO how to find roots?
;; TODO how to handle arrays (detect pointers)
;; TODO how to handle 0-byte allocations? is there reference equality anywhere?
;; TODO have some post-GC handler?
;; TODO can the GC have its own stack without reusing or unwinding program stack?
;; TODO is BF search better because only stack memory (and less total?), or is DF better bc of memory locality?

(module
    (import "host" "log_i32" (func $log_i32 (param i32)))
    (import "host" "log_i32x4" (func $log_i32x4 (param i32) (param i32) (param i32) (param i32)))
    (import "host" "log_err_code" (func $log_err_code (param i32)))
    (memory 3 3)  ;; 2x 64k
    (func $alloc_init
        (i32.store (call $addr_stack_length) (i32.const 0))
        (i32.store (call $addr_young_side) (i32.const 0))
        (i32.store (call $addr_young_length) (i32.const 0))
        (i32.store (call $addr_old_length) (i32.const 0)))
    (start $alloc_init)

    ;; these are addresses (in bytes) but sizes at the addresses are in words
    (func $addr_stack_length (result i32) i32.const 4)
    (func $addr_young_side (result i32) i32.const 8)
    (func $addr_young_length (result i32) i32.const 12)
    (func $addr_old_length (result i32) i32.const 12)

    ;; max size is in words
    (func $const_stack_max_size (result i32) i32.const 1024)
    (func $const_young_side_max_size (result i32) i32.const 16384)
    (func $const_old_heap_max_size (result i32) i32.const 0)

    (func $glob_stack_start_addr (result i32) i32.const 20)
    (func $glob_young_start_addr (result i32) (local $res i32)
        ;; start of stack + length of stack + currently used stack space

        ;; (call $log_i32 (i32.const -1))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        ;; (call $log_i32 (call $glob_stack_start_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        ;; (call $log_i32 (i32.mul (i32.const 4) (call $const_stack_max_size)))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        ;; (call $log_i32 (i32.mul (i32.const 4) (i32.load (call $addr_young_length))))  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        (local.set $res (i32.add
            (call $glob_stack_start_addr)
            (i32.mul (i32.const 4) (i32.add
                (call $const_stack_max_size)
                (i32.load (call $addr_young_length))))))

        (if (i32.ne (i32.load (call $addr_young_side)) (i32.const 0)) (then
            ;; when using 'other half' of young, add one half's size
            ;; (call $log_i32 (i32.mul (i32.const 4) (call $const_young_side_max_size)))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
            (local.set $res (i32.add
                (i32.mul (i32.const 4) (call $const_young_side_max_size))
                (local.get $res)))
        ))
        ;; (call $log_i32 (i32.const -2))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        local.get $res
    )

    ;; default alloc, traps when OOM
    (func $alloc (export "alloc")
            (param $pointer_cnt i32)
            (param $data_size_32 i32)  ;; units are 32-bit words
            (param $is_mutable i32)
            (result i32)  ;; addr
            (local $res i32)
        (block $alloc_ok
            (local.set $res
                (call $alloc0
                    (local.get $pointer_cnt)
                    (local.get $data_size_32)
                    (local.get $is_mutable)))
            (i32.eq
                (local.get $res)
                (i32.const 0))
            br_if $alloc_ok
            (return (local.get $res))
        )
        ;; OOM (returned 0)
        (call $log_err_code (i32.const 1))
        unreachable
    )

    ;; like $alloc, but returns 0 when OOM, so user code can handle it
    (func $alloc0 (export "alloc0")
            (param $pointer_cnt i32)
            (param $data_size_32 i32)  ;; units are 32-bit words
            (param $is_mutable i32)
            (result i32)  ;; addr
            (local $alloc_size i32)
            (local $orig_young_length i32)
            (local $new_young_length i32)
            (local $orig_offset_addr i32)

        ;; mutable not supported yet
        (if (i32.ne (local.get $is_mutable) (i32.const 0)) (then
            (call $log_err_code (i32.const 2))
            unreachable
        ))
        ;; pointer_cnt not supported yet
        (if (i32.ne (local.get $pointer_cnt) (i32.const 0)) (then
            (call $log_err_code (i32.const 3))
            unreachable
        ))

        ;; calculate the necessary size (words) including metadata
        (local.set $alloc_size (i32.add (i32.const 1) (i32.add (local.get $pointer_cnt) (local.get $data_size_32))))
        ;;TODO @mark: for now assume metadata is 1 word ^

        ;; calculate new young heap size (but don't update yet)
        (local.set $orig_young_length (i32.load (call $addr_young_length)))
        (local.set $new_young_length (i32.add (local.get $orig_young_length) (local.get $alloc_size)))

        ;; check if enough memory
        (if (i32.gt_u (local.get $new_young_length) (call $const_young_side_max_size)) (then
            (return (i32.const 0)) ))

        ;; find current top of young heap addr
        (local.set $orig_offset_addr (i32.add
            (call $glob_young_start_addr)
            (i32.mul (i32.const 4) (local.get $orig_young_length))))

        ;; write metadata - just length for now
        (call $write_metadata
                (local.get $orig_offset_addr)
                (local.get $pointer_cnt)
                (local.get $data_size_32)
                (local.get $is_mutable))

        ;; update heap length
        (i32.store (call $addr_young_length) (local.get $new_young_length))

        ;; return data address, which is after metadata
        (return (i32.add (local.get $orig_offset_addr) (i32.const 4)))
    )

    ;; start a stack frame; can allocate with stack_alloc,
    ;; but only if doesn't live past stack_pop.
    (func $stack_push (export "stack_push")
        unreachable   ;;TODO @mark: TEMPORARY! REMOVE THIS!
    )

    ;; drop stack frame started with stack_alloc; assuming
    ;; all memory is unreferenced.
    (func $stack_pop (export "stack_pop")
        unreachable   ;;TODO @mark: TEMPORARY! REMOVE THIS!
    )

    ;; allocate memory on current stack frame; must be
    ;; unreferenced before stack_pop.
    ;; (it may be possible to group several objects into a
    ;; single allocation, but not all of them, due to
    ;; dynamically sized objects).
    (func $stack_alloc (export "stack_alloc")
            (param $pointer_cnt i32)
            (param $data_size_32 i32)  ;; units are 32-bit words
            (result i32)  ;; addr
        unreachable   ;;TODO @mark: TEMPORARY! REMOVE THIS!
    )

    ;; do a small GC, e.g. young generation only
    (func $gc_fast (export "gc_fast")
    )

    ;; do a big GC, e.g. check all memory regions
    (func $gc_full (export "gc_full")
    )

    (func $write_metadata
            (param $meta_addr i32)
            (param $pointer_cnt i32)
            (param $data_size_32 i32)
            (param $is_mutable i32)
        (if (i32.gt_u (local.get $pointer_cnt) (i32.const 127)) (then (call $log_err_code (i32.const 103)) unreachable ))
        (if (i32.gt_u (local.get $data_size_32) (i32.const 127)) (then (call $log_err_code (i32.const 104)) unreachable ))

        (i32.store8 (i32.add (local.get $meta_addr) (i32.const 2)) (local.get $pointer_cnt))
        (i32.store8 (i32.add (local.get $meta_addr) (i32.const 3)) (local.get $data_size_32))
    )

    (func $read_metadata_pointer_cnt
            (param $meta_addr i32)
            (result i32)
        (i32.load8_u (i32.add (local.get $meta_addr) (i32.const 2)))
    )

    (func $read_metadata_data_word_cnt
            (param $meta_addr i32)
            (result i32)
        (i32.load8_u (i32.add (local.get $meta_addr) (i32.const 3)))
    )

    ;;
    ;; some internals, perhaps mostly for testing, as they make it hard to change impl
    ;;

    (func $get_young_size
            (result i32)
        (i32.load (call $addr_young_length))
    )

    (func $print_memory
        call $print_stack
        call $print_heap
    )

    (func $print_stack
            (local $i i32)
            (local $upto i32)
        ;;TODO @mark:
    )

    (func $print_heap
            (local $i i32)
            (local $upto i32)
        (local.set $upto (i32.load (call $addr_young_length)))
        (local.set $i (i32.const 0))
        (block $outer (loop $continue
            (i32.ge_u (local.get $i) (local.get $upto))
            br_if $outer
            (call $log_i32x4
                    (i32.load8_u (i32.add (local.get $i) (i32.const 0)))
                    (i32.load8_u (i32.add (local.get $i) (i32.const 1)))
                    (i32.load8_u (i32.add (local.get $i) (i32.const 2)))
                    (i32.load8_u (i32.add (local.get $i) (i32.const 3))))
            (local.set $i (i32.add (local.get $i) (i32.const 4)))
            (br $continue)
        ))
    )

    ;;
    ;; TESTS
    ;;

    (func $gc_tests (export "tests")
        (call $test_empty_heap)

        (call $test_double_data_alloc)
        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $alloc_init)  ;; reset heap

        (call $test_alloc_full_heap_GC)
        (call $alloc_init)  ;; reset heap
    )

    (func $test_empty_heap
        (call $alloc_init)  ;; reset heap
        (if (i32.ne (call $get_young_size) (i32.const 0))
            (then (call $log_err_code (i32.const 107)) unreachable))
    )

    (func $test_double_data_alloc

        ;; first allocation
        (call $log_i32 (call $alloc (i32.const 0) (i32.const 2) (i32.const 0)))
        (if (i32.ne (call $get_young_size) (i32.const 3)) (then
            (call $log_err_code (i32.const 100))
            unreachable
        ))

        ;; what if we do it again
        (call $log_i32 (call $alloc (i32.const 0) (i32.const 1) (i32.const 0)))
        (if (i32.ne (call $get_young_size) (i32.const 5)) (then
            (call $log_err_code (i32.const 101))
            unreachable
        ))
    )

    (func $test_alloc_full_heap_GC
            (local $i i32)

        ;; fill almost all memory
        (local.set $i (i32.const 0))
        (block $outer (loop $continue
            (i32.ge_u (local.get $i) (i32.const 32))
            br_if $outer
            ;;TODO @mark: TEMPORARY! REMOVE THIS!(call $log_err_code (local.get $i))
            (drop (call $alloc (i32.const 0) (i32.const 127) (i32.const 0)))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            br $continue
        ))

        ;; test that alloc fails
        (call $alloc0 (i32.const 0) (i32.const 127) (i32.const 0))
        (if (i32.ne (i32.const 0)) (then
            (call $log_err_code (i32.const 105)) unreachable))

        ;; test that GC cleans memory
        call $gc_full
        call $alloc_init  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (if (i32.ne (call $get_young_size) (i32.const 0)) (then
            (call $log_err_code (i32.const 106)) unreachable))
    )
)
