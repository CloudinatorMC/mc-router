use anyhow::{anyhow, Context, Result};
use tokio::net::TcpStream;
use tokio::io::AsyncReadExt;

// Upper bound for handshake packet payload (excluding length varint). Vanilla is tiny (<300 bytes),
// but modded (Forge/Fabric with extra data) can be larger. 32 KiB should be ample while protecting memory.
pub const MAX_HANDSHAKE_PACKET: usize = 32 * 1024;

pub async fn read_framed_packet(stream: &mut TcpStream) -> Result<Vec<u8>> {
    // Read length VarInt (max 5 bytes) first.
    let mut len_bytes = Vec::with_capacity(5);
    loop {
        let mut b = [0u8;1];
        let n = stream.read(&mut b).await?;
        if n == 0 { return Err(anyhow!("connection closed before length")); }
        len_bytes.push(b[0]);
        if (b[0] & 0x80) == 0 { break; }
        if len_bytes.len() == 5 { return Err(anyhow!("length varint too long")); }
    }
    let (payload_len, _) = read_varint(&len_bytes, 0)?; // reuse logic
    if payload_len < 0 { return Err(anyhow!("negative length")); }
    let payload_len = payload_len as usize;
    if payload_len > MAX_HANDSHAKE_PACKET { return Err(anyhow!("handshake too large ({} > {})", payload_len, MAX_HANDSHAKE_PACKET)); }
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).await?;
    // Combine length + payload for downstream parser which expects both.
    let mut full = len_bytes;
    full.extend_from_slice(&payload);
    Ok(full)
}

// Parse VarInt length-prefixed packet: after length the first VarInt is packet id which for handshake is 0x00 in state=handshaking.
// The structure of handshake (modern):
// packet id (VarInt) = 0
// protocol version (VarInt)
// server address (String with VarInt length) -- may include domain + optional null + FML marker etc
// server port (Unsigned Short)
// next state (VarInt)
pub fn parse_handshake_server_address(data: &[u8]) -> Result<String> {
    let (packet_length, mut idx) = read_varint(data, 0)?; // total payload length
    if packet_length as usize + idx != data.len() { return Err(anyhow!("length mismatch")); }
    let (packet_id, ni) = read_varint(data, idx)?; idx = ni;
    if packet_id != 0 { return Err(anyhow!("not a handshake packet id={packet_id}")); }
    let (_proto, ni2) = read_varint(data, idx)?; idx = ni2;
    let (addr, _after_addr) = read_mc_string(data, idx)?;
    // We don't need further fields for routing.
    Ok(addr)
}

pub fn read_varint(bytes: &[u8], mut idx: usize) -> Result<(i32, usize)> {
    let mut num = 0i32;
    let mut shift = 0u32;
    loop {
        if idx >= bytes.len() { return Err(anyhow!("unexpected eof reading varint")); }
        let b = bytes[idx]; idx += 1;
        num |= ((b & 0x7F) as i32) << shift;
        if (b & 0x80) == 0 { break; }
        shift += 7;
        if shift >= 35 { return Err(anyhow!("varint too large")); }
    }
    Ok((num, idx))
}

pub fn read_mc_string(bytes: &[u8], idx: usize) -> Result<(String, usize)> {
    let (len, mut i) = read_varint(bytes, idx)?;
    if len < 0 { return Err(anyhow!("negative string length")); }
    let len = len as usize;
    if i + len > bytes.len() { return Err(anyhow!("string length out of bounds")); }
    let s = std::str::from_utf8(&bytes[i..i + len]).context("utf8")?.to_string();
    i += len;
    Ok((s, i))
}
