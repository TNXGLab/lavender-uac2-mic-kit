use libc::{c_char, c_int, c_uint, c_ulong, c_void};
use nnnoiseless::DenoiseState;
use std::env;
use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const SAMPLE_RATE: i32 = 48_000;
const CHANNELS: i32 = 1;
const FRAME_SIZE: usize = DenoiseState::FRAME_SIZE;
const TIMEOUT_NANOS: i64 = 100_000_000;

const AAUDIO_OK: i32 = 0;
const AAUDIO_DIRECTION_INPUT: i32 = 1;
const AAUDIO_FORMAT_PCM_I16: i32 = 1;
const AAUDIO_SHARING_MODE_SHARED: i32 = 1;
const AAUDIO_PERFORMANCE_MODE_LOW_LATENCY: i32 = 12;
const AAUDIO_INPUT_PRESET_CAMCORDER: i32 = 5;

const PCM_OUT: c_uint = 0;
const PCM_FORMAT_S16_LE: c_int = 0;

static KEEP_RUNNING: AtomicBool = AtomicBool::new(true);

#[repr(C)]
struct AAudioStreamBuilder {
    _private: [u8; 0],
}

#[repr(C)]
struct AAudioStream {
    _private: [u8; 0],
}

#[repr(C)]
struct Pcm {
    _private: [u8; 0],
}

#[repr(C)]
struct PcmConfig {
    channels: c_uint,
    rate: c_uint,
    period_size: c_uint,
    period_count: c_uint,
    format: c_int,
    start_threshold: c_ulong,
    stop_threshold: c_ulong,
    silence_threshold: c_ulong,
    silence_size: c_ulong,
    avail_min: c_ulong,
}

#[link(name = "aaudio")]
extern "C" {
    fn AAudio_convertResultToText(return_code: i32) -> *const c_char;
    fn AAudio_createStreamBuilder(builder: *mut *mut AAudioStreamBuilder) -> i32;
    fn AAudioStreamBuilder_delete(builder: *mut AAudioStreamBuilder);
    fn AAudioStreamBuilder_setDirection(builder: *mut AAudioStreamBuilder, direction: i32);
    fn AAudioStreamBuilder_setPerformanceMode(builder: *mut AAudioStreamBuilder, mode: i32);
    fn AAudioStreamBuilder_setSharingMode(builder: *mut AAudioStreamBuilder, sharing_mode: i32);
    fn AAudioStreamBuilder_setSampleRate(builder: *mut AAudioStreamBuilder, sample_rate: i32);
    fn AAudioStreamBuilder_setChannelCount(builder: *mut AAudioStreamBuilder, channel_count: i32);
    fn AAudioStreamBuilder_setFormat(builder: *mut AAudioStreamBuilder, format: i32);
    fn AAudioStreamBuilder_setInputPreset(builder: *mut AAudioStreamBuilder, input_preset: i32);
    fn AAudioStreamBuilder_openStream(
        builder: *mut AAudioStreamBuilder,
        stream: *mut *mut AAudioStream,
    ) -> i32;
    fn AAudioStream_requestStart(stream: *mut AAudioStream) -> i32;
    fn AAudioStream_requestStop(stream: *mut AAudioStream) -> i32;
    fn AAudioStream_close(stream: *mut AAudioStream) -> i32;
    fn AAudioStream_read(
        stream: *mut AAudioStream,
        buffer: *mut c_void,
        num_frames: i32,
        timeout_nanoseconds: i64,
    ) -> i32;
}

type PcmOpenFn = unsafe extern "C" fn(c_uint, c_uint, c_uint, *const PcmConfig) -> *mut Pcm;
type PcmCloseFn = unsafe extern "C" fn(*mut Pcm) -> c_int;
type PcmIsReadyFn = unsafe extern "C" fn(*const Pcm) -> c_int;
type PcmGetErrorFn = unsafe extern "C" fn(*const Pcm) -> *const c_char;
type PcmWriteFn = unsafe extern "C" fn(*mut Pcm, *const c_void, c_uint) -> c_int;

extern "C" fn handle_signal(_: c_int) {
    KEEP_RUNNING.store(false, Ordering::SeqCst);
}

struct AAudioInput {
    stream: *mut AAudioStream,
}

impl Drop for AAudioInput {
    fn drop(&mut self) {
        unsafe {
            if !self.stream.is_null() {
                let _ = AAudioStream_requestStop(self.stream);
                let _ = AAudioStream_close(self.stream);
            }
        }
    }
}

struct Uac2Playback {
    pcm: *mut Pcm,
    pcm_close: PcmCloseFn,
    pcm_get_error: PcmGetErrorFn,
}

impl Drop for Uac2Playback {
    fn drop(&mut self) {
        unsafe {
            if !self.pcm.is_null() {
                let _ = (self.pcm_close)(self.pcm);
            }
        }
    }
}

struct TinyAlsa {
    handle: *mut c_void,
    pcm_open: PcmOpenFn,
    pcm_close: PcmCloseFn,
    pcm_is_ready: PcmIsReadyFn,
    pcm_get_error: PcmGetErrorFn,
    pcm_write: PcmWriteFn,
}

impl Drop for TinyAlsa {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                let _ = libc::dlclose(self.handle);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct RuntimeConfig {
    gain: f32,
    vad_threshold: f32,
    floor_attenuation: f32,
    raw_blend: f32,
    active_rms_threshold: f32,
}

fn main() {
    unsafe {
        libc::signal(libc::SIGINT, handle_signal as *const () as usize);
        libc::signal(libc::SIGTERM, handle_signal as *const () as usize);
    }

    let config = RuntimeConfig::from_args();
    if let Err(error) = run(config) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

impl RuntimeConfig {
    fn from_args() -> Self {
        let mut args = env::args().skip(1);
        let gain = parse_f32_arg(args.next(), 8.0, 1.0, 24.0);
        let vad_threshold = parse_f32_arg(args.next(), 0.35, 0.0, 1.0);
        let floor_attenuation = parse_f32_arg(args.next(), 0.12, 0.0, 1.0);
        let raw_blend = parse_f32_arg(args.next(), 0.25, 0.0, 1.0);
        let active_rms_threshold = parse_f32_arg(args.next(), 180.0, 0.0, 4000.0);

        Self {
            gain,
            vad_threshold,
            floor_attenuation,
            raw_blend,
            active_rms_threshold,
        }
    }
}

fn run(config: RuntimeConfig) -> Result<(), String> {
    let tinyalsa = TinyAlsa::load()?;
    let input = open_aaudio_input()?;
    let mut output = wait_for_uac2_playback(&tinyalsa)?;
    let mut denoise = DenoiseState::new();

    let mut input_samples = [0_i16; FRAME_SIZE];
    let mut denoise_input = [0.0_f32; FRAME_SIZE];
    let mut denoise_output = [0.0_f32; FRAME_SIZE];
    let mut output_samples = [0_i16; FRAME_SIZE];
    let mut high_pass_state = 0.0_f32;
    let mut skip_first_frame = true;

    eprintln!(
        "bridging AAudio mic -> UAC2 playback at 48kHz mono s16 with nnnoiseless RNNoise, gain={:.1}x, vad_threshold={:.2}, floor_attenuation={:.2}, raw_blend={:.2}, active_rms_threshold={:.1}",
        config.gain,
        config.vad_threshold,
        config.floor_attenuation,
        config.raw_blend,
        config.active_rms_threshold
    );

    while KEEP_RUNNING.load(Ordering::SeqCst) {
        let frames_read = unsafe {
            AAudioStream_read(
                input.stream,
                input_samples.as_mut_ptr().cast::<c_void>(),
                FRAME_SIZE as i32,
                TIMEOUT_NANOS,
            )
        };

        if frames_read < 0 {
            return Err(format!(
                "AAudioStream_read failed: {}",
                aaudio_result_text(frames_read)
            ));
        }

        if frames_read == 0 {
            continue;
        }

        let frames_read = frames_read as usize;
        let mut energy_sum = 0.0_f32;
        for frame in 0..frames_read {
            let input_sample = input_samples[frame] as f32;
            high_pass_state += (input_sample - high_pass_state) / 256.0;
            let high_passed_sample = input_sample - high_pass_state;
            denoise_input[frame] = high_passed_sample;
            energy_sum += high_passed_sample * high_passed_sample;
        }
        denoise_input[frames_read..].fill(0.0);

        let raw_rms = (energy_sum / frames_read as f32).sqrt();
        let voice_probability = denoise.process_frame(&mut denoise_output, &denoise_input);
        let has_active_signal = raw_rms >= config.active_rms_threshold;

        // RNNoise is optimized for speech. The raw blend keeps music and other real sounds from
        // being mistaken for removable noise, while the RMS gate still suppresses quiet-room hiss.
        let attenuation = if !has_active_signal && voice_probability < config.vad_threshold {
            smoothstep_attenuation(
                voice_probability,
                config.vad_threshold,
                config.floor_attenuation,
            )
        } else {
            1.0
        };

        for frame in 0..frames_read {
            let sample = if skip_first_frame {
                0.0
            } else {
                let preserved_raw = denoise_input[frame] * config.raw_blend;
                let cleaned = denoise_output[frame] * (1.0 - config.raw_blend);
                (cleaned + preserved_raw) * config.gain * attenuation
            };
            output_samples[frame] = soft_limited_i16(sample);
        }
        skip_first_frame = false;

        let write_result = unsafe {
            (tinyalsa.pcm_write)(
                output.pcm,
                output_samples.as_ptr().cast::<c_void>(),
                (frames_read * std::mem::size_of::<i16>()) as c_uint,
            )
        };
        if write_result < 0 {
            eprintln!(
                "pcm_write failed: {}, reopening UAC2 playback",
                output.error()
            );
            drop(output);
            output = wait_for_uac2_playback(&tinyalsa)?;
            skip_first_frame = true;
        }
    }

    Ok(())
}

impl TinyAlsa {
    fn load() -> Result<Self, String> {
        let handle = open_tinyalsa_handle()?;

        unsafe {
            Ok(Self {
                handle,
                pcm_open: load_symbol(handle, "pcm_open")?,
                pcm_close: load_symbol(handle, "pcm_close")?,
                pcm_is_ready: load_symbol(handle, "pcm_is_ready")?,
                pcm_get_error: load_symbol(handle, "pcm_get_error")?,
                pcm_write: load_symbol(handle, "pcm_write")?,
            })
        }
    }
}

impl Uac2Playback {
    fn error(&self) -> String {
        unsafe {
            let text = (self.pcm_get_error)(self.pcm);
            if text.is_null() {
                "unknown tinyalsa error".to_string()
            } else {
                CStr::from_ptr(text).to_string_lossy().into_owned()
            }
        }
    }
}

fn open_aaudio_input() -> Result<AAudioInput, String> {
    unsafe {
        let mut builder: *mut AAudioStreamBuilder = ptr::null_mut();
        let result = AAudio_createStreamBuilder(&mut builder);
        if result != AAUDIO_OK {
            return Err(format!(
                "AAudio_createStreamBuilder failed: {}",
                aaudio_result_text(result)
            ));
        }

        AAudioStreamBuilder_setDirection(builder, AAUDIO_DIRECTION_INPUT);
        AAudioStreamBuilder_setPerformanceMode(builder, AAUDIO_PERFORMANCE_MODE_LOW_LATENCY);
        AAudioStreamBuilder_setSharingMode(builder, AAUDIO_SHARING_MODE_SHARED);
        AAudioStreamBuilder_setSampleRate(builder, SAMPLE_RATE);
        AAudioStreamBuilder_setChannelCount(builder, CHANNELS);
        AAudioStreamBuilder_setFormat(builder, AAUDIO_FORMAT_PCM_I16);
        AAudioStreamBuilder_setInputPreset(builder, AAUDIO_INPUT_PRESET_CAMCORDER);

        let mut stream: *mut AAudioStream = ptr::null_mut();
        let open_result = AAudioStreamBuilder_openStream(builder, &mut stream);
        AAudioStreamBuilder_delete(builder);
        if open_result != AAUDIO_OK {
            return Err(format!(
                "AAudioStreamBuilder_openStream failed: {}",
                aaudio_result_text(open_result)
            ));
        }

        let start_result = AAudioStream_requestStart(stream);
        if start_result != AAUDIO_OK {
            let _ = AAudioStream_close(stream);
            return Err(format!(
                "AAudioStream_requestStart failed: {}",
                aaudio_result_text(start_result)
            ));
        }

        Ok(AAudioInput { stream })
    }
}

fn open_uac2_playback(tinyalsa: &TinyAlsa) -> Result<Uac2Playback, String> {
    let config = PcmConfig {
        channels: CHANNELS as c_uint,
        rate: SAMPLE_RATE as c_uint,
        period_size: FRAME_SIZE as c_uint,
        period_count: 4,
        format: PCM_FORMAT_S16_LE,
        start_threshold: 0,
        stop_threshold: 0,
        silence_threshold: 0,
        silence_size: 0,
        avail_min: 0,
    };

    unsafe {
        let pcm = (tinyalsa.pcm_open)(1, 0, PCM_OUT, &config);
        if pcm.is_null() {
            return Err("failed to open UAC2 playback: pcm_open returned null".to_string());
        }

        let playback = Uac2Playback {
            pcm,
            pcm_close: tinyalsa.pcm_close,
            pcm_get_error: tinyalsa.pcm_get_error,
        };

        if (tinyalsa.pcm_is_ready)(pcm) == 0 {
            let error = playback.error();
            return Err(format!("failed to open UAC2 playback: {error}"));
        }

        Ok(playback)
    }
}

fn wait_for_uac2_playback(tinyalsa: &TinyAlsa) -> Result<Uac2Playback, String> {
    let mut attempts = 0_u32;

    while KEEP_RUNNING.load(Ordering::SeqCst) {
        match open_uac2_playback(tinyalsa) {
            Ok(playback) => {
                if attempts > 0 {
                    eprintln!("UAC2 playback reopened after {attempts} attempts");
                }
                return Ok(playback);
            }
            Err(error) => {
                attempts = attempts.saturating_add(1);
                eprintln!("UAC2 playback is not ready: {error}; retrying");
                thread::sleep(Duration::from_secs(1));
            }
        }
    }

    Err("stopped while waiting for UAC2 playback".to_string())
}

fn open_tinyalsa_handle() -> Result<*mut c_void, String> {
    for library_path in [
        "/system/lib64/libtinyalsa.so",
        "/vendor/lib64/libtinyalsa.so",
        "libtinyalsa.so",
    ] {
        let path = CString::new(library_path).unwrap();
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW) };
        if !handle.is_null() {
            return Ok(handle);
        }
    }

    Err(format!(
        "failed to load libtinyalsa.so: {}",
        dynamic_loader_error()
    ))
}

unsafe fn load_symbol<T>(handle: *mut c_void, symbol_name: &str) -> Result<T, String> {
    let symbol = CString::new(symbol_name).unwrap();
    let pointer = libc::dlsym(handle, symbol.as_ptr());
    if pointer.is_null() {
        Err(format!(
            "failed to load symbol {symbol_name}: {}",
            dynamic_loader_error()
        ))
    } else {
        Ok(std::mem::transmute_copy(&pointer))
    }
}

fn dynamic_loader_error() -> String {
    unsafe {
        let error = libc::dlerror();
        if error.is_null() {
            "unknown dlopen/dlsym error".to_string()
        } else {
            CStr::from_ptr(error).to_string_lossy().into_owned()
        }
    }
}

fn parse_f32_arg(value: Option<String>, default_value: f32, min_value: f32, max_value: f32) -> f32 {
    value
        .and_then(|raw| raw.parse::<f32>().ok())
        .map(|parsed| parsed.clamp(min_value, max_value))
        .unwrap_or(default_value)
}

fn smoothstep_attenuation(voice_probability: f32, threshold: f32, floor_attenuation: f32) -> f32 {
    if threshold <= 0.0 {
        return 1.0;
    }

    let ratio = (voice_probability / threshold).clamp(0.0, 1.0);
    let smooth = ratio * ratio * (3.0 - 2.0 * ratio);
    floor_attenuation + (1.0 - floor_attenuation) * smooth
}

fn soft_limited_i16(sample: f32) -> i16 {
    let normalized = (sample / 32768.0).clamp(-4.0, 4.0);
    let limited = normalized.tanh() * 32767.0;
    limited.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

fn aaudio_result_text(result: i32) -> String {
    unsafe {
        let text = AAudio_convertResultToText(result);
        if text.is_null() {
            format!("unknown AAudio result {result}")
        } else {
            CStr::from_ptr(text).to_string_lossy().into_owned()
        }
    }
}
