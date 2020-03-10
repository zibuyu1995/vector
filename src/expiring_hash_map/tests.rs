use super::*;
// use crate::runtime::Runtime;
// use futures::future::poll_fn;
// use std::time::{Duration, Instant};
use std::task::Poll;
use tokio02 as tokio;
use tokio_test::{assert_pending, assert_ready, assert_ready_ok, task};

fn unwrap_ready<T>(poll: Poll<T>) -> T {
    assert_ready!(&poll);
    match poll {
        Poll::Ready(val) => val,
        _ => unreachable!(),
    }
}

#[test]
fn next_expired_is_pending_with_empty_map() {
    let mut map = ExpiringHashMap::<String, String>::new();
    let mut fut = task::spawn(map.next_expired());
    assert_pending!(fut.poll());
}

#[tokio::test]
async fn next_expired_is_pending_with_empty_map_after_it_was_non_empty() {
    let mut map = ExpiringHashMap::<String, String>::new();

    map.insert("key".to_owned(), "val".to_owned(), Duration::from_secs(1));
    map.remove("key");

    let mut fut = task::spawn(map.next_expired());
    assert_pending!(fut.poll());
}

#[tokio::test]
async fn it_does_not_wake_when_the_value_is_available_upfront() {
    let mut map = ExpiringHashMap::<String, String>::new();

    let a_minute_ago = Instant::now() - Duration::from_secs(60);
    map.insert_at("key".to_owned(), "val".to_owned(), a_minute_ago);

    let mut fut = task::spawn(map.next_expired());
    assert_ready_ok!(fut.poll());
    assert_eq!(fut.is_woken(), false);
}

// TODO: rewrite this test with tokio::time::clock when it's available.
// For now we just wait for an actal second. We should just scroll time instead.
// In theory, this is only possible when the runtime timer used in the
// underlying delay queue and the means by which we fresse/adjust time are
// working together.
#[tokio::test]
async fn it_wakes_and_becomes_ready_when_value_ttl_expires() {
    let mut map = ExpiringHashMap::<String, String>::new();

    let ttl = Duration::from_secs(5);
    map.insert("key".to_owned(), "val".to_owned(), ttl);

    let mut fut = task::spawn(map.next_expired());

    // At first, has to be pending.
    assert_pending!(fut.poll());

    // Sleep twice the ttl, to guarantee we're over the deadline.
    assert_eq!(fut.is_woken(), false);
    std::thread::sleep(ttl * 2);
    assert_eq!(fut.is_woken(), true);

    // Then, after deadline, has to be ready.
    assert_eq!(unwrap_ready(fut.poll()).unwrap().0, "val".to_owned());
}

#[tokio::test]
async fn it_wakes_when_the_value_is_added_to_an_empty_list() {
    let mut map = ExpiringHashMap::<String, String>::new();

    let mut fut = task::spawn(map.next_expired());

    // At first, has to be pending.
    assert_pending!(fut.poll());

    // Insert an item.
    assert_eq!(fut.is_woken(), false);
    let ttl = Duration::from_secs(1000);
    map.insert("key".to_owned(), "val".to_owned(), ttl);
    assert_eq!(fut.is_woken(), true);

    // Then, after value is inserted, has to be still pending.
    assert_pending!(fut.poll());
}

// #[test]
// fn does_not_wake_on_insert_when_non_empty() {
//     let mut rt = Runtime::new().unwrap();
//     rt.block_on_std(async {
//         let mut map = ExpiringHashMap::<String, String>::new();

//         map.insert("key1".to_owned(), "val".to_owned(), Duration::from_secs(1));

//         let (waker, count) = new_count_waker();
//         let mut cx = Context::from_waker(&waker);
//         assert!(map.next_expired().poll(&mut cx).is_pending());

//         assert_eq!(count, 0);

//         map.insert("key2".to_owned(), "val".to_owned(), Duration::from_secs(1));

//         assert_eq!(count, 0);
//     });
// }
