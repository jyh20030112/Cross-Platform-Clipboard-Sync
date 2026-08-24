use crate::error::AppResult;
use crate::protocol::{ClipboardEvent, ClipboardKind, Version};
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

const SEEN_EVENT_CAPACITY: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteEventResult {
    Duplicate,
    ForwardOnly,
    ApplyAndForward,
}

#[derive(Debug)]
struct SeenEvents {
    ids: HashSet<Uuid>,
    order: VecDeque<Uuid>,
}

impl SeenEvents {
    fn new() -> Self {
        Self {
            ids: HashSet::new(),
            order: VecDeque::new(),
        }
    }

    fn insert(&mut self, id: Uuid) -> bool {
        if !self.ids.insert(id) {
            return false;
        }
        self.order.push_back(id);
        if self.order.len() > SEEN_EVENT_CAPACITY {
            if let Some(expired) = self.order.pop_front() {
                self.ids.remove(&expired);
            }
        }
        true
    }
}

#[derive(Debug)]
pub struct SyncEngine {
    device_id: Uuid,
    logical_clock: u64,
    current: Option<ClipboardEvent>,
    seen: SeenEvents,
}

impl SyncEngine {
    pub fn new(device_id: Uuid) -> Self {
        Self {
            device_id,
            logical_clock: 0,
            current: None,
            seen: SeenEvents::new(),
        }
    }

    pub fn current(&self) -> Option<&ClipboardEvent> {
        self.current.as_ref()
    }

    pub fn local_event(
        &mut self,
        kind: ClipboardKind,
        payload: Vec<u8>,
    ) -> AppResult<ClipboardEvent> {
        self.logical_clock = self.logical_clock.saturating_add(1);
        let event = ClipboardEvent::new(self.device_id, self.logical_clock, kind, payload)?;
        self.seen.insert(event.event_id);
        self.current = Some(event.clone());
        Ok(event)
    }

    pub fn accept_remote(&mut self, event: ClipboardEvent) -> AppResult<RemoteEventResult> {
        event.validate()?;
        self.logical_clock = self
            .logical_clock
            .max(event.version.counter)
            .saturating_add(1);

        if !self.seen.insert(event.event_id) {
            return Ok(RemoteEventResult::Duplicate);
        }

        if self
            .current
            .as_ref()
            .is_none_or(|current| event.version > current.version)
        {
            self.current = Some(event);
            Ok(RemoteEventResult::ApplyAndForward)
        } else {
            Ok(RemoteEventResult::ForwardOnly)
        }
    }

    pub fn should_send_state(&self, known: Option<&Version>) -> bool {
        match (self.current.as_ref(), known) {
            (Some(_), None) => true,
            (None, _) => false,
            (Some(current), Some(known)) => current.version > *known,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_event_is_not_applied_twice() {
        let origin = Uuid::new_v4();
        let mut receiver = SyncEngine::new(Uuid::new_v4());
        let mut sender = SyncEngine::new(origin);
        let event = sender
            .local_event(ClipboardKind::Text, b"hello".to_vec())
            .unwrap();

        assert_eq!(
            receiver.accept_remote(event.clone()).unwrap(),
            RemoteEventResult::ApplyAndForward
        );
        assert_eq!(
            receiver.accept_remote(event).unwrap(),
            RemoteEventResult::Duplicate
        );
    }

    #[test]
    fn older_event_is_forwarded_but_not_applied() {
        let origin_a = Uuid::from_u128(1);
        let origin_b = Uuid::from_u128(2);
        let mut receiver = SyncEngine::new(Uuid::new_v4());
        let mut sender_a = SyncEngine::new(origin_a);
        let mut sender_b = SyncEngine::new(origin_b);
        let first = sender_a
            .local_event(ClipboardKind::Text, b"first".to_vec())
            .unwrap();
        let second = sender_b
            .local_event(ClipboardKind::Text, b"second".to_vec())
            .unwrap();

        let (newer, older) = if first.version > second.version {
            (first, second)
        } else {
            (second, first)
        };
        assert_eq!(
            receiver.accept_remote(newer.clone()).unwrap(),
            RemoteEventResult::ApplyAndForward
        );
        assert_eq!(
            receiver.accept_remote(older).unwrap(),
            RemoteEventResult::ForwardOnly
        );
        assert_eq!(receiver.current(), Some(&newer));
    }

    #[test]
    fn clock_advances_after_remote_event() {
        let remote_id = Uuid::new_v4();
        let mut receiver = SyncEngine::new(Uuid::new_v4());
        let remote =
            ClipboardEvent::new(remote_id, 100, ClipboardKind::Text, b"x".to_vec()).unwrap();
        receiver.accept_remote(remote).unwrap();
        let local = receiver
            .local_event(ClipboardKind::Text, b"local".to_vec())
            .unwrap();
        assert!(local.version.counter > 100);
    }
}
