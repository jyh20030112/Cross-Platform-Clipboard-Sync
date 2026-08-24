use crate::error::{AppError, AppResult};
use crate::protocol::MAX_PAYLOAD_BYTES;
use arboard::{Clipboard, ImageData};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use std::borrow::Cow;
use std::io::Cursor;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const REMOTE_SUPPRESSION: Duration = Duration::from_millis(1200);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClipboardItem {
    Text(String),
    Png(Vec<u8>),
}

impl ClipboardItem {
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        match self {
            Self::Text(text) => {
                hasher.update(b"text\0");
                hasher.update(text.as_bytes());
            }
            Self::Png(png) => {
                hasher.update(b"png\0");
                hasher.update(png);
            }
        }
        *hasher.finalize().as_bytes()
    }

    pub fn into_parts(self) -> (crate::protocol::ClipboardKind, Vec<u8>) {
        match self {
            Self::Text(text) => (crate::protocol::ClipboardKind::Text, text.into_bytes()),
            Self::Png(png) => (crate::protocol::ClipboardKind::Png, png),
        }
    }
}

pub trait ClipboardBackend: Send + 'static {
    fn snapshot(&mut self) -> AppResult<Option<ClipboardItem>>;
    fn apply(&mut self, item: &ClipboardItem) -> AppResult<()>;
}

pub struct ArboardBackend {
    clipboard: Clipboard,
}

impl ArboardBackend {
    pub fn new() -> AppResult<Self> {
        Ok(Self {
            clipboard: Clipboard::new().map_err(|error| AppError::Clipboard(error.to_string()))?,
        })
    }
}

impl ClipboardBackend for ArboardBackend {
    fn snapshot(&mut self) -> AppResult<Option<ClipboardItem>> {
        if let Ok(image) = self.clipboard.get_image() {
            let png = encode_png(image.width, image.height, image.bytes.as_ref())?;
            if png.len() > MAX_PAYLOAD_BYTES {
                return Err(AppError::Clipboard("image exceeds payload limit".into()));
            }
            return Ok(Some(ClipboardItem::Png(png)));
        }

        match self.clipboard.get_text() {
            Ok(text) if !text.is_empty() => Ok(Some(ClipboardItem::Text(text))),
            Ok(_) => Ok(None),
            Err(_) => Ok(None),
        }
    }

    fn apply(&mut self, item: &ClipboardItem) -> AppResult<()> {
        match item {
            ClipboardItem::Text(text) => self
                .clipboard
                .set_text(text)
                .map_err(|error| AppError::Clipboard(error.to_string())),
            ClipboardItem::Png(png) => {
                let image = image::load_from_memory(png)
                    .map_err(|error| AppError::Clipboard(format!("invalid PNG: {error}")))?
                    .to_rgba8();
                let data = ImageData {
                    width: image.width() as usize,
                    height: image.height() as usize,
                    bytes: Cow::Owned(image.into_raw()),
                };
                self.clipboard
                    .set_image(data)
                    .map_err(|error| AppError::Clipboard(error.to_string()))
            }
        }
    }
}

fn encode_png(width: usize, height: usize, rgba: &[u8]) -> AppResult<Vec<u8>> {
    if width > u32::MAX as usize || height > u32::MAX as usize {
        return Err(AppError::Clipboard(
            "image dimensions exceed PNG limits".into(),
        ));
    }
    let expected = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| AppError::Clipboard("image dimensions overflow".into()))?;
    if expected != rgba.len() {
        return Err(AppError::Clipboard(
            "clipboard image has invalid dimensions".into(),
        ));
    }

    let image = ImageBuffer::<Rgba<u8>, _>::from_raw(width as u32, height as u32, rgba.to_vec())
        .ok_or_else(|| AppError::Clipboard("could not construct RGBA image".into()))?;
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|error| AppError::Clipboard(format!("PNG encoding failed: {error}")))?;
    Ok(output.into_inner())
}

pub enum ClipboardCommand {
    Apply(ClipboardItem),
    Shutdown,
}

pub fn spawn_worker(
    mut backend: impl ClipboardBackend,
) -> (
    Sender<ClipboardCommand>,
    Receiver<ClipboardItem>,
    JoinHandle<()>,
) {
    let (command_tx, command_rx) = mpsc::channel();
    let (local_tx, local_rx) = mpsc::channel();
    let thread = thread::spawn(move || {
        let mut last_fingerprint = None;
        let mut suppress_until = Instant::now();
        let mut next_poll = Instant::now();

        loop {
            match command_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(ClipboardCommand::Apply(item)) => {
                    if let Err(error) = backend.apply(&item) {
                        warn!(%error, "failed to apply remote clipboard item");
                    } else {
                        last_fingerprint = Some(item.fingerprint());
                        suppress_until = Instant::now() + REMOTE_SUPPRESSION;
                    }
                }
                Ok(ClipboardCommand::Shutdown) => break,
                Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }

            if Instant::now() < next_poll {
                continue;
            }
            next_poll = Instant::now() + POLL_INTERVAL;

            match backend.snapshot() {
                Ok(Some(item)) => {
                    let fingerprint = item.fingerprint();
                    if Some(fingerprint) == last_fingerprint || Instant::now() < suppress_until {
                        continue;
                    }
                    last_fingerprint = Some(fingerprint);
                    if local_tx.send(item).is_err() {
                        debug!("clipboard event receiver stopped");
                        break;
                    }
                }
                Ok(None) => {}
                Err(error) => debug!(%error, "clipboard snapshot unavailable"),
            }
        }
    });
    (command_tx, local_rx, thread)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_fingerprint_distinguishes_content_type() {
        assert_ne!(
            ClipboardItem::Text("abc".into()).fingerprint(),
            ClipboardItem::Png(b"abc".to_vec()).fingerprint()
        );
    }

    #[test]
    fn png_encoder_rejects_inconsistent_dimensions() {
        let result = encode_png(2, 2, &[0; 3]);
        assert!(result.is_err());
    }
}
