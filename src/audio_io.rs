use hound::{SampleFormat, WavSpec, WavWriter};
use rubato::{audioadapter_buffers::direct::InterleavedSlice, Fft, Resampler};
use std::{fs::File, path::Path};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::{probe::Hint, FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia_adapter_libopus::OpusDecoder;

#[derive(Debug)]
pub enum OutputFormat {
    Wav,
    Flac,
    Mp3,
}

impl OutputFormat {
    pub fn from_path(path: &Path) -> Result<Self, String> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or("No file extension")?
            .to_lowercase();

        match ext.as_str() {
            "wav" => Ok(OutputFormat::Wav),
            "flac" => Ok(OutputFormat::Flac),
            "mp3" => Ok(OutputFormat::Mp3),
            _ => Err(format!("Unsupported output format: {}", ext)),
        }
    }
}

fn tpdf_dither(value: f32, bits: u32) -> f32 {
    let lsb = 2.0_f32.powi(-(bits as i32 - 1));
    let noise = (fastrand::f32() - fastrand::f32()) * lsb;
    value + noise
}

fn ensure_stereo(samples: &[f32], channels: usize) -> Vec<(f32, f32)> {
    match channels {
        1 => samples.iter().map(|&s| (s, s)).collect(),
        2 => samples.chunks(2).map(|c| (c[0], c[1])).collect(),
        _ => samples.chunks(channels).map(|c| (c[0], c[1])).collect(),
    }
}

pub fn load_audio(path: &Path) -> Result<Vec<(f32, f32)>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();

    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, format_opts, metadata_opts)
        .map_err(|e| format!("Failed to get audio format: {}", e))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or("No valid audio track found")?;

    let track_id = track.id;
    let codec_params = track.codec_params.as_ref().ok_or("No codec parameters")?;
    let audio_params = codec_params.audio().ok_or("Not an audio track")?;
    let sample_rate = audio_params.sample_rate.ok_or("Sample rate not found")?;
    let channels = audio_params
        .channels
        .as_ref()
        .ok_or("Channel info not found")?
        .count();

    let decoder_opts = AudioDecoderOptions::default();
    let mut codec_registry = symphonia::core::codecs::registry::CodecRegistry::new();
    symphonia::default::register_enabled_codecs(&mut codec_registry);
    codec_registry.register_audio_decoder::<OpusDecoder>();
    let mut decoder = codec_registry
        .make_audio_decoder(
            track.codec_params.as_ref().unwrap().audio().unwrap(),
            &decoder_opts,
        )
        .map_err(|e| format!("Failed to create decoder: {}", e))?;

    let mut all_samples = Vec::new();

    while let Ok(Some(packet)) = format.next_packet() {
        if packet.track_id != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(audio_buf) => {
                let nbframes = audio_buf.frames();
                let num_channels = audio_buf.spec().channels().count();
                let total_samples = nbframes * num_channels;
                let mut interleaved = vec![0.0f32; total_samples];
                audio_buf.copy_to_slice_interleaved(&mut interleaved);
                all_samples.extend(interleaved);
            }
            Err(_) => continue,
        }
    }

    let stereo_samples = ensure_stereo(&all_samples, channels);

    if sample_rate != 44100 {
        let interleaved: Vec<f32> = stereo_samples.iter().flat_map(|&(l, r)| [l, r]).collect();
        let resampled = resample_interleaved(interleaved, sample_rate, 44100, 2)?;
        Ok(resampled.chunks_exact(2).map(|c| (c[0], c[1])).collect())
    } else {
        Ok(stereo_samples)
    }
}

fn resample_interleaved(
    samples: Vec<f32>,
    from_rate: u32,
    to_rate: u32,
    channels: usize,
) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    if from_rate == to_rate {
        return Ok(samples);
    }

    let frames = samples.len() / channels;
    let mut resampler = Fft::<f32>::new(
        from_rate as usize,
        to_rate as usize,
        1024,
        channels,
        rubato::FixedSync::Input,
    )?;

    let input = InterleavedSlice::new(&samples, channels, frames)
        .map_err(|e| format!("Failed to create input buffer: {:?}", e))?;
    let output = resampler
        .process_all(&input, frames, None)
        .map_err(|e| format!("Resampling failed: {:?}", e))?;

    Ok(output.take_data())
}

pub fn save_audio(
    path: &Path,
    samples: &[(f32, f32)],
    sample_rate: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let format = OutputFormat::from_path(path)?;

    match format {
        OutputFormat::Wav => save_wav(path, samples, sample_rate),
        OutputFormat::Flac => save_flac(path, samples, sample_rate),
        OutputFormat::Mp3 => save_mp3(path, samples, sample_rate),
    }
}

fn save_wav(
    path: &Path,
    samples: &[(f32, f32)],
    sample_rate: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let spec = WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)?;
    for (left, right) in samples {
        let left_sample = (tpdf_dither(left.clamp(-1.0, 1.0), 16) * 32767.0) as i16;
        let right_sample = (tpdf_dither(right.clamp(-1.0, 1.0), 16) * 32767.0) as i16;
        writer.write_sample(left_sample)?;
        writer.write_sample(right_sample)?;
    }
    writer.finalize()?;
    Ok(())
}

fn save_flac(
    path: &Path,
    samples: &[(f32, f32)],
    sample_rate: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;

    let mut i32_samples: Vec<i32> = Vec::with_capacity(samples.len() * 2);
    for (left, right) in samples {
        i32_samples.push((tpdf_dither(left.clamp(-1.0, 1.0), 16) * 32767.0) as i32);
        i32_samples.push((tpdf_dither(right.clamp(-1.0, 1.0), 16) * 32767.0) as i32);
    }

    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| format!("FLAC config error: {:?}", e))?;

    let source =
        flacenc::source::MemSource::from_samples(&i32_samples, 2, 16, sample_rate as usize);

    let flac_stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)?;

    let mut sink = flacenc::bitsink::ByteSink::new();
    flac_stream.write(&mut sink)?;

    std::fs::write(path, sink.as_slice())?;
    Ok(())
}

fn save_mp3(
    path: &Path,
    samples: &[(f32, f32)],
    sample_rate: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, Quality};

    let mut builder = Builder::new().unwrap();
    builder.set_num_channels(2).unwrap();
    builder.set_sample_rate(sample_rate).unwrap();
    builder.set_brate(Bitrate::Kbps320).unwrap();
    builder.set_quality(Quality::Best).unwrap();

    let mut encoder = builder.build().unwrap();

    let mut left_channel: Vec<i16> = Vec::with_capacity(samples.len());
    let mut right_channel: Vec<i16> = Vec::with_capacity(samples.len());
    for (left, right) in samples {
        left_channel.push((tpdf_dither(left.clamp(-1.0, 1.0), 16) * 32767.0) as i16);
        right_channel.push((tpdf_dither(right.clamp(-1.0, 1.0), 16) * 32767.0) as i16);
    }

    let mut mp3_buffer = Vec::new();

    const CHUNK_SIZE: usize = 32768;
    for i in (0..left_channel.len()).step_by(CHUNK_SIZE) {
        let end = (i + CHUNK_SIZE).min(left_channel.len());
        let input = DualPcm {
            left: &left_channel[i..end],
            right: &right_channel[i..end],
        };

        mp3_buffer.reserve(mp3lame_encoder::max_required_buffer_size(input.left.len()));
        let encoded_size = encoder
            .encode(input, mp3_buffer.spare_capacity_mut())
            .unwrap();

        unsafe {
            mp3_buffer.set_len(mp3_buffer.len() + encoded_size);
        }
    }

    mp3_buffer.reserve(7200);
    let flushed_size = encoder
        .flush::<FlushNoGap>(mp3_buffer.spare_capacity_mut())
        .unwrap();

    unsafe {
        mp3_buffer.set_len(mp3_buffer.len() + flushed_size);
    }

    std::fs::write(path, &mp3_buffer)?;
    Ok(())
}
