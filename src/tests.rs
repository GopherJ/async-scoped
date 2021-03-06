#[async_std::test]
async fn scope() {
    let not_copy = String::from("hello world!");
    let not_copy_ref = &not_copy;

    let (stream, _) = unsafe {crate::scope(|s| {
        for _ in 0..10 {
            let proc = || async move {
                assert_eq!(not_copy_ref, "hello world!");
            };
            s.spawn(proc());
        }
    })};

    // Uncomment this for compile error
    // std::mem::drop(not_copy);

    use futures::StreamExt;
    let count = stream.collect::<Vec<_>>().await.len();

    // Drop here is okay, as stream has been consumed.
    std::mem::drop(not_copy);
    assert_eq!(count, 10);
}

/// Test scope bounds: should allow any future with lifetime
/// larger than the scope's lifetime
#[async_std::test]
async fn scope_lifetime() {
    use std::future::Future;
    let static_fut = futures::future::ready(());
    fn test_static<F: Future + 'static>(_: &F) {}
    test_static(&static_fut);

    let not_copy = String::from("hello world!");
    let not_copy_ref = &not_copy;
    let ((), vals) = unsafe { crate::scope_and_collect(|s| {
        s.spawn(static_fut);
        for _ in 0..10 {
            let proc = || async {
                assert_eq!(not_copy_ref, "hello world!");
            };
            s.spawn(proc());
        }
    })}.await;
    assert_eq!(vals.len(), 11);

}

#[async_std::test]
async fn scope_async() {
    let not_copy = String::from("hello world!");
    let not_copy_ref = &not_copy;

    let stream = unsafe {
        use async_std::future::{timeout, pending};
        use std::time::Duration;
        let mut s = crate::Scope::create();
        for _ in 0..10 {
            let proc = || async move {
                assert_eq!(not_copy_ref, "hello world!");
            };
            s.spawn(proc());
            let _ = timeout(
                Duration::from_millis(10),
                pending::<()>(),
            ).await;
        }
        s
    };

    // Uncomment this for compile error
    // std::mem::drop(not_copy);

    use futures::StreamExt;
    let count = stream.collect::<Vec<_>>().await.len();

    // Drop here is okay, as stream has been consumed.
    std::mem::drop(not_copy);
    assert_eq!(count, 10);
}


#[async_std::test]
async fn scope_and_collect() {
    let not_copy = String::from("hello world!");
    let not_copy_ref = &not_copy;

    let (_, vals) = unsafe { crate::scope_and_collect(|s| {
        for _ in 0..10 {
            let proc = || async {
                assert_eq!(not_copy_ref, "hello world!");
            };
            s.spawn(proc());
        }
    }) }.await;

    assert_eq!(vals.len(), 10);
}

#[async_std::test]
async fn scope_and_block() {
    let not_copy = String::from("hello world!");
    let not_copy_ref = &not_copy;

    let ((), vals) = crate::scope_and_block(|s| {
        for _ in 0..10 {
            let proc = || async {
                assert_eq!(not_copy_ref, "hello world!");
            };
            s.spawn(proc());
        }
    });

    assert_eq!(vals.len(), 10);
}

/// This is a simplified version of the soundness bug
/// pointed out on [reddit][reddit-ref]. Here, we test that
/// it does not happen when using the `scope_and_collect`,
/// but the returned future is not forgotten. Forgetting the
/// future should lead to an invalid memory access.
///
/// [reddit-ref]: https://www.reddit.com/r/rust/comments/ee3vsu/asyncscoped_spawn_non_static_futures_with_asyncstd/fbpis3c?utm_source=share&utm_medium=web2x
#[async_std::test]
async fn cancellation_soundness() {
    use async_std::future;
    use std::time::*;

    async fn inner() {
        let mut shared = true;
        let shared_ref = &mut shared;

        let start = Instant::now();

        let mut fut = Box::pin(
            unsafe { crate::scope_and_collect(|scope| {
                scope.spawn_cancellable(async {
                    assert!(future::timeout(
                        Duration::from_millis(500),
                        future::pending::<()>(),
                    ).await.is_err());

                    eprintln!("Trying to write to shared_ref");
                    *shared_ref = false;
                    assert!(*shared_ref);
                }, || ());
            })}
        );
        let _ = future::timeout(Duration::from_millis(10), &mut fut).await;

        // Dropping explicitly to measure time taken to complete drop.
        // Change the drop to forget for panic due to invalid mem. access.
        std::mem::drop(fut);
        let elapsed = start.elapsed().as_millis();


        // The cancelled future should have been polled
        // before the inner large timeout.
        assert!(elapsed < 100);
        eprintln!("Elapsed: {}ms", start.elapsed().as_millis());
    }

    inner().await;

    // This timeout allows any (possible) invalid memory
    // access to actually take place.
    assert!(future::timeout(Duration::from_millis(600),
                            future::pending::<()>()).await.is_err());

}

/// This test is resource consuming and ignored by default
#[async_std::test]
#[ignore]
async fn backpressure() {
    let mut s = unsafe { crate::Scope::create() };
    let limit = 0x10;
    for i in 0..0x100 {
        s.spawn(async {
            // Allocate a large array (256 MB)
            let blob = vec![42u8; 0x10000000];

            // Spend a lot of time on it asynchronously
            use async_std::future;
            use std::time::Duration;
            let _ = future::timeout(
                Duration::from_millis(100),
                future::pending::<()>()
            ).await;

            std::mem::drop(blob);
        });

        while s.remaining() > limit {
            use futures::StreamExt;
            s.next().await;
        }
        eprintln!("Spawned {} futures", i);
    }
}

// Mutability test: should fail to compile.
// TODO: use compiletest_rs
// #[async_std::test]
// async fn mutating_scope() {
//     let mut not_copy = String::from("hello world!");
//     let not_copy_ref = &mut not_copy;
//     let mut count = 0;

//     crate::scope_and_block(|s| {
//         for _ in 0..10 {
//             let proc = || async {
//                 not_copy_ref.push('.');
//             };
//             s.spawn(proc()); //~ ERROR
//         }
//     });

//     assert_eq!(count, 10);
// }
