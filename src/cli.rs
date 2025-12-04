use clap::{Args, Parser, Subcommand};
use log::LevelFilter;
use std::{net::SocketAddr, path::PathBuf};

use crate::app::encrypt::Secret;

/// Cli parser
#[derive(Parser, Clone, Debug)]
#[command(version, about, long_about = None, name = "tappi-share")]
pub struct Cli {
    /// Logging level (off/error/warn/info/debug)
    #[arg(short = 'l', long, default_value = "off")]
    pub log_level: LevelFilter,
    /// Log filename
    #[arg(short = 'f', long, default_value = "tappi-share.log")]
    pub log_file: String,

    /// Application mode
    #[command(subcommand)]
    pub app_mode: Commands,
}

/// Subcommands
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Clone, Debug)]
pub enum Commands {
    /// Start file sharing client
    Client(ClientArgs),
    /// Start signaling server
    Server(ServerArgs),
}

#[derive(Args, Clone, Debug)]
pub struct ClientArgs {
    /// Path(s) to the file(s) to upload
    #[arg(short='f', long, num_args = 1.., value_terminator(";"))]
    pub files: Option<Vec<PathBuf>>,
    /// Size in KiB to break the data into chunks by (valid range: 8–64)
    #[arg(short='s', long, default_value = "64", value_parser = parse_kib)]
    pub chunk_size: usize,
    /// Ignore sending empty folders
    #[arg(short = 'i', long, default_value = "false")]
    pub ignore_empty: bool,
    /// Additional STUN/TURN server(s)
    #[arg(short='a', long, num_args = 1.., value_terminator(";"))]
    pub additional_servers: Option<Vec<String>>,
    /// Additional STUN/TURN username
    #[arg(short = 'u', long)]
    pub username: Option<String>,
    /// Additional STUN/TURN credential
    #[arg(short = 'c', long)]
    pub credential: Option<String>,

    /// Signaling solution
    #[command(subcommand)]
    pub signaling_mode: SignalingSolutions,
}

/// Signaling commands
#[derive(Args, Clone, Debug)]
pub struct ServerArgs {
    /// Address and port to host the server on
    #[arg(short = 'a', long, default_value = "127.0.0.1:3030")]
    pub address: SocketAddr,
}

#[derive(Subcommand, Clone, Debug)]
pub enum SignalingSolutions {
    /// Exchange the handshake manually
    Manual(SignalingSolutionManualArgs),
    /// Exchange the handshake using a WebSocket signaling server
    Socket(SignalingSolutionSocketArgs),
    /// Exchange the handshake using an MQTT broker
    Mqtt(SignalingSolutionMqttArgs),
}
#[derive(Args, Clone, Debug)]
pub struct SignalingSolutionManualArgs {
    /// Force being polite during the negotiation. One of the peers has to be polite
    #[arg(short = 'p', long, default_value = "false")]
    pub polite: bool, // Polite is answering and impolite is offering
    /// Encryption secret key, must be 32 characters long
    #[arg(short = 's', long)]
    pub secret: Option<Secret>,
}
#[derive(Args, Clone, Debug)]
pub struct SignalingSolutionSocketArgs {
    /// Address of the signaling server
    #[arg(short = 'a', long, default_value = "127.0.0.1")]
    // This default value is handy for testing so i'll leave it
    pub address: String,
    /// A server port number
    #[arg(short = 'p', long, default_value = "3030")]
    pub port: u16,
    /// Name of the room
    #[arg(short = 'r', long)]
    pub room: String,
}
#[derive(Args, Clone, Debug)]
pub struct SignalingSolutionMqttArgs {
    /// Broker address
    #[arg(short = 'b', long, default_value = "broker.emqx.io")]
    // I don't yet have a preference, so let's go with this one
    pub broker: String,
    /// Broker port number
    #[arg(short = 'p', long, default_value = "1883")]
    pub port: u16,
    /// MQTT topic to pass signaling data through (basically acts as a room name)
    #[arg(short = 't', long, default_value = "inbox")]
    pub topic: String,
    /// Local device MQTT name
    #[arg(short = 'l', long)]
    pub local_name: String,
    /// Remote device MQTT name
    #[arg(short = 'r', long)]
    pub remote_name: String,
    /// Encryption secret key, must be 32 characters long
    #[arg(short = 's', long)]
    pub secret: Option<Secret>,
    /// MQTT keep alive period in seconds
    #[arg(short = 'k', long, default_value = "5")]
    pub keep_alive: u16,
}
impl SignalingSolutionMqttArgs {
    pub fn local_topic(&self) -> String {
        format!("{}/{}", self.local_name, self.topic)
    }
    pub fn remote_topic(&self) -> String {
        format!("{}/{}", self.remote_name, self.topic)
    }
}

fn parse_kib(s: &str) -> Result<usize, String> {
    let kib: u64 = s
        .parse()
        .map_err(|_| "Expected an integer KiB value".to_string())?;
    let bytes = kib * 1024; // Convert kibibytes to bytes
    let result = bytes.clamp(8_192, 65_535) as usize; // 65535 bytes or 64KiB-1B is the max SCTP chunk size
    Ok(result)
}
