
;; This wasm GC makes some important assumptions:
;; - allocations are N pointers followed by M bytes if non-pointer data
        ;;TODO @mark: how does this work with arrays? ^
;; - code only reads/writes allocated memory, and only while reachable from either roots or allocated pointers
;; - roots don't change during GC
;; - there is a single thread (or in the future perhaps one heap per thread)
;; - most data is immutable, and only mutable data can mutate
;;   (this is important for e.g. the assumption that old gen cannot point to young gen)
;; - all allocations are multiples of 32 bytes
;; - for now (can be lifted): at most 128 pointers and words of data
;; priorities for the GC are:
;; - small size
;; - easy to configure
;; - decent performance in common cases
;;   performance encompasses everything: allocation, GC, memory locality, etc
;;TODO @mark: ^ is all this still correct?
;;
;; Metadata:
;; - pointer cnt
;; - data word size
;; - is mutable
;; - generation count
;; - is array? length?
;; Some of this is per-type instead of per-object, but might still be efficient to duplicate

;; TODO how to find roots?
;; TODO how to handle arrays (detect pointers)
;; TODO how to handle 0-byte allocations? is there reference equality anywhere?
;; TODO have some post-GC handler?

(module
    (import "host" "log_i32" (func $log_i32 (param i32)))
    (import "host" "log_i32x4" (func $log_i32x4 (param i32) (param i32) (param i32) (param i32)))
    (import "host" "log_err_code" (func $log_err_code (param i32)))
    (memory 1 1)  ;; 64k
    (func $alloc_init (i32.store (call $const_addr_young_length) (i32.const 8)))
    (start $alloc_init)

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
            (local $offset_addr i32)
            (local $init_top i32)
            (local $req_alloc_size i32)
            (local $meta_alloc_size i32)
        (local.set $offset_addr (call $const_addr_young_length))

        ;; mutable not supported yet
        (i32.ne (local.get $is_mutable) (i32.const 0))
        (if (then
            (call $log_err_code (i32.const 2))
            unreachable
        ))
        ;; pointer_cnt not supported yet
        (i32.ne (local.get $pointer_cnt) (i32.const 0))
        (if (then
            (call $log_err_code (i32.const 3))
            unreachable
        ))

        ;; calculate the necessary size including metadata
        (local.set $req_alloc_size (i32.mul (i32.const 4)
                (i32.add (local.get $pointer_cnt) (local.get $data_size_32))))
        (local.set $meta_alloc_size (i32.add (i32.const 4) (local.get $req_alloc_size)))

        ;; read current end-of-young-gen address
        (local.set $init_top (i32.load (local.get $offset_addr)))
        (i32.store (local.get $offset_addr) (i32.add
                (local.get $init_top) (local.get $meta_alloc_size)))

        ;; write metadata - just length for now
        (call $write_metadata
                (local.get $init_top)
                (local.get $pointer_cnt)
                (local.get $data_size_32)
                (local.get $is_mutable))

        ;; return data address, which is after metadata
        (return (i32.add (local.get $init_top) (i32.const 4)))
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
        (if (i32.gt_u (local.get $data_size_32) (i32.const 127)) (then (call $log_err_code (i32.const 103)) unreachable ))

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
        (i32.load (call $const_addr_young_length))
    )

    (func $print_heap
            (local $i i32)
            (local $upto i32)
        (local.set $upto (i32.load (call $const_addr_young_length)))
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

    (func $const_addr_young_length (result i32)
        i32.const 4
    )

    ;;
    ;; TESTS
    ;;

    (func $gc_tests (export "tests")
        (call $test_double_data_alloc)
        (call $print_heap)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $alloc_init)  ;; reset heap
        (call $log_i32 (i32.const -1))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (call $print_heap)  ;;TODO @mark: TEMPORARY! REMOVE THIS!
    )

    (func $test_double_data_alloc

        ;; first allocation
        (i32.ne
            (call $alloc (i32.const 0) (i32.const 2) (i32.const 0))
            (i32.const 12))
        (if (then
            (call $log_err_code (i32.const 100))
            unreachable
        ))

        ;; what if we do it again
        (i32.ne
            (call $alloc (i32.const 0) (i32.const 1) (i32.const 0))
            (i32.const 24))
        (if (then
            (call $log_err_code (i32.const 101))
            unreachable
        ))

        ;; check young size (note pointer returned before if after metadata but before actual data)
        (i32.ne
            (call $get_young_size)
            (i32.const 28))
        (if (then
            (call $log_err_code (i32.const 102))
            unreachable
        ))
    )
)
