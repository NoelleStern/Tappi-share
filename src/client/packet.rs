use color_eyre::eyre::eyre;
use rmpp::{MsgPackEntry, MsgPackValue};

#[derive(Clone, Debug)]
pub struct Packet {
    pub id: usize,
    pub meta: bool,
    pub last: bool,
    pub binary: Vec<u8>,
}
impl Packet {
    pub fn new(entry: MsgPackEntry) -> color_eyre::Result<Self> {
        let array: Vec<MsgPackEntry> = get_vec(&entry)?;

        Ok(Self {
            id: get_u32(&array[0])? as usize,
            meta: get_bool(&array[1])?,
            last: get_bool(&array[2])?,
            binary: get_bin32(&array[3])?,
        })
    }
}

fn get_vec(msg: &MsgPackEntry) -> color_eyre::Result<Vec<MsgPackEntry>> {
    if let MsgPackValue::FixArray(a) = &msg.data {
        Ok(a.to_vec())
    } else {
        Err(eyre!("Not a FixArray"))
    }
}
fn get_u32(msg: &MsgPackEntry) -> color_eyre::Result<u32> {
    if let MsgPackValue::U32(n) = msg.data {
        Ok(n)
    } else {
        Err(eyre!("Not a U64"))
    }
}
fn get_bool(msg: &MsgPackEntry) -> color_eyre::Result<bool> {
    if let MsgPackValue::Bool(n) = msg.data {
        Ok(n)
    } else {
        Err(eyre!("Not a Bool"))
    }
}
fn get_bin32(msg: &MsgPackEntry) -> color_eyre::Result<Vec<u8>> {
    if let MsgPackValue::Bin32(b) = &msg.data {
        Ok(b.to_vec())
    } else {
        Err(eyre!("Not a Bin32"))
    }
}
