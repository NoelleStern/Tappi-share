use bytes::Bytes;
use rmpp::encode;
use rmpp::types::{MsgPackEntry, MsgPackValue};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;
use webrtc::data_channel::RTCDataChannel;

use crate::app::app_event::{AppEventClient, DebugDataChannel};
use crate::app::event::{BasicEvent, BasicEventSenderExt};
use crate::app::file_manager::{FileProgressReport, OutputFile};
use crate::client::message::Message;

// TODO: make overhead minimal, probably using something else than MessagePack
/// Payload base length excluding the data
///
/// fix_array:  1
/// id_u32:     5
/// meta_bool:  1
/// last_bool:  1
/// data_bin32: 5
///
/// ----------> 13 bytes
///
/// Not the biggest overhead!
pub const BASE_LENGTH: usize = 13;

/// Creates a basic MsgPackEntry, primarily for testing
#[allow(dead_code)]
fn get_base_entry() -> MsgPackEntry {
    MsgPackEntry::new(
        0,
        MsgPackValue::FixArray(vec![
            MsgPackEntry::new(0, MsgPackValue::U32(0)),
            MsgPackEntry::new(0, MsgPackValue::Bool(false)),
            MsgPackEntry::new(0, MsgPackValue::Bool(false)),
            MsgPackEntry::new(0, MsgPackValue::Bin32(vec![])),
        ]),
    )
}

/// Get basic MsgPackEntry byte length, primarily for testing
#[allow(dead_code)]
fn get_base_length() -> usize {
    encode::pack(&get_base_entry()).len()
}

/// Packs MsgPackEntry into binary
fn pack(id: u32, meta: bool, last: bool, chunk: Vec<u8>) -> Vec<u8> {
    encode::pack(&MsgPackEntry::new(
        0,
        MsgPackValue::FixArray(vec![
            MsgPackEntry::new(0, MsgPackValue::U32(id)),
            MsgPackEntry::new(0, MsgPackValue::Bool(meta)),
            MsgPackEntry::new(0, MsgPackValue::Bool(last)),
            MsgPackEntry::new(0, MsgPackValue::Bin32(chunk)), // Both meta and data can be represented by binary
        ]),
    ))
}

pub async fn send_all_meta(
    dc: Arc<RTCDataChannel>,
    files: &VecDeque<OutputFile>,
    chunk_size: usize,
    buffer_watch_rx: &mut watch::Receiver<bool>,
    sender: Option<&UnboundedSender<BasicEvent>>,
) -> color_eyre::Result<()> {
    for f in files {
        let meta_json = serde_json::to_string(&f.meta)?;
        let buffer_size = chunk_size - BASE_LENGTH;
        send_meta_string(
            dc.clone(),
            &meta_json,
            f.id as u32,
            buffer_size,
            buffer_watch_rx,
        )
        .await?;

        if f.meta.size == 0
            && let Some(sender) = sender
        {
            sender
                .send_event(AppEventClient::OutputFileProgress(FileProgressReport::new(
                    f.id, 1.0,
                )))
                .await;
        }
    }

    if let Some(sender) = sender {
        sender
            .send_event(AppEventClient::MetaSent(DebugDataChannel::new(dc)))
            .await;
    }

    Ok(())
}

pub async fn send_file_data(
    dc: Arc<RTCDataChannel>,
    output_file: &OutputFile,
    chunk_size: usize,
    buffer_watch_rx: &mut watch::Receiver<bool>,
    sender: Option<&UnboundedSender<BasicEvent>>,
) -> color_eyre::Result<()> {
    let mut file = File::open(&output_file.meta.path).await?;
    let buffer_size = chunk_size - BASE_LENGTH;
    send_data(
        dc.clone(),
        output_file,
        &mut file,
        buffer_size,
        buffer_watch_rx,
        sender,
    )
    .await?;

    // Send final file report and a file finished signal
    if let Some(sender) = sender {
        sender
            .send_event(AppEventClient::OutputFileProgress(FileProgressReport::new(
                output_file.id,
                1.0,
            )))
            .await;
        sender
            .send_event(AppEventClient::OutputFileFinished(DebugDataChannel::new(
                dc.clone(),
            )))
            .await;
    }

    Ok(())
}

async fn send_meta_string(
    dc: Arc<RTCDataChannel>,
    meta_json: &String,
    file_id: u32,
    buffer_size: usize,
    buffer_watch_rx: &mut watch::Receiver<bool>,
) -> color_eyre::Result<()> {
    let bytes: &[u8] = meta_json.as_bytes();
    let string_size: usize = bytes.len();
    let mut counter: usize = 0;

    loop {
        if counter < string_size {
            let borrow_size: usize = buffer_size.min(string_size);
            let new_counter: usize = counter + borrow_size;
            let chunk = &bytes[counter..new_counter];

            let packed = pack(file_id, true, borrow_size >= string_size, chunk.to_vec());

            // Send chunk
            send_binary(dc.clone(), buffer_watch_rx, &packed).await?;

            counter = new_counter;
        } else {
            break;
        }
    }

    Ok(())
}

async fn send_data(
    dc: Arc<RTCDataChannel>,
    output_file: &OutputFile,
    file: &mut File,
    buffer_size: usize,
    buffer_watch_rx: &mut watch::Receiver<bool>,
    sender: Option<&UnboundedSender<BasicEvent>>,
) -> color_eyre::Result<()> {
    let mut buf = vec![0u8; buffer_size];
    let mut counter: usize = 0;
    let file_size = output_file.meta.size;

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        } // EOF

        counter += n;

        let chunk = &buf[..n];
        let packed = pack(
            output_file.id as u32,
            false,
            counter >= file_size,
            chunk.to_vec(),
        );

        // Send chunk
        send_binary(dc.clone(), buffer_watch_rx, &packed).await?;

        // Report back
        if let Some(sender) = sender {
            let progress = ((counter as f64) / (file_size as f64)).clamp(0.0, 0.99); // I don't want it to show a 100 before it reaches it
            sender
                .send_event(AppEventClient::OutputFileProgress(FileProgressReport::new(
                    output_file.id,
                    progress,
                )))
                .await;
        }
    }

    Ok(())
}

pub async fn send_message(
    dc: Arc<RTCDataChannel>,
    buffer_watch_rx: &mut watch::Receiver<bool>,
    message: Message,
) -> color_eyre::Result<()> {
    await_threshold(dc.clone(), buffer_watch_rx).await?;
    let message_json = serde_json::to_string(&message)?;
    dc.send_text(message_json).await?;
    Ok(())
}
async fn send_binary(
    dc: Arc<RTCDataChannel>,
    buffer_watch_rx: &mut watch::Receiver<bool>,
    binary: &[u8],
) -> color_eyre::Result<()> {
    await_threshold(dc.clone(), buffer_watch_rx).await?;
    dc.send(&Bytes::copy_from_slice(binary)).await?;
    Ok(())
}

async fn await_threshold(
    dc: Arc<RTCDataChannel>,
    buffer_watch_rx: &mut watch::Receiver<bool>,
) -> color_eyre::Result<()> {
    if dc.buffered_amount().await > dc.buffered_amount_low_threshold().await {
        buffer_watch_rx.changed().await?; // Await a change of any kind
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_length() {
        assert_eq!(get_base_length(), BASE_LENGTH);
    }
}
