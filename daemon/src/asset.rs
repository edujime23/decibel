use std::io::Cursor;
use std::sync::Arc;
use lewton::inside_ogg::OggStreamReader;

#[derive(Clone)]
pub struct PCMAsset {
    pub pcm: Arc<Vec<f32>>,
    pub channels: u16,
    pub sample_rate: u32,
}

pub fn decode_ogg_in_memory(raw_ogg_bytes: Vec<u8>) -> Result<PCMAsset, String> {
    let cursor = Cursor::new(raw_ogg_bytes);
    let mut srr = OggStreamReader::new(cursor)
        .map_err(|e| format!("OggStreamReader error: {:?}", e))?;

    let mut pcm_data = Vec::new();
    while let Ok(Some(packet)) = srr.read_dec_packet_itl() {
        for sample in packet {
            pcm_data.push(sample as f32 / 32768.0);
        }
    }

    Ok(PCMAsset {
        pcm: Arc::new(pcm_data),
        channels: srr.ident_hdr.audio_channels as u16,
        sample_rate: srr.ident_hdr.audio_sample_rate,
    })
}