mod timeheap;

use futures_lite::future::FutureExt;
use once_cell::sync::Lazy;

use std::{
    thread::JoinHandle,
    time::{Duration, Instant},
};
use timeheap::TimeHeap;

static START: Lazy<Instant> = Lazy::new(Instant::now);

/// Converts an Instant to an epoch.
fn instant_to_epoch(i: Instant) -> u64 {
    i.saturating_duration_since(*START).as_millis() as u64
}

fn epoch_to_instant(e: u64) -> Instant {
    *START + Duration::from_millis(e)
}

/// A mapping between epochs to their wakeup event
static EVENT_QUEUE: Lazy<TimeHeap> = Lazy::new(Default::default);

static WAKER: Lazy<JoinHandle<()>> = Lazy::new(|| {
    std::thread::Builder::new()
        .name("microsleep".into())
        .spawn(|| {
            loop {
                let now = instant_to_epoch(Instant::now());
                EVENT_QUEUE.fire_before(now);
                let earliest = EVENT_QUEUE.earliest_tick().unwrap_or(u64::MAX);
                // slow path: there are no timers that are gonna fire really soon.
                if earliest.saturating_sub(now) > 50 {
                    // if long in the future, we wait
                    let wait = EVENT_QUEUE.wait_until_change();
                    let earliest = EVENT_QUEUE.earliest_tick().unwrap_or(u64::MAX);
                    let timer = async move {
                        async_io::Timer::at(epoch_to_instant(earliest)).await;
                    };
                    futures_lite::future::block_on(timer.race(wait));
                } else {
                    // fast path: just sleep for 1 ms
                    spin_sleep::native_sleep(Duration::from_millis(1));
                }
            }
        })
        .unwrap()
});

/// Sleeps for the given interval
pub async fn sleep(dur: Duration) {
    until(Instant::now() + dur).await
}

/// Sleeps until the given Instant.
pub async fn until(i: Instant) {
    Lazy::force(&WAKER);
    futures_lite::future::yield_now().await;
    let epoch = instant_to_epoch(i);
    if instant_to_epoch(Instant::now()) >= epoch {
        return;
    }
    EVENT_QUEUE.subscribe(epoch).wait().await;
}
