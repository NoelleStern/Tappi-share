use bytes::Bytes;
use std::sync::Arc;
use tokio::fs::File;
use tokio::sync::watch;
use tokio::io::AsyncReadExt;
use std::collections::VecDeque;
use tokio::sync::mpsc::UnboundedSender;
use webrtc::data_channel::RTCDataChannel;

use crate::client::message::Message;
use crate::app::event::{BasicEvent, BasicEventSenderExt};
use crate::app::app_event::{AppEventClient, DebugDataChannel};
use crate::app::file_manager::{FileProgressReport, OutputFile};


pub async fn send_all_meta(
    dc: Arc<RTCDataChannel>,
    files: &VecDeque<OutputFile>,
    chunk_size: usize,
    buffer_watch_rx: &mut watch::Receiver<bool>,
    sender: Option<&UnboundedSender<BasicEvent>>,
) -> color_eyre::Result<()> {
    for f in files {
        let meta_json: String = serde_json::to_string(&f.meta)?;
        send_meta_string(
            dc.clone(),
            &meta_json,
            f.id as u32,
            chunk_size,
            buffer_watch_rx,
        )
        .await?;

        if  f.meta.size == 0
            && let Some(sender) = sender {
                sender.send_event(AppEventClient::OutputFileProgress(FileProgressReport::new(
                    f.id, 1.0,
                ))).await;
            }
    }

    if let Some(sender) = sender {
        sender.send_event(AppEventClient::MetaSent(DebugDataChannel::new(dc))).await;
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
    send_data(
        dc.clone(),
        output_file,
        &mut file,
        chunk_size,
        buffer_watch_rx,
        sender,
    )
    .await?;

    // Send final file report and a file finished signal
    if let Some(sender) = sender {
        sender.send_event(AppEventClient::OutputFileProgress(FileProgressReport::new(
            output_file.id, 1.0,
        ))).await;
        sender.send_event(AppEventClient::OutputFileFinished(DebugDataChannel::new(
            dc.clone(),
        ))).await;
    }

    Ok(())
}

async fn send_meta_string(
    dc: Arc<RTCDataChannel>,
    meta_json: &String,
    file_id: u32,
    chunk_size: usize,
    buffer_watch_rx: &mut watch::Receiver<bool>,
) -> color_eyre::Result<()> {
    let bytes: &[u8] = meta_json.as_bytes();
    let string_size: usize = bytes.len();
    let mut counter: usize = 0;

    send_message(dc.clone(), buffer_watch_rx, Message::MetadataNotification(file_id)).await?;

    loop {
        if counter < string_size {
            let borrow_size: usize = chunk_size.min(string_size);
            let new_counter: usize = counter + borrow_size;
            let chunk = &bytes[counter..new_counter];
             
            send_binary(dc.clone(), buffer_watch_rx, &chunk).await?; // Send chunk
            counter = new_counter;
        } else {
            break;
        }
    }

    send_message(dc.clone(), buffer_watch_rx, Message::MetadataLast()).await?;

    Ok(())
}

async fn send_data(
    dc: Arc<RTCDataChannel>,
    output_file: &OutputFile,
    file: &mut File,
    chunk_size: usize,
    buffer_watch_rx: &mut watch::Receiver<bool>,
    sender: Option<&UnboundedSender<BasicEvent>>,
) -> color_eyre::Result<()> {
    let mut buf = vec![0u8; chunk_size];
    let mut counter: usize = 0;
    let file_size = output_file.meta.size;

    send_message(dc.clone(), buffer_watch_rx, Message::BinaryDataNotification(output_file.id as u32)).await?;

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 { break; } // EOF

        counter += n;
        let chunk = &buf[..n];

        // Send chunk
        send_binary(dc.clone(), buffer_watch_rx, &chunk).await?;

        // Report back
        if let Some(sender) = sender {
            let progress = ((counter as f64) / (file_size as f64)).clamp(0.0, 0.99); // I don't want it to show a 100 before it reaches it
            sender.send_event(AppEventClient::OutputFileProgress(FileProgressReport::new(
                output_file.id, progress,
            ))).await;
        }
    }

    send_message(dc.clone(), buffer_watch_rx, Message::BinaryDataLast()).await?;

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