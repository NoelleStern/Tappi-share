use rmpp::MsgPackEntry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, create_dir_all};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::{Mutex, watch};
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;

use crate::app::app_event::AppEventClient;
use crate::app::event::BasicEvent;
use crate::app::event::BasicEventSenderExt;
use crate::app::file_manager::{FileId, SpeedReport};
use crate::app::file_manager::{FileProgressReport, InputFile, MetaData};
use crate::client::packet;
use crate::client::payload::send_message;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Message {
    TextMessage(String), // TODO: reserved for potential future text chat functionality
    FilePacketReceived(SpeedReport), // Speed-monitoring-related message
    FileReceived(FileId), // To make sure a file was successfully delivered
}

// Handles files, folder structures, empty folders and empty files + file messages
pub async fn handle_message(
    msg: DataChannelMessage,
    channel: Arc<RTCDataChannel>,
    buffer_watch_rx: &mut watch::Receiver<bool>,
    sender: UnboundedSender<BasicEvent>,
    metadata_map: Arc<Mutex<HashMap<usize, MetaData>>>,
    metadata_bytes_map: Arc<Mutex<HashMap<usize, Vec<u8>>>>,
) -> color_eyre::Result<()> {
    match msg.is_string {
        // Handle messages
        true => {
            let json = String::from_utf8(msg.data.to_vec())?;
            let message: Message = serde_json::from_str(&json)?;
            sender
                .send_event(AppEventClient::MessageReceived(message))
                .await;
        }
        // Handle file meta and data
        false => {
            let entry: MsgPackEntry = rmpp::unpack(&msg.data)?;
            let packet = packet::Packet::new(entry)?;

            // Process the data
            if packet.meta {
                // Metadata
                let metadata = metadata_map.lock().await;

                // Ignore if it's already in
                if metadata.get(&packet.id).is_none() {
                    let mut meta_bytes_map = metadata_bytes_map.lock().await; // lock mutex
                    if let Some(bytes) = meta_bytes_map.get_mut(&packet.id) {
                        bytes.extend(packet.binary);
                    } else {
                        meta_bytes_map.insert(packet.id, packet.binary);
                    }
                }
            } else {
                // File data
                let mut metadata_map = metadata_map.lock().await;
                if let Some(metadata) = metadata_map.get_mut(&packet.id) {
                    metadata.progress_bytes += packet.binary.len();
                    append_data_to_file(metadata.get_path(), &packet.binary)?;

                    let progress = (metadata.progress_bytes as f64) / (metadata.size as f64);
                    sender
                        .send_event(AppEventClient::InputFileProgress(FileProgressReport::new(
                            packet.id, progress,
                        )))
                        .await;
                    sender
                        .send_event(AppEventClient::ReportFileSpeed(SpeedReport::new(
                            packet.id,
                            packet.binary.len(),
                        )))
                        .await;

                    // Report to the other client
                    send_message(
                        channel.clone(),
                        buffer_watch_rx,
                        Message::FilePacketReceived(SpeedReport::new(
                            packet.id,
                            packet.binary.len(),
                        )),
                    )
                    .await?;
                }
            }

            // Do stuff if last
            if packet.last {
                if packet.meta {
                    let meta_bytes_map = metadata_bytes_map.lock().await;
                    if let Some(bytes) = meta_bytes_map.get(&packet.id) {
                        //
                        let meta_string = String::from_utf8_lossy(bytes);
                        let mut metadata = metadata_map.lock().await;
                        let value: MetaData = serde_json::from_str(&meta_string)?;
                        metadata.insert(packet.id, value.clone());
                        create_folder_structure(&value)?;

                        if !value.is_dir {
                            if value.size > 0 {
                                sender
                                    .send_event(AppEventClient::InputFileNew(InputFile::new(
                                        packet.id, value,
                                    )))
                                    .await;
                            } else {
                                create_file(value.get_path(), false)?;
                                sender
                                    .send_event(AppEventClient::InputFileNew(InputFile::new(
                                        packet.id, value,
                                    )))
                                    .await; // Creates the file in the UI
                                sender
                                    .send_event(AppEventClient::InputFileProgress(
                                        FileProgressReport::new(packet.id, 1.0),
                                    ))
                                    .await; // Updates the progress
                                send_message(
                                    channel.clone(),
                                    buffer_watch_rx,
                                    Message::FileReceived(packet.id),
                                )
                                .await?; // Reports back
                            }
                        } else {
                            // Report to the other client
                            send_message(
                                channel.clone(),
                                buffer_watch_rx,
                                Message::FileReceived(packet.id),
                            )
                            .await?; // Should be fine
                        }
                    }
                } else {
                    let mut metadata = metadata_map.lock().await;
                    if let Some(metadata) = metadata.get_mut(&packet.id) {
                        remove_part_ext(metadata.get_path())?;
                    }

                    // Report to the other client
                    send_message(
                        channel.clone(),
                        buffer_watch_rx,
                        Message::FileReceived(packet.id),
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}

fn create_folder_structure(metadata: &MetaData) -> color_eyre::Result<()> {
    if metadata.is_dir {
        create_dir_all(metadata.get_path())?;
    } else if let Some(parent) = metadata.get_path().parent()
        && !parent.exists()
        && parent.to_string_lossy() != ""
    {
        create_dir_all(parent)?;
    }

    Ok(())
}

fn create_file(path: PathBuf, append_part: bool) -> color_eyre::Result<File> {
    // Couldn't create a file without wright permissions, but .append(true) provides those
    let p = if append_part {
        append_part_ext(path)
    } else {
        path
    };
    Ok(fs::OpenOptions::new().create(true).append(true).open(p)?)
}
fn append_data_to_file(path: PathBuf, data: &[u8]) -> color_eyre::Result<()> {
    let mut file = create_file(path, true)?;
    file.write_all(data)?;
    Ok(())
}

// https://internals.rust-lang.org/t/pathbuf-has-set-extension-but-no-add-extension-cannot-cleanly-turn-tar-to-tar-gz/14187/10
pub fn append_ext(ext: impl AsRef<OsStr>, path: PathBuf) -> PathBuf {
    let mut os_string: OsString = path.into();
    os_string.push(".");
    os_string.push(ext.as_ref());
    os_string.into()
}
pub fn append_part_ext(path: PathBuf) -> PathBuf {
    append_ext("part", path)
}
pub fn remove_part_ext(path: PathBuf) -> color_eyre::Result<()> {
    fs::rename(append_part_ext(path.clone()), path)?;
    Ok(())
}
