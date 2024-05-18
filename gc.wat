
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
    (memory 1)
    ;; default alloc returns 0 when OOM
    (func $alloc (export "alloc")
            (param i32)  ;; size
            (result i32)  ;; addr
        i32.const 1
    )
    ;;
    (func $alloc (export "alloc")
            (param i32)  ;; number of pointers
            (param i32)  ;; additional size
            (param i32)  ;; is-mutable
            (result i32)  ;; addr
        i32.const 1
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
                i32.const 4
                call $alloc
                i32.const 2
                i32.ne
                br_if $outer
            )
        )
        i32.const 0
        return
    )
)
