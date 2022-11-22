use std::{
    collections::BTreeMap,
    sync::{Arc, Weak},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use futures_intrusive::sync::ManualResetEvent;
use once_cell::sync::Lazy;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use smallvec::SmallVec;

static START: Lazy<Instant> = Lazy::new(Instant::now);

/// Converts an Instant to an epoch.
fn instant_to_epoch(i: Instant) -> u64 {
    i.saturating_duration_since(*START).as_millis() as u64
}

fn epoch_to_instant(e: u64) -> Instant {
    *START + Duration::from_millis(e)
}

struct QueueEntry {
    key: u64,
    event: ManualResetEvent,
}

/// A mapping between epochs to their wakeup event
static EVENT_QUEUE: Lazy<RwLock<BTreeMap<u64, Arc<QueueEntry>>>> = Lazy::new(Default::default);

static WAKER: Lazy<JoinHandle<()>> = Lazy::new(|| {
    std::thread::Builder::new()
        .name("microsleep".into())
        .spawn(|| loop {
            let now = Instant::now();
            // first fire off for any things that are already expired
            let mut cue = EVENT_QUEUE.write();
            let now = instant_to_epoch(now);
            let mut to_rem: SmallVec<[u64; 32]> = SmallVec::new();
            for (&epoch, evt) in cue.iter() {
                if epoch <= now {
                    evt.event.set();

                    to_rem.push(epoch);
                } else {
                    break;
                }
            }
            for r in to_rem {
                cue.remove(&r);
            }

            spin_sleep::native_sleep(Duration::from_millis(1));
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
    let epoch = instant_to_epoch(i);
    let qe = {
        loop {
            let q = EVENT_QUEUE.upgradable_read();
            if let Some(v) = q.get(&epoch).map(|v| v.clone()) {
                break v;
            } else if let Ok(mut q) = RwLockUpgradableReadGuard::try_upgrade(q) {
                let qe = Arc::new(QueueEntry {
                    key: epoch,
                    event: ManualResetEvent::new(false),
                });

                q.insert(epoch, qe.clone());
                // WAKER.thread().unpark();
                break qe;
            }
        }
    };
    qe.event.wait().await
}
