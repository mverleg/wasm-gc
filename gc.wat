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
;; - is array? length?
;; - only for heap, not stack:
;;   - are the pointers mutable
;;   - reachable in current GC
;;   - generation count
;;   - is redirect in current GC
;; Some of this is per-type instead of per-object, but might still be efficient to duplicate

(module
    (import "host" "log_i32" (func $log_i32 (param i32)))
    (import "host" "log_i32x7" (func $log_i32x7 (param i32) (param i32) (param i32) (param i32) (param i32) (param i32) (param i32)))
    (import "host" "log_nl" (func $log_nl))
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

        (local.set $res (i32.add
            (call $glob_stack_start_addr)
            (i32.mul (i32.const 4) (call $const_stack_max_size))))

        (if (i32.ne (i32.load (call $addr_young_side)) (i32.const 0)) (then
            ;; when using 'other half' of young, add one half's size
            (local.set $res (i32.add
                (i32.mul (i32.const 4) (call $const_young_side_max_size))
                (local.get $res)))
        ))
        local.get $res
    )

    ;; default alloc, traps when OOM
    (func $alloc (export "alloc")
            (param $pointer_cnt i32)
            (param $data_size_32 i32)  ;; units are 32-bit words
            (param $pointers_mutable i32)
            (result i32)  ;; addr
            (local $res i32)

        (local.set $res (call $alloc0 (local.get $pointer_cnt) (local.get $data_size_32) (local.get $pointers_mutable)))
        (if (i32.eq (local.get $res) (i32.const 0)) (then
            (call $log_err_code (i32.const 1))
            unreachable
        ))
        local.get $res
    )

    ;; like $alloc, but returns 0 when OOM, so user code can handle it
    (func $alloc0 (export "alloc0")
            (param $pointer_cnt i32)
            (param $data_size_32 i32)  ;; units are 32-bit words
            (param $pointers_mutable i32)
            (result i32)  ;; addr
            (local $alloc_size i32)
            (local $orig_young_length i32)
            (local $new_young_length i32)
            (local $orig_offset_addr i32)

        ;; debug only?
        (if (i32.ne (local.get $pointers_mutable) (i32.const 0)) (then
        (if (i32.eq (local.get $pointer_cnt) (i32.const 0)) (then
            (call $log_err_code (i32.const 12))
            unreachable
        ))))

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

        ;; write metadata
        (call $write_metadata_heap
                (local.get $orig_offset_addr)
                (local.get $pointer_cnt)
                (local.get $data_size_32)
                (local.get $pointers_mutable))

        ;; update heap length
        (i32.store (call $addr_young_length) (local.get $new_young_length))

        ;; return data address, which is after metadata
        (return (i32.add (local.get $orig_offset_addr) (i32.const 4)))
    )

    ;; start a stack frame; can allocate with stack_alloc,
    ;; but only if doesn't live past stack_pop_to.
    ;; Returns $frame_ix (in words) to pass to stack_pop_to.
    (func $stack_push (export "stack_push")
            (result i32)
        (i32.load (call $addr_young_side))
    )

    ;; drop stack frame started with stack_alloc; assumes all dropped
    ;; memory is unreferenced. Must provide the ix returned by stack_push.
    (func $stack_pop_to (export "stack_pop")
            (param $frame_ix i32)
            (local $orig_size i32)
        (local.set $orig_size (i32.load (call $addr_young_side)))
        ;;TODO: should such safeties be disabled in production mode?
        (if (i32.gt_u (local.get $frame_ix) (local.get $orig_size)) (then
            ;; this must only shrink the stack, not grow
            (call $log_err_code (i32.const 4))
            unreachable
        ))
        (if (i32.lt_s (local.get $frame_ix) (i32.const 0)) (then
            (call $log_err_code (i32.const 5))
            unreachable
        ))
        (i32.store (call $addr_stack_length) (local.get $frame_ix))
    )

    ;; like $alloc_stack0, but traps when OOM
    (func $alloc_stack (export "$alloc_stack")
            (param $pointer_cnt i32)
            (param $data_size_32 i32)  ;; units are 32-bit words
            (result i32)  ;; addr
            (local $res i32)
        (local.set $res (call $alloc_stack0 (local.get $pointer_cnt) (local.get $data_size_32)))
        (if (i32.eq (local.get $res) (i32.const 0)) (then
            (call $log_err_code (i32.const 6))
            unreachable
        ))
        local.get $res
    )

    ;; allocate memory on current stack frame; must be unreferenced before
    ;; stack_pop_to. (it may be possible to group several objects into a
    ;; single allocation, but not all of them, due to dynamically sized objects).
    ;;TODO: make a 0-returning version?
    (func $alloc_stack0 (export "stack_alloc0")
            (param $pointer_cnt i32)
            (param $data_size_32 i32)  ;; units are 32-bit words
            (result i32)  ;; addr
            (local $alloc_size i32)
            (local $orig_stack_length i32)
            (local $new_stack_length i32)
            (local $orig_offset_addr i32)
        ;;TODO @mark: this should mirror alloc0 except mutability

        ;; calculate the necessary size (words) including metadata
        (local.set $alloc_size (i32.add (i32.const 1) (i32.add (local.get $pointer_cnt) (local.get $data_size_32))))
        ;;TODO @mark: for now assume metadata is 1 word ^

        ;; calculate new stack size (but don't update yet)
        (local.set $orig_stack_length (i32.load (call $addr_stack_length)))
        (local.set $new_stack_length (i32.add (local.get $orig_stack_length) (local.get $alloc_size)))

        ;; check if enough memory
        (if (i32.gt_u (local.get $new_stack_length) (call $const_stack_max_size)) (then
            (return (i32.const 0)) ))

        ;; find current top of young heap addr
        (local.set $orig_offset_addr (i32.add
            (call $glob_stack_start_addr)
            (i32.mul (i32.const 4) (local.get $orig_stack_length))))

        ;; write metadata - just length for now
        (call $write_metadata_stack
                (local.get $orig_offset_addr)
                (local.get $pointer_cnt)
                (local.get $data_size_32))
        ;;TODO: can skip some values

        ;; update stack size length
        (i32.store (call $addr_stack_length) (local.get $new_stack_length))

        ;; return data address, which is after metadata
        (return (i32.add (local.get $orig_offset_addr) (i32.const 4)))
    )

    ;; do a small GC, e.g. young generation only
    (func $gc_fast (export "gc_fast")
    )

    ;; do a big GC, e.g. check all memory regions
    (func $gc_full (export "gc_full")
    )

    (func $write_metadata_heap
            (param $meta_addr i32)
            (param $pointer_cnt i32)
            (param $data_size_32 i32)
            (param $pointers_mutable i32)
            (local $flags i32)

        ;; reset all bits to 0
        (i32.store (local.get $meta_addr) (i32.const 0))

        ;; sizes
        (if (i32.gt_u (local.get $pointer_cnt) (i32.const 127)) (then (call $log_err_code (i32.const 7)) unreachable ))
        (if (i32.gt_u (local.get $data_size_32) (i32.const 127)) (then (call $log_err_code (i32.const 8)) unreachable ))

        (i32.store8 (i32.add (local.get $meta_addr) (i32.const 2)) (local.get $pointer_cnt))
        (i32.store8 (i32.add (local.get $meta_addr) (i32.const 3)) (local.get $data_size_32))

        ;; flags
        (local.set $flags (i32.const 0))

        ;; first bit of this byte is for GC flag, 2nd for pointer mutability
        (if (i32.ne (local.get $pointers_mutable) (i32.const 0)) (then
                (local.set $flags (i32.or (local.get $flags) (i32.const 2)))))

        (i32.store8 (i32.add (local.get $meta_addr) (i32.const 1)) (local.get $flags))

        ;; type, see $read_metadata_type
        (i32.store8 (local.get $meta_addr) (i32.const 1))
    )

    (func $write_metadata_stack
            (param $meta_addr i32)
            (param $pointer_cnt i32)
            (param $data_size_32 i32)
        (if (i32.gt_u (local.get $pointer_cnt) (i32.const 127)) (then (call $log_err_code (i32.const 9)) unreachable ))
        (if (i32.gt_u (local.get $data_size_32) (i32.const 127)) (then (call $log_err_code (i32.const 10)) unreachable ))

        (i32.store8 (i32.add (local.get $meta_addr) (i32.const 2)) (local.get $pointer_cnt))
        (i32.store8 (i32.add (local.get $meta_addr) (i32.const 3)) (local.get $data_size_32))
    )

    ;; same for stack and heap
    (func $read_metadata_pointer_cnt
            (param $meta_addr i32)
            (result i32)
        (i32.load8_u (i32.add (local.get $meta_addr) (i32.const 2)))
    )

    ;; same for stack and heap
    (func $read_metadata_data_word_cnt
            (param $meta_addr i32)
            (result i32)
        (i32.load8_u (i32.add (local.get $meta_addr) (i32.const 3)))
    )

    ;; only for heap, not stack
    ;; type (1=struct, 2=array, 127=GC-redirect)
    (func $read_metadata_type
            (param $meta_addr i32)
            (result i32)
            (local $res i32)
        (local.set $res (i32.load8_u (local.get $meta_addr)))
        ;; check in debug mode only:
        (if (i32.and (i32.ne (local.get $res) (i32.const 1))
                (i32.and (i32.ne (local.get $res) (i32.const 2))
                    (i32.ne (local.get $res) (i32.const 127)))) (then
            (call $log_err_code (i32.const 11)) unreachable
        ))
        local.get $res
    )

    ;; only for heap, not stack
    (func $read_metadata_pointers_mutable
            (param $meta_addr i32)
            (result i32)
        (i32.ne (i32.const 0)
            (i32.and
                (i32.load8_u (i32.add (local.get $meta_addr) (i32.const 1)))
                (i32.const 2)))
    )

    ;;
    ;; some internals, perhaps mostly for testing, as they make it hard to change impl
    ;;

    (func $get_young_size
            (result i32)
        (i32.load (call $addr_young_length))
    )

    (func $get_stack_size
            (result i32)
        (i32.load (call $addr_stack_length))
    )

    (func $print_memory
        call $print_stack
        call $print_heap
    )

    (func $print_stack
            (local $i i32)
            (local $p i32)
            (local $start i32)
            (local $upto i32)
        (local.set $start (call $glob_stack_start_addr))
        (local.set $upto (i32.mul (i32.const 4) (call $get_stack_size)))
        (local.set $i (i32.const 0))
        (block $outer (loop $continue
            (i32.ge_u (local.get $i) (local.get $upto))
            br_if $outer
            (local.set $p (i32.add (local.get $i) (local.get $start)))
            (call $log_i32x7
                    (i32.div_s (local.get $i) (i32.const -4))
                    (i32.load8_u (i32.add (local.get $p) (i32.const 0)))
                    (i32.load8_u (i32.add (local.get $p) (i32.const 1)))
                    (i32.load8_u (i32.add (local.get $p) (i32.const 2)))
                    (i32.load8_u (i32.add (local.get $p) (i32.const 3)))
                    (i32.load (local.get $p))
                    (local.get $p))
            (local.set $i (i32.add (local.get $i) (i32.const 4)))
            (br $continue)
        ))
    )

    (func $print_heap
            (local $i i32)
            (local $p i32)
            (local $start i32)
            (local $upto i32)
        (local.set $start (call $glob_young_start_addr))
        (local.set $upto (i32.mul (i32.const 4) (call $get_young_size)))
        (local.set $i (i32.const 0))
        (block $outer (loop $continue
            (i32.ge_u (local.get $i) (local.get $upto))
            br_if $outer
            (local.set $p (i32.add (local.get $i) (local.get $start)))
            (call $log_i32x7
                    (i32.div_s (local.get $i) (i32.const 4))
                    (i32.load8_u (i32.add (local.get $p) (i32.const 0)))
                    (i32.load8_u (i32.add (local.get $p) (i32.const 1)))
                    (i32.load8_u (i32.add (local.get $p) (i32.const 2)))
                    (i32.load8_u (i32.add (local.get $p) (i32.const 3)))
                    (i32.load (local.get $p))
                    (local.get $p))
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

        (call $test_mut_pointer_alloc)
        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $alloc_init)  ;; reset heap

        (call $test_double_stack_alloc)
        ;;TODO @mark: not printing?
        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        (call $test_alloc_full_heap_GC)
        (call $alloc_init)  ;; reset heap

        (call $test_compact_alive_in_full_GC)
        (call $alloc_init)  ;; reset heap
    )

    (func $test_empty_heap
        (call $alloc_init)  ;; reset heap
        (if (i32.ne (call $get_young_size) (i32.const 0))
            (then (call $log_err_code (i32.const 107)) unreachable))
    )

    (func $test_double_data_alloc

        ;; first allocation
        (drop (call $alloc (i32.const 0) (i32.const 2) (i32.const 0)))
        (if (i32.ne (call $get_young_size) (i32.const 3)) (then
            (call $log_err_code (i32.const 100))
            unreachable
        ))

        ;; what if we do it again
        (drop (call $alloc (i32.const 0) (i32.const 1) (i32.const 0)))
        (if (i32.ne (call $get_young_size) (i32.const 5)) (then
            (call $log_err_code (i32.const 101))
            unreachable
        ))
    )

    (func $test_mut_pointer_alloc
            (local $addr i32)

        ;; allocation with mutable pointers and data twice
        (drop (call $alloc (i32.const 1) (i32.const 0) (i32.const 1)))
        (local.set $addr (call $alloc (i32.const 2) (i32.const 2) (i32.const 1)))

        ;; check the size and properties
        (if (i32.ne (call $get_young_size) (i32.const 7)) (then (
            call $log_err_code (i32.const 112)) unreachable ))
    )

    (func $test_double_stack_alloc
            (local $top1 i32)
            (local $top2 i32)

        ;; frames
        (local.set $top1 (call $stack_push))
        (local.set $top2 (call $stack_push))
        (call $stack_pop_to (local.get $top2))
        (local.set $top2 (call $stack_push))

        ;; first allocation
        (drop (call $alloc_stack (i32.const 0) (i32.const 2)))
        (if (i32.ne (call $get_stack_size) (i32.const 3)) (then
            (call $log_err_code (i32.const 108))
            unreachable
        ))

        ;; prev frame
        (call $stack_pop_to (local.get $top2))

        ;; what if we do it again
        (drop (call $alloc_stack (i32.const 0) (i32.const 1)))
        (drop (call $alloc_stack (i32.const 0) (i32.const 8)))
        (if (i32.ne (call $get_stack_size) (i32.const 11)) (then
            (call $log_err_code (i32.const 109))
            unreachable
        ))

        ;; empty stack
        (call $stack_pop_to (local.get $top1))
        (if (i32.ne (call $get_stack_size) (i32.const 0)) (then
            (call $log_err_code (i32.const 110))
            unreachable
        ))

        ;; leave some data for heap test
        (local.set $top1 (call $stack_push))
        (drop (call $alloc_stack (i32.const 0) (i32.const 2)))
    )

    (func $test_alloc_full_heap_GC
            (local $i i32)
            (local $orig_stack_size i32)

        ;; fill almost all memory
        (local.set $i (i32.const 0))
        (block $outer (loop $continue
            (i32.ge_u (local.get $i) (i32.const 128))
            br_if $outer
            (drop (call $alloc (i32.const 0) (i32.const 127) (i32.const 0)))
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            br $continue
        ))

        ;; test that alloc fails
        (call $alloc0 (i32.const 0) (i32.const 127) (i32.const 0))
        (if (i32.ne (i32.const 0)) (then
            (call $log_err_code (i32.const 105)) unreachable))

        ;; create some stack allocs so GC isn't too easy
        (drop (call $stack_push))
        (drop (call $alloc_stack (i32.const 0) (i32.const 127)))
        (drop (call $stack_push))
        (drop (call $alloc_stack (i32.const 0) (i32.const 3)))
        (drop (call $alloc_stack (i32.const 0) (i32.const 50)))
        (local.set $orig_stack_size (call $get_stack_size))

        ;; do GC
        call $gc_full

        ;; test that GC does not clcean stack
        (if (i32.ne (call $get_stack_size) (local.get $orig_stack_size)) (then
            (call $log_err_code (i32.const 111)) unreachable))

        ;; test that GC cleans heap
        call $alloc_init  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (if (i32.ne (call $get_young_size) (i32.const 0)) (then
            (call $log_err_code (i32.const 106)) unreachable))
    )

    (func $test_compact_alive_in_full_GC
            (local $orig_heap_size i32)
            (local $stack_top i32)
            (local $ref_on_stack_addr_1 i32)
            (local $ref_on_stack_addr_2 i32)
            (local $ref_on_stack_addr_3 i32)
            (local $heap_selfref_addr i32)
            (local $heap_popped_addr i32)
            (local $heap_shallow_addr i32)
            (local $heap_deep_addr i32)
            (local $new_heap_shallow_addr i32)
            (local $new_heap_deep_addr i32)

;;        (call $log_i32 (i32.const -1))  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        ;; fill some unreferences heap memory
;;        (call $log_i32 (call $glob_young_start_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (local.set $heap_selfref_addr (call $alloc (i32.const 1) (i32.const 2) (i32.const 1)))

;;        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (i32.const -2))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (local.get $heap_selfref_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (call $glob_young_start_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (i32.add (local.get $heap_selfref_addr) (i32.const 4)))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (local.get $heap_selfref_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        (i32.store (local.get $heap_selfref_addr) (local.get $heap_selfref_addr))

;;        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (i32.const -3))  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        (local.set $heap_popped_addr (call $alloc (i32.const 1) (i32.const 1) (i32.const 1)))

;;        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (i32.const -4))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (local.get $heap_popped_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        (i32.store (local.get $heap_popped_addr) (local.get $heap_popped_addr))
        ;;TODO @mark: make sure this gets referenced from the heap, but cleaned ^

;;        (call $log_i32 (i32.const -5))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_nl)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        ;; fill some more heap memory that we'll reference
        (local.set $heap_deep_addr (call $alloc (i32.const 1) (i32.const 1) (i32.const 1)))
        (i32.store (i32.add (local.get $heap_deep_addr) (i32.const 4)) (i32.const -1))

;;        (call $log_i32 (i32.const -6))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_nl)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        (local.set $heap_shallow_addr (call $alloc (i32.const 1) (i32.const 2) (i32.const 1)))
        (i32.store (local.get $heap_shallow_addr) (local.get $heap_deep_addr))
        (i32.store (i32.add (local.get $heap_shallow_addr) (i32.const 4)) (local.get $heap_deep_addr))
        (i32.store (i32.add (local.get $heap_shallow_addr) (i32.const 8)) (i32.const -2))

;;        (call $log_i32 (i32.const -7))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (local.get $heap_deep_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_i32 (local.get $heap_shallow_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $log_nl)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
;;        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        (call $log_nl)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        ;; add some references to the heap on the stack:
        ;;; reference to heap
        (drop (call $stack_push))
        (local.set $ref_on_stack_addr_1 (call $alloc_stack (i32.const 1) (i32.const 10)))
        (i32.store (local.get $ref_on_stack_addr_1) (local.get $heap_shallow_addr))
        (i32.store (i32.add (local.get $ref_on_stack_addr_1) (i32.const 4)) (i32.const -3))

        (call $log_i32 (i32.const -8))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $log_i32 (local.get $ref_on_stack_addr_1))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $log_i32 (local.get $heap_shallow_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $log_nl)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        ;;; will be popped before GC
        (local.set $stack_top (call $stack_push))
        (local.set $ref_on_stack_addr_2 (call $alloc_stack (i32.const 1) (i32.const 5)))
        (i32.store (local.get $ref_on_stack_addr_2) (local.get $heap_popped_addr))
        ;;; reference to stack
        (call $stack_pop_to (local.get $stack_top))
        (local.set $ref_on_stack_addr_3 (call $alloc_stack (i32.const 1) (i32.const 8)))
        (i32.store (local.get $ref_on_stack_addr_3) (local.get $ref_on_stack_addr_1))

        return  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        ;; run GC
        (local.set $orig_heap_size (call $get_young_size))
        call $gc_full

        ;; check that memory usage decreased
        (if (i32.eq (call $get_young_size) (i32.const 0)) (then
            (call $log_err_code (i32.const 113)) unreachable))
;;        (if (i32.ge_s (call $get_young_size) (local.get $orig_heap_size)) (then
;;            (call $log_err_code (i32.const 114)) unreachable))
        ;;TODO @mark: ON ^

        (call $log_nl)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $print_memory)  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        (call $log_i32 (local.get $ref_on_stack_addr_1))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $log_i32 (i32.load (i32.add (local.get $ref_on_stack_addr_1) (i32.const 4))))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $log_i32 (local.get $heap_shallow_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $log_i32 (local.get $new_heap_shallow_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $log_i32 (local.get $heap_deep_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $log_i32 (local.get $new_heap_deep_addr))  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        ;; check that referenced memory still exists
        (if (i32.ne (i32.load (i32.add (local.get $ref_on_stack_addr_1) (i32.const 4))) (i32.const -3)) (then
            (call $log_err_code (i32.const 115)) unreachable))
        (local.set $new_heap_shallow_addr (i32.load (local.get $ref_on_stack_addr_1)))
;;        (if (i32.ne (i32.load (i32.add (local.get $new_heap_shallow_addr) (i32.const 8))) (i32.const -2)) (then
;;            (call $log_err_code (i32.const 116)) unreachable))
        ;;TODO @mark: ON ^
        (local.set $new_heap_deep_addr (i32.load (local.get $new_heap_shallow_addr)))
;;        (if (i32.ne (i32.load (i32.add (local.get $new_heap_deep_addr) (i32.const 4))) (i32.const -1)) (then
;;            (call $log_err_code (i32.const 117)) unreachable))
        ;;TODO @mark: ON ^

        ;; check that referenced memory has moved (which in combination
        ;; with above probably means compacted)
        (if (i32.eq (local.get $heap_shallow_addr) (local.get $new_heap_shallow_addr)) (then
            (call $log_err_code (i32.const 118)) unreachable))
        (if (i32.eq (local.get $heap_deep_addr) (local.get $new_heap_deep_addr)) (then
            (call $log_err_code (i32.const 119)) unreachable))
    )

    ;;TODO @mark: test that mutable pointer data doesn't go to old heap
)
