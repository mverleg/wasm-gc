
;; This wasm GC makes some important assumptions:
;; - allocations are N pointers followed by M bytes if non-pointer data
        ;;TODO @mark: how does this work with arrays? ^
;; - code only reads/writes allocated memory, and only while reachable from either roots or allocated pointers
;; - roots don't change during GC
;; - there is a single thread (or in the future perhaps one heap per thread)
;; - most data is immutable, and only mutable data can mutate
;;   (this is important for e.g. the assumption that old gen cannot point to young gen)
;; priorities for the GC are:
;; - small size
;; - easy to configure
;; - decent performance in common cases
;;   performance encompasses everything: allocation, GC, memory locality, etc
;;TODO @mark: ^ is all this still correct?

(module
    (import "host" "log_i32" (func $log_i32 (param i32)))
    (import "host" "log_err_code" (func $log_err_code (param i32)))
    (memory 1)
    (func $alloc_init (i32.store (i32.const 1) (i32.const 2)))
    (start $alloc_init)
    ;; default alloc, traps when OOM
    (func $alloc (export "alloc")
            (param $pointer_cnt i32)
            (param $data_size i32)
            (param $is_mutable i32)
            (result i32)  ;; addr
            (local $res i32)
        (block $alloc_ok
            (local.set $res
                (call $alloc0 (local.get $pointer_cnt) (local.get $data_size) (local.get $is_mutable)))
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
            (param $data_size i32)
            (param $is_mutable i32)
            (result i32)  ;; addr
            (local $offset_addr i32)
            (local $init_top i32)
            (local $req_alloc_size i32)
            (local $meta_alloc_size i32)
        (local.set $offset_addr (i32.const 1))

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
        (local.set $req_alloc_size (i32.add (local.get $pointer_cnt) (local.get $data_size)))
        (local.set $meta_alloc_size (i32.add (i32.const 1) (local.get $req_alloc_size)))

        ;; read current end-of-young-gen address
        (local.set $init_top (i32.load (local.get $offset_addr)))
        (i32.store (local.get $offset_addr) (i32.add
                (local.get $init_top) (local.get $meta_alloc_size)))

        ;; write metadata - just length for now
        (call $log_i32 (local.get $init_top))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        (i32.store (local.get $init_top) (local.get $meta_alloc_size))

        ;; (call $log_i32 (local.get $init_top))  ;;TODO @mark: TEMPORARY! REMOVE THIS!
        ;; (call $log_i32 (local.get $alloc_size))  ;;TODO @mark: TEMPORARY! REMOVE THIS!

        ;; return data address, which is after metadata
        (return (i32.add (local.get $init_top) (i32.const 1)))
    )
    ;; do a small GC, e.g. young generation only
    (func $gc_fast (export "gc_fast")
    )
    ;; do a big GC, e.g. check all memory regions
    (func $gc_full (export "gc_full")
    )

    ;; TODO @mark: make it possible to register an post-GC handler?

    (func $gc_tests (export "tests")
            (result i32)  ;; 0 if ok, 1 if fail

        ;; first allocation
        (i32.ne
            (call $alloc (i32.const 0) (i32.const 4) (i32.const 0))
            (i32.const 3))
        (if (then
            (call $log_err_code (i32.const 100))
            unreachable
        ))

        ;; what if we do it again
        (i32.ne
            (call $alloc (i32.const 0) (i32.const 4) (i32.const 0))
            (i32.const 8))
        (if (then
            (call $log_err_code (i32.const 101))
            unreachable
        ))

        ;; no error yet
        i32.const 0
    )
)
