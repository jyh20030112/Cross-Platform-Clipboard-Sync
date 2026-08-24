use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FRAME_BYTES: usize = MAX_PAYLOAD_BYTES + 4096;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hello {
    pub protocol_version: u16,
    pub device_id: Uuid,
    pub listen_port: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Version {
    pub counter: u64,
    pub device_id: Uuid,
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.counter
            .cmp(&other.counter)
            .then_with(|| self.device_id.cmp(&other.device_id))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ClipboardKind {
    Text,
    Png,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardEvent {
    pub event_id: Uuid,
    pub origin: Uuid,
    pub version: Version,
    pub kind: ClipboardKind,
    pub payload: Vec<u8>,
    pub digest: [u8; 32],
}

impl ClipboardEvent {
    pub fn new(
        origin: Uuid,
        counter: u64,
        kind: ClipboardKind,
        payload: Vec<u8>,
    ) -> AppResult<Self> {
        if payload.len() > MAX_PAYLOAD_BYTES {
            return Err(AppError::Protocol(format!(
                "payload is {} bytes, maximum is {}",
                payload.len(),
                MAX_PAYLOAD_BYTES
            )));
        }

        let digest = *blake3::hash(&payload).as_bytes();
        Ok(Self {
            event_id: Uuid::new_v4(),
            origin,
            version: Version {
                counter,
                device_id: origin,
            },
            kind,
            payload,
            digest,
        })
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(AppError::Protocol(
                "payload exceeds configured limit".into(),
            ));
        }
        if self.origin != self.version.device_id {
            return Err(AppError::Protocol(
                "origin and version device differ".into(),
            ));
        }
        if *blake3::hash(&self.payload).as_bytes() != self.digest {
            return Err(AppError::Protocol("payload digest mismatch".into()));
        }
        if matches!(self.kind, ClipboardKind::Text) && std::str::from_utf8(&self.payload).is_err() {
            return Err(AppError::Protocol("text payload is not valid UTF-8".into()));
        }
        if matches!(self.kind, ClipboardKind::Png) && self.payload.is_empty() {
            return Err(AppError::Protocol("PNG payload cannot be empty".into()));
        }
        if matches!(self.kind, ClipboardKind::Png)
            && !self.payload.starts_with(b"\x89PNG\r\n\x1a\n")
        {
            return Err(AppError::Protocol(
                "PNG payload has an invalid signature".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WireMessage {
    Hello(Hello),
    Event(ClipboardEvent),
    StateRequest { known: Option<Version> },
    State { latest: Option<ClipboardEvent> },
    Ping(u64),
    Pong(u64),
}

impl WireMessage {
    pub fn encode(&self) -> AppResult<Vec<u8>> {
        Ok(bincode::serialize(self)?)
    }

    pub fn decode(frame: &[u8]) -> AppResult<Self> {
        if frame.len() > MAX_FRAME_BYTES {
            return Err(AppError::Protocol("frame exceeds configured limit".into()));
        }
        let message: Self = bincode::deserialize(frame)?;
        if let Self::Event(event) = &message {
            event.validate()?;
        }
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_event_as_binary() {
        let device = Uuid::new_v4();
        let event =
            ClipboardEvent::new(device, 1, ClipboardKind::Text, "hello".as_bytes().to_vec())
                .unwrap();
        let decoded =
            WireMessage::decode(&WireMessage::Event(event.clone()).encode().unwrap()).unwrap();
        assert_eq!(decoded, WireMessage::Event(event));
    }

    #[test]
    fn rejects_tampered_payload() {
        let device = Uuid::new_v4();
        let mut event =
            ClipboardEvent::new(device, 1, ClipboardKind::Text, b"hello".to_vec()).unwrap();
        event.payload = b"tampered".to_vec();
        let result = WireMessage::decode(&WireMessage::Event(event).encode().unwrap());
        assert!(matches!(result, Err(AppError::Protocol(message)) if message.contains("digest")));
    }

    #[test]
    fn rejects_oversized_payload() {
        let result = ClipboardEvent::new(
            Uuid::new_v4(),
            1,
            ClipboardKind::Png,
            vec![0; MAX_PAYLOAD_BYTES + 1],
        );
        assert!(result.is_err());
    }

    #[test]
    fn version_order_is_deterministic_for_concurrent_events() {
        let a = Version {
            counter: 7,
            device_id: Uuid::from_u128(1),
        };
        let b = Version {
            counter: 7,
            device_id: Uuid::from_u128(2),
        };
        assert!(a < b);
    }
}
