// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! Its own test binary (process): the checker's count is global, so no other test may run in
//! the same process while this one allocates on purpose.

// The plug-in library installs the checker as the global allocator (nih-plug, in debug builds):
// link it.
extern crate prowave_plug;

/// The allocation checker the other tests rely on is live: an allocation inside it is counted.
#[test]
fn the_allocation_checker_sees_an_allocation() {
    assert_no_alloc::reset_violation_count();
    assert_no_alloc::assert_no_alloc(|| {
        let v = vec![1u8; 100];
        std::hint::black_box(&v);
    });
    assert!(
        assert_no_alloc::violation_count() >= 1,
        "the allocation was not seen"
    );
    assert_no_alloc::reset_violation_count();
}
