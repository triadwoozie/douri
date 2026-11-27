//! Audio file decoding for the Marbble Sampler pool using `symphonia`.
//!
//! Ported from Marbble (<https://github.com/JackMechem/marbble>),
//! Copyright (c) 2026 Jack Mechem, MIT License.

pub mod decode {
    use std::fs::File;
    use std::path::Path;
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::errors::Error;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    pub struct Decoded {
        pub sample_rate: u32,
        pub channels: Vec<Vec<f32>>,
    }

    pub fn decode(path: &Path) -> Result<Decoded, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .map_err(|e| e.to_string())?;
        let mut format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| "no audio track".to_string())?;
        let track_id = track.id;
        let sample_rate = track.codec_params.sample_rate.unwrap_or(0);
        let n_ch = track
            .codec_params
            .channels
            .map(|c| c.count())
            .unwrap_or(1)
            .max(1);
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| e.to_string())?;

        let mut channels = vec![Vec::new(); n_ch];
        let mut sbuf: Option<SampleBuffer<f32>> = None;

        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(Error::ResetRequired) => break,
                Err(e) => return Err(e.to_string()),
            };
            if packet.track_id() != track_id {
                continue;
            }
            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                Err(Error::DecodeError(_)) => continue,
                Err(e) => return Err(e.to_string()),
            };
            let spec = *decoded.spec();
            let buf =
                sbuf.get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
            if buf.capacity() < decoded.frames() * spec.channels.count() {
                *buf = SampleBuffer::new(decoded.capacity() as u64, spec);
            }
            buf.copy_interleaved_ref(decoded);
            let ch_count = spec.channels.count().min(channels.len());
            for frame in buf.samples().chunks_exact(spec.channels.count()) {
                for (c, ch) in channels.iter_mut().enumerate().take(ch_count) {
                    ch.push(frame[c]);
                }
            }
        }
        Ok(Decoded {
            sample_rate,
            channels,
        })
    }

    #[cfg(test)]
    pub fn write_wav(path: &Path, sample_rate: u32, channels: &[Vec<f32>]) -> std::io::Result<()> {
        use std::io::Write;
        let n_ch = channels.len().max(1) as u16;
        let n_frames = channels.first().map_or(0, |c| c.len());
        let data_bytes = (n_frames * n_ch as usize * 4) as u32;
        let mut f = File::create(path)?;
        f.write_all(b"RIFF")?;
        f.write_all(&(36 + data_bytes).to_le_bytes())?;
        f.write_all(b"WAVEfmt ")?;
        f.write_all(&16u32.to_le_bytes())?;
        f.write_all(&3u16.to_le_bytes())?; // IEEE float
        f.write_all(&n_ch.to_le_bytes())?;
        f.write_all(&sample_rate.to_le_bytes())?;
        f.write_all(&(sample_rate * n_ch as u32 * 4).to_le_bytes())?;
        f.write_all(&(n_ch * 4).to_le_bytes())?;
        f.write_all(&32u16.to_le_bytes())?;
        f.write_all(b"data")?;
        f.write_all(&data_bytes.to_le_bytes())?;
        for i in 0..n_frames {
            for ch in channels {
                let s = ch.get(i).copied().unwrap_or(0.0);
                f.write_all(&s.to_le_bytes())?;
            }
        }
        Ok(())
    }
}
