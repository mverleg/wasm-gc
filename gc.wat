
;; This wasm GC makes some important assumptions:
;; - allocations are N pointers followed by M bytes if non-pointer data
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
    ;; default alloc, traps when OOM
    (func $alloc (export "alloc")
            (param $pointer_cnt i32)
            (param $data_size i32)
            (param $is_mutable i32)
            (result i32)  ;; addr
        (block $alloc_ok
            (i32.eq
                (call $alloc0 (local.get $pointer_cnt) (local.get $data_size) (local.get $is_mutable))
                (i32.const 0))
            br_if $alloc_ok
            i32.const 1  ;;TODO @mark:
            return
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
        i32.const 0  ;;TODO @mark:
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
        (block $outer
            (block $test_alloc
                (i32.ne
                    (call $alloc (i32.const 0) (i32.const 4) (i32.const 0))
                    (i32.const 2))
                br_if $outer
            )
        )
        i32.const 0
        return
    )
)
