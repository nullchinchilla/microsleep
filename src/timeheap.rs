use event_listener::{Event, EventListener};
use futures_intrusive::sync::ManualResetEvent;
use parking_lot::RwLock;
use priority_queue::PriorityQueue;
use slab::Slab;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub struct TimeHeap {
    inner: RwLock<Inner>,
    waker: Event,
}

impl TimeHeap {
    /// Returns an event for a particular tick.
    pub fn subscribe(&self, tick: u64) -> Arc<ManualResetEvent> {
        {
            let inner = self.inner.read();
            if let Some(&evt_idx) = inner.tick_to_evt.get(&tick) {
                return inner.events[evt_idx].clone();
            }
        }
        // okay now we use a write lock
        {
            let mut inner = self.inner.write();
            let pre_first = inner.evt_to_tick.peek().map(|v| v.1).copied();
            // repeat the code above because something might have changed by now
            if let Some(&evt_idx) = inner.tick_to_evt.get(&tick) {
                return inner.events[evt_idx].clone();
            }
            let evt = Arc::new(ManualResetEvent::new(false));
            let evt_idx = inner.events.insert(evt.clone());
            inner.tick_to_evt.insert(tick, evt_idx);
            inner.evt_to_tick.push(evt_idx, tick);
            // if the first tick changed, notify the waker
            let post_first = inner.lowest();
            if pre_first != post_first {
                self.waker.notify(1);
            }
            evt
        }
    }

    /// Waits until something changes
    pub fn wait_until_change(&self) -> EventListener {
        self.waker.listen()
    }

    /// The earliest tick.
    pub fn earliest_tick(&self) -> Option<u64> {
        self.inner.read().lowest()
    }

    /// Fires all events before a particular tick.
    pub fn fire_before(&self, tick: u64) {
        let mut inner = self.inner.write();
        while let Some((evt_id, evt_tick)) = inner.evt_to_tick.pop() {
            if evt_tick > tick {
                inner.evt_to_tick.push(evt_id, evt_tick);
                break;
            }
            inner.events.remove(evt_id).set();
            inner.tick_to_evt.remove(&tick);
        }
    }
}

#[derive(Default)]
pub struct Inner {
    events: Slab<Arc<ManualResetEvent>>,
    evt_to_tick: PriorityQueue<usize, u64>,
    tick_to_evt: HashMap<u64, usize>,
}

impl Inner {
    fn lowest(&self) -> Option<u64> {
        self.evt_to_tick.peek().map(|v| v.1).copied()
    }
}
