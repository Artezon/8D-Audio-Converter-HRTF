use hound::{SampleFormat, WavSpec, WavWriter};
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

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

/// Convert multi-channel audio to mono by averaging all channels
fn convert_to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }

    samples
        .chunks(channels)
        .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
        .collect()
}

pub fn load_audio(path: &Path) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();
    let decoder_opts = DecoderOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| format!("Failed to get audio format: {}", e))?;

    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or("No valid audio track found")?;

    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or("Sample rate not found")?;
    let channels = track
        .codec_params
        .channels
        .ok_or("Channel info not found")?
        .count();

    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &decoder_opts)?;

    let mut all_samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(decoded) => {
                let spec = *decoded.spec();
                let duration = decoded.capacity() as u64;
                let mut sample_buf = SampleBuffer::<f32>::new(duration, spec);
                sample_buf.copy_interleaved_ref(decoded);

                all_samples.extend_from_slice(sample_buf.samples());
            }
            Err(_) => continue,
        }
    }

    let mut mono_samples = convert_to_mono(&all_samples, channels);

    if sample_rate != 44100 {
        mono_samples = resample(mono_samples, sample_rate, 44100)?;
    }

    Ok(mono_samples)
}

pub fn resample(
    samples: Vec<f32>,
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    if from_rate == to_rate {
        return Ok(samples);
    }

    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        to_rate as f64 / from_rate as f64,
        2.0,
        params,
        samples.len(),
        1,
    )?;

    let waves_in = vec![samples];
    let waves_out = resampler.process(&waves_in, None)?;

    Ok(waves_out[0].clone())
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
        let left_sample = (left.clamp(-1.0, 1.0) * 32767.0) as i16;
        let right_sample = (right.clamp(-1.0, 1.0) * 32767.0) as i16;

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

    // Convert to interleaved i32 samples
    let mut i32_samples: Vec<i32> = Vec::with_capacity(samples.len() * 2);
    for (left, right) in samples {
        i32_samples.push((left.clamp(-1.0, 1.0) * 32767.0) as i32);
        i32_samples.push((right.clamp(-1.0, 1.0) * 32767.0) as i32);
    }

    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| format!("FLAC config error: {:?}", e))?;

    let source = flacenc::source::MemSource::from_samples(
        &i32_samples,
        2,  // channels
        16, // bits_per_sample
        sample_rate as usize,
    );

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
    use mp3lame_encoder::{Builder, DualPcm, FlushNoGap};

    let mut builder = Builder::new().expect("Create LAME builder");
    builder.set_num_channels(2).expect("set channels");
    builder
        .set_sample_rate(sample_rate)
        .expect("set sample rate");
    builder
        .set_brate(mp3lame_encoder::Bitrate::Kbps320)
        .expect("set bitrate");
    builder
        .set_quality(mp3lame_encoder::Quality::Best)
        .expect("set quality");

    let mut encoder = builder.build().expect("create encoder");

    // Convert to separate i16 channels
    let mut left_channel: Vec<i16> = Vec::with_capacity(samples.len());
    let mut right_channel: Vec<i16> = Vec::with_capacity(samples.len());

    for (left, right) in samples {
        left_channel.push((left.clamp(-1.0, 1.0) * 32767.0) as i16);
        right_channel.push((right.clamp(-1.0, 1.0) * 32767.0) as i16);
    }

    let mut mp3_buffer = Vec::new();

    // Encode in chunks using DualPcm
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
            .expect("encode chunk");

        unsafe {
            mp3_buffer.set_len(mp3_buffer.len() + encoded_size);
        }
    }

    // Flush remaining data
    mp3_buffer.reserve(7200); // LAME requires at least 7200 bytes for flush
    let flushed_size = encoder
        .flush::<FlushNoGap>(mp3_buffer.spare_capacity_mut())
        .expect("flush encoder");

    unsafe {
        mp3_buffer.set_len(mp3_buffer.len() + flushed_size);
    }

    std::fs::write(path, &mp3_buffer)?;
    Ok(())
}
