//! Reads the sound itself, for the waveforms. It lives only in this buffer, which
//! holds a fraction of a second and is replaced as it goes. Nothing is recorded.

use std::{
    collections::VecDeque,
    sync::mpsc::{self, Sender},
    time::Duration,
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Media::Audio::{
            AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            AUDCLNT_STREAMFLAGS_LOOPBACK, AUDIOCLIENT_ACTIVATION_PARAMS,
            AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, ActivateAudioInterfaceAsync,
            IActivateAudioInterfaceAsyncOperation, IActivateAudioInterfaceCompletionHandler,
            IActivateAudioInterfaceCompletionHandler_Impl, IAudioCaptureClient, IAudioClient,
            IMMDevice, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX,
        },
        System::{
            Com::{CLSCTX_ALL, CoTaskMemFree, StructuredStorage::PROPVARIANT},
            Threading::CreateEventW,
            Variant::VT_BLOB,
        },
    },
    core::{Error, HRESULT, IUnknown, Interface, Ref, Result, implement},
};

/// How much recent sound is measured, 50 milliseconds at 48 kHz.
const WINDOW: usize = 2400;

/// Averaging this many samples keeps sound below about 100 Hz: the bass.
const BASS_SPAN: usize = 240;

/// Averaging this many samples keeps sound below about 1 kHz. What it removes from
/// the whole is the treble, and what it adds to the bass is the mids.
const MIDS_SPAN: usize = 24;

/// How far each band is lifted to fill its range. Music has far less treble than bass.
const BASS_GAIN: f32 = 3.5;
const MIDS_GAIN: f32 = 5.0;
const TREBLE_GAIN: f32 = 12.0;

/// The buffer Windows keeps, in 100 nanosecond units: 200 milliseconds.
const BUFFER: i64 = 2_000_000;

const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(2);

const SAMPLE_RATE: u32 = 48_000;
const WAVE_FORMAT_PCM: u16 = 1;

/// A PROPVARIANT holding a blob, which the Windows bindings have no builder for.
#[repr(C)]
struct BlobVariant {
    kind: u16,
    reserved: [u16; 3],
    size: u32,
    data: *const AUDIOCLIENT_ACTIVATION_PARAMS,
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Activated(Sender<()>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(&self, _: Ref<IActivateAudioInterfaceAsyncOperation>) -> Result<()> {
        let _ = self.0.send(());
        Ok(())
    }
}

pub struct Capture {
    client: IAudioClient,
    reader: IAudioCaptureClient,
    channels: usize,
    float: bool,
    recent: VecDeque<f32>,
    ready: Option<HANDLE>,
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: the client and the event belong to this value alone.
        unsafe {
            let _ = self.client.Stop();
            if let Some(ready) = self.ready {
                let _ = CloseHandle(ready);
            }
        }
    }
}

impl Capture {
    /// Everything playing on a device.
    pub fn of_device(device: &IMMDevice) -> Result<Self> {
        // SAFETY: the format is only read while Windows' copy of it is alive, and
        // Initialize copies what it needs before the copy is freed.
        unsafe {
            let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let format = client.GetMixFormat()?;
            let channels = usize::from((*format).nChannels);
            let initialized = client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                BUFFER,
                0,
                format,
                None,
            );
            CoTaskMemFree(Some(format.cast()));
            initialized?;
            let reader = client.GetService()?;
            client.Start()?;
            Ok(Self {
                client,
                reader,
                channels,
                float: true,
                recent: VecDeque::with_capacity(WINDOW * 2),
                ready: None,
            })
        }
    }

    /// Everything one app plays. This needs Windows 11, and fails on Windows 10.
    pub fn of_process(process_id: u32) -> Result<Self> {
        let params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: process_id,
                    ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
                },
            },
        };
        let blob = BlobVariant {
            kind: VT_BLOB.0,
            reserved: [0; 3],
            size: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            data: &params,
        };
        let (done, finished) = mpsc::channel();
        let handler: IActivateAudioInterfaceCompletionHandler = Activated(done).into();

        // SAFETY: `blob` and `params` outlive the wait below, the variant has the
        // layout Windows expects of a blob, and the buffer flags and format
        // match this call.
        unsafe {
            let operation = ActivateAudioInterfaceAsync(
                VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
                &IAudioClient::IID,
                Some((&raw const blob).cast::<PROPVARIANT>()),
                &handler,
            )?;
            finished
                .recv_timeout(ACTIVATION_TIMEOUT)
                .map_err(|_| Error::empty())?;

            let mut result = HRESULT(0);
            let mut activated: Option<IUnknown> = None;
            operation.GetActivateResult(&mut result, &mut activated)?;
            result.ok()?;
            let client: IAudioClient = activated.ok_or_else(Error::empty)?.cast()?;

            let channels = 2u16;
            let format = WAVEFORMATEX {
                wFormatTag: WAVE_FORMAT_PCM,
                nChannels: channels,
                nSamplesPerSec: SAMPLE_RATE,
                nAvgBytesPerSec: SAMPLE_RATE * u32::from(channels) * 2,
                nBlockAlign: channels * 2,
                wBitsPerSample: 16,
                cbSize: 0,
            };
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                BUFFER,
                0,
                &format,
                None,
            )?;
            // This kind of stream refuses to start without an event, though
            // Pulse reads it on a timer and never waits on it.
            let ready = CreateEventW(None, false, false, None)?;
            let reader = client
                .SetEventHandle(ready)
                .and_then(|()| client.GetService())
                .and_then(|reader| client.Start().map(|()| reader));
            let reader = match reader {
                Ok(reader) => reader,
                Err(e) => {
                    let _ = CloseHandle(ready);
                    return Err(e);
                }
            };
            Ok(Self {
                client,
                reader,
                channels: usize::from(channels),
                float: false,
                recent: VecDeque::with_capacity(WINDOW * 2),
                ready: Some(ready),
            })
        }
    }

    /// Takes in whatever has played since the last read.
    pub fn read(&mut self) -> Result<()> {
        // SAFETY: the pointer is read only between GetBuffer and ReleaseBuffer,
        // within the frames Windows reports, in the format chosen at the start.
        unsafe {
            while self.reader.GetNextPacketSize()? > 0 {
                let mut data = std::ptr::null_mut();
                let mut frames = 0;
                let mut flags = 0;
                self.reader
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;

                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
                for frame in 0..frames as usize {
                    let first = frame * self.channels;
                    let sample = if silent {
                        0.0
                    } else if self.float {
                        *data.cast::<f32>().add(first)
                    } else {
                        f32::from(*data.cast::<i16>().add(first)) / f32::from(i16::MAX)
                    };
                    self.recent.push_back(sample);
                }
                self.reader.ReleaseBuffer(frames)?;
            }
        }
        while self.recent.len() > WINDOW {
            self.recent.pop_front();
        }
        Ok(())
    }

    /// How much bass, mids and treble are in the latest sound, each from 0 to 1.
    pub fn bands(&self) -> [f32; 3] {
        if self.recent.len() < WINDOW {
            return [0.0; 3];
        }
        let samples: Vec<f32> = self.recent.iter().copied().collect();
        let bass = softened(&samples, BASS_SPAN);
        let lower = softened(&samples, MIDS_SPAN);

        let (mut bass_power, mut mids_power, mut treble_power) = (0.0, 0.0, 0.0);
        for index in 0..samples.len() {
            bass_power += bass[index] * bass[index];
            let mids = lower[index] - bass[index];
            mids_power += mids * mids;
            let treble = samples[index] - lower[index];
            treble_power += treble * treble;
        }
        let strength = |power: f32, gain: f32| {
            let level = (power / samples.len() as f32).sqrt() * gain;
            (level.min(1.0).sqrt() * 1000.0).round() / 1000.0
        };
        [
            strength(bass_power, BASS_GAIN),
            strength(mids_power, MIDS_GAIN),
            strength(treble_power, TREBLE_GAIN),
        ]
    }
}

/// Averages each sample with those before it, which removes what moves faster.
fn softened(samples: &[f32], span: usize) -> Vec<f32> {
    let mut sum = 0.0;
    samples
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            sum += sample;
            if index >= span {
                sum -= samples[index - span];
            }
            sum / (index + 1).min(span) as f32
        })
        .collect()
}
