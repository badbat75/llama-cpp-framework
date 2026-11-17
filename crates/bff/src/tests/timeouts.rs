//! The time limits the service hands the router: the reads' and the writes'.
//! The HTTP tests of a route's answer hand their own pair through
//! `router_with_session_and_write_timeout` (in `tests/`), so the pair
//! production runs with is pinned here.

use std::time::Duration;

/// The reads' limit and the writes' one, and the rule that the write's is the
/// longer: a manager busy writing is not cut off at the limit that is right for
/// a read.
#[test]
fn the_write_limit_outlasts_the_read_limit() {
    assert_eq!(crate::MANAGER_TIMEOUT, Duration::from_secs(5));
    assert_eq!(crate::WRITE_TIMEOUT, Duration::from_secs(15));
    assert!(crate::WRITE_TIMEOUT > crate::MANAGER_TIMEOUT);
}
