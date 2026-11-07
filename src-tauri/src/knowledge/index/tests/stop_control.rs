use super::*;

#[test]
fn index_stop_attempt_failure_is_not_overwritten_by_a_later_successful_retry() {
    let stop = StopControl::new();
    let first = stop.request();
    let observer = stop.subscribe();
    assert_eq!(stop.request(), first, "concurrent observers coalesce");
    stop.publish(first, Err(Code::Storage));
    assert_eq!(
        stop.request(),
        first,
        "unobserved failure cannot auto-start a retry"
    );
    stop.observed_failure(first);
    let second = stop.request();
    assert_eq!(second, first + 1);
    stop.publish(second, Ok(()));
    assert_eq!(
        observer.borrow().number,
        first,
        "each observer retains its own attempt"
    );
    assert_eq!(
        observer.borrow().result,
        Some(Err(Code::Storage)),
        "a later success cannot change a previously failed observation"
    );
}
