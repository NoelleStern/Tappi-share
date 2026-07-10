use std::sync::Arc;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;
use std::collections::HashMap;
use tokio::sync::{Mutex, watch};
use std::ffi::{OsStr, OsString};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedSender;
use webrtc::data_channel::RTCDataChannel;
use std::fs::{self, File, create_dir_all};
use webrtc::data_channel::data_channel_message::DataChannelMessage;

use crate::app::event::BasicEvent;
use crate::client::payload::send_message;
use crate::app::app_event::AppEventClient;
use crate::app::event::BasicEventSenderExt;
use crate::app::file_manager::{FileId, SpeedReport};
use crate::app::file_manager::{FileProgressReport, InputFile, MetaData};


#[derive(Default)]
pub struct MessageHandlerState {
    mode: Mode,
    counter: SpeedCounterHandler,
}

pub struct SpeedCounterHandler {
    init_flag: bool,
    bytes: u32,
    last_speed_report: Instant,
}
impl Default for SpeedCounterHandler {
    fn default() -> Self {
        Self { init_flag: false, bytes: 0, last_speed_report: Instant::now() }
    }
}
impl SpeedCounterHandler {
    pub fn clock(&mut self, bytes: u32) -> bool {
        self.bytes += bytes;
        self.init_flag || self.last_speed_report.elapsed().as_secs() >= 3
    }
    pub fn reset(&mut self) {
        self.init_flag = false;
        self.bytes = 0;
        self.last_speed_report = Instant::now();
    }
}


#[derive(Default)]
pub enum Mode {
    #[default]
    None,
    Metadata(u32),
    BinaryData(u32),
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Message {
    /// TODO: reserved for potential future text chat functionality
    TextMessage(String),
    /// Notify the other side the following binary will contain file's metadata
    MetadataNotification(u32),
    MetadataLast(),
    /// Notify the other side the following binary will contain file's binary data
    BinaryDataNotification(u32),
    BinaryDataLast(),
    /// Speed-monitoring-related message
    SpeedReport(SpeedReport),
    /// To make sure a file was successfully delivered
    FileReceived(FileId),
}

// Handles files, folder structures, empty folders and empty files + file messages
pub async fn handle_message(
    msg: DataChannelMessage,
    channel: Arc<RTCDataChannel>,
    buffer_watch_rx: &mut watch::Receiver<bool>,
    sender: UnboundedSender<BasicEvent>,
    state: Arc<Mutex<MessageHandlerState>>,
    metadata_map: Arc<Mutex<HashMap<usize, MetaData>>>,
    metadata_bytes_map: Arc<Mutex<HashMap<usize, Vec<u8>>>>,
) -> color_eyre::Result<()> {
    match msg.is_string {
        // Handle messages (and a little bit of files)
        true => {
            let mut last_flag = false;
            let json = String::from_utf8(msg.data.to_vec())?;
            let message: Message = serde_json::from_str(&json)?;

            match message {
                Message::MetadataNotification(id) => { state.lock().await.mode = Mode::Metadata(id) },
                Message::BinaryDataNotification(id) => { state.lock().await.mode = Mode::BinaryData(id) },
                Message::MetadataLast() => last_flag = true,
                Message::BinaryDataLast() => last_flag = true,
                _ => ()
            }

            sender.send_event(AppEventClient::MessageReceived(message)).await;

            // Do stuff if last
            if last_flag {
                match state.lock().await.mode {
                    Mode::Metadata(id) => {
                        let id = id as usize;
                        let meta_bytes_map = metadata_bytes_map.lock().await;
                        if let Some(bytes) = meta_bytes_map.get(&id) {
                            //
                            let meta_string = String::from_utf8_lossy(bytes);
                            let mut metadata = metadata_map.lock().await;
                            let value: MetaData = serde_json::from_str(&meta_string)?;
                            metadata.insert(id, value.clone());
                            create_folder_structure(&value)?;

                            if !value.is_dir {
                                if value.size > 0 {
                                    sender.send_event(AppEventClient::InputFileNew(InputFile::new(
                                        id, value,
                                    ))).await;
                                } else {
                                    create_file(value.get_path(), false)?;
                                    sender.send_event(AppEventClient::InputFileNew(InputFile::new(
                                        id, value,
                                    ))).await; // Creates the file in the UI
                                    sender.send_event(AppEventClient::InputFileProgress(
                                        FileProgressReport::new(id, 1.0),
                                    )).await; // Updates the progress
                                    send_message(
                                        channel.clone(),
                                        buffer_watch_rx,
                                        Message::FileReceived(id),
                                    ).await?; // Reports back
                                }
                            } else {
                                // Report to the other client
                                send_message(
                                    channel.clone(),
                                    buffer_watch_rx,
                                    Message::FileReceived(id),
                                ).await?; // Should be fine
                            }
                        }
                    },
                    Mode::BinaryData(id) => {
                        let id = id as usize;
                        let mut metadata = metadata_map.lock().await;
                        if let Some(metadata) = metadata.get_mut(&id) {
                            remove_part_ext(metadata.get_path())?;
                        }

                        // Report to the other client
                        send_message(
                            channel.clone(),
                            buffer_watch_rx,
                            Message::FileReceived(id),
                        ).await?;
                    },
                    _ => ()
                }

            }
        }
        // Handle file meta and data
        false => {
            // Process the data
            let mut state_locked = state.lock().await;
            match state_locked.mode {
                Mode::Metadata(id) => {
                    let id = id as usize;
                    let metadata = metadata_map.lock().await;

                    // Ignore if it's already in
                    if metadata.get(&id).is_none() {
                        let mut meta_bytes_map = metadata_bytes_map.lock().await; // lock mutex
                        if let Some(bytes) = meta_bytes_map.get_mut(&id) {
                            bytes.extend(msg.data);
                        } else {
                            meta_bytes_map.insert(id, msg.data.to_vec());
                        }
                    }
                },
                Mode::BinaryData(id) => {
                    let id = id as usize;
                    let mut metadata_map = metadata_map.lock().await;
                    if let Some(metadata) = metadata_map.get_mut(&id) {
                        metadata.progress_bytes += msg.data.len();
                        append_data_to_file(metadata.get_path(), &msg.data)?;

                        let progress = (metadata.progress_bytes as f64) / (metadata.size as f64);
                        sender.send_event(AppEventClient::InputFileProgress(FileProgressReport::new(id, progress))).await;

                        // Report speed every few seconds
                        if state_locked.counter.clock(msg.data.len() as u32) {
                            sender.send_event(AppEventClient::ReportFileSpeed(SpeedReport::new(state_locked.counter.bytes))).await;
                            send_message(
                                channel.clone(), buffer_watch_rx,
                                Message::SpeedReport(SpeedReport::new(state_locked.counter.bytes)),
                            ).await?;
                            state_locked.counter.reset();
                        }
                    }
                },
                _ => ()
            }
        }
    }

    Ok(())
}

fn create_folder_structure(metadata: &MetaData) -> color_eyre::Result<()> {
    if metadata.is_dir {
        create_dir_all(metadata.get_path())?;
    } else if let Some(parent) = metadata.get_path().parent()
        && !parent.exists() && parent.to_string_lossy() != ""
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
