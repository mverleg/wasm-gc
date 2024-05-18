(module
    (import "host" "log_i32" (func $log_i32 (param i32)))
    (memory 1)
    (func $alloc (export "alloc")
            (param i32)  ;; size
            (result i32)  ;; addr
        i32.const 1
    )
    (func $gc_fast (export "gc_fast")
    )
    (func $gc_full (export "gc_full")
    )

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
