//! Speakers, headphones and microphones: which exist, which is the default, and how
//! loud each default is. Every call here happens on the audio thread.

use std::sync::mpsc::Sender;

use serde_json::{Value as Json, json};
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::PROPERTYKEY,
        Media::Audio::{
            AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole,
            Endpoints::{
                IAudioEndpointVolume, IAudioEndpointVolumeCallback,
                IAudioEndpointVolumeCallback_Impl, IAudioMeterInformation,
            },
            IMMDevice, IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl,
            MMDeviceEnumerator, eCapture, eConsole, eRender,
        },
        System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, STGM_READ},
    },
    core::{GUID, PCWSTR, Result, implement},
};

use super::{Action, Flow, default_device, worker::Command};

/// Tells the worker that the list of devices or the default has changed.
#[implement(IMMNotificationClient)]
struct DeviceEvents(Sender<Command>);

impl DeviceEvents {
    fn changed(&self) -> Result<()> {
        let _ = self.0.send(Command::Refresh);
        Ok(())
    }
}

impl IMMNotificationClient_Impl for DeviceEvents_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> Result<()> {
        self.changed()
    }

    fn OnDeviceAdded(&self, _: &PCWSTR) -> Result<()> {
        self.changed()
    }

    fn OnDeviceRemoved(&self, _: &PCWSTR) -> Result<()> {
        self.changed()
    }

    fn OnDefaultDeviceChanged(&self, _: EDataFlow, _: ERole, _: &PCWSTR) -> Result<()> {
        self.changed()
    }

    fn OnPropertyValueChanged(&self, _: &PCWSTR, _: &PROPERTYKEY) -> Result<()> {
        self.changed()
    }
}

/// Tells the worker that a device's volume has changed, from anywhere.
#[implement(IAudioEndpointVolumeCallback)]
struct VolumeEvents(Sender<Command>);

impl IAudioEndpointVolumeCallback_Impl for VolumeEvents_Impl {
    fn OnNotify(&self, _: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        let _ = self.0.send(Command::Refresh);
        Ok(())
    }
}

/// The current default device of one flow, with its volume being watched.
struct Watched {
    id: String,
    volume: IAudioEndpointVolume,
    meter: IAudioMeterInformation,
    callback: IAudioEndpointVolumeCallback,
}

impl Drop for Watched {
    fn drop(&mut self) {
        // SAFETY: this callback was registered on this object by `watch`.
        let _ = unsafe { self.volume.UnregisterControlChangeNotify(&self.callback) };
    }
}

pub struct Devices {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
    events: Sender<Command>,
    output: Option<Watched>,
    input: Option<Watched>,
}

impl Drop for Devices {
    fn drop(&mut self) {
        // SAFETY: this client was registered on this enumerator by `new`.
        let _ = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.client)
        };
    }
}

fn windows_flow(flow: Flow) -> EDataFlow {
    match flow {
        Flow::Output => eRender,
        Flow::Input => eCapture,
    }
}

fn id_of(device: &IMMDevice) -> Result<String> {
    // SAFETY: Windows hands over the string, which is copied and then freed here.
    unsafe {
        let id = device.GetId()?;
        let text = id.to_string();
        CoTaskMemFree(Some(id.as_ptr().cast()));
        text.map_err(|_| windows::core::Error::empty())
    }
}

fn name_of(device: &IMMDevice) -> Result<String> {
    // SAFETY: the store is read-only and is used only within this call.
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ)?;
        Ok(store.GetValue(&PKEY_Device_FriendlyName)?.to_string())
    }
}

impl Devices {
    pub fn new(events: Sender<Command>) -> Result<Self> {
        let client: IMMNotificationClient = DeviceEvents(events.clone()).into();
        // SAFETY: called on a thread with COM started, and the client is
        // unregistered before it is released, when this value is dropped.
        let enumerator: IMMDeviceEnumerator = unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            enumerator.RegisterEndpointNotificationCallback(&client)?;
            enumerator
        };
        Ok(Self {
            enumerator,
            client,
            events,
            output: None,
            input: None,
        })
    }

    pub fn default_device(&self, flow: Flow) -> Option<IMMDevice> {
        // SAFETY: a plain query. It fails when there is no device of this kind.
        unsafe {
            self.enumerator
                .GetDefaultAudioEndpoint(windows_flow(flow), eConsole)
                .ok()
        }
    }

    fn list(&self, flow: Flow) -> Result<Vec<Json>> {
        // SAFETY: plain queries over a collection owned by this call.
        unsafe {
            let devices = self
                .enumerator
                .EnumAudioEndpoints(windows_flow(flow), DEVICE_STATE_ACTIVE)?;
            (0..devices.GetCount()?)
                .map(|index| {
                    let device = devices.Item(index)?;
                    Ok(json!({ "id": id_of(&device)?, "name": name_of(&device)? }))
                })
                .collect()
        }
    }

    /// Follows the default device, moving the volume watch whenever it changes.
    fn watch(&self, device: &IMMDevice, id: String) -> Result<Watched> {
        let callback: IAudioEndpointVolumeCallback = VolumeEvents(self.events.clone()).into();
        // SAFETY: the callback is unregistered before release, by `Watched::drop`.
        let (volume, meter): (IAudioEndpointVolume, IAudioMeterInformation) = unsafe {
            let meter = device.Activate(CLSCTX_ALL, None)?;
            let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None)?;
            volume.RegisterControlChangeNotify(&callback)?;
            (volume, meter)
        };
        Ok(Watched {
            id,
            volume,
            meter,
            callback,
        })
    }

    fn describe(&mut self, flow: Flow) -> Result<Json> {
        let devices = self.list(flow)?;
        let default = self.default_device(flow);
        let current = default.as_ref().map(id_of).transpose()?;

        let watching = match flow {
            Flow::Output => self.output.as_ref(),
            Flow::Input => self.input.as_ref(),
        }
        .map(|watched| watched.id.clone());
        if watching != current {
            let watched = match (&default, &current) {
                (Some(device), Some(id)) => Some(self.watch(device, id.clone())?),
                _ => None,
            };
            match flow {
                Flow::Output => self.output = watched,
                Flow::Input => self.input = watched,
            }
        }

        let watched = match flow {
            Flow::Output => self.output.as_ref(),
            Flow::Input => self.input.as_ref(),
        };
        let (volume, muted) = match watched {
            // SAFETY: plain queries on an object this struct owns.
            Some(watched) => unsafe {
                (
                    watched.volume.GetMasterVolumeLevelScalar()?,
                    watched.volume.GetMute()?.as_bool(),
                )
            },
            None => (0.0, false),
        };
        Ok(json!({ "devices": devices, "current": current, "volume": volume, "muted": muted }))
    }

    /// Everything the Audio tab needs, read fresh from Windows.
    pub fn snapshot(&mut self) -> std::result::Result<Json, String> {
        let read = |flow, devices: &mut Self| {
            devices
                .describe(flow)
                .map_err(|e| format!("Could not read audio devices: {e}"))
        };
        Ok(json!({
            "output": read(Flow::Output, self)?,
            "input": read(Flow::Input, self)?,
        }))
    }

    pub fn current_id(&self, flow: Flow) -> Option<String> {
        self.default_device(flow)
            .and_then(|device| id_of(&device).ok())
    }

    /// How loud a device is right now, from 0 to 1.
    pub fn peak(&self, flow: Flow) -> f32 {
        match flow {
            Flow::Output => self.output.as_ref(),
            Flow::Input => self.input.as_ref(),
        }
        // SAFETY: a plain query on an object this struct owns.
        .and_then(|watched| unsafe { watched.meter.GetPeakValue() }.ok())
        .unwrap_or(0.0)
    }

    fn volume_of(&self, flow: Flow) -> std::result::Result<&IAudioEndpointVolume, String> {
        match flow {
            Flow::Output => self.output.as_ref(),
            Flow::Input => self.input.as_ref(),
        }
        .map(|watched| &watched.volume)
        .ok_or_else(|| "That device is no longer connected".to_owned())
    }

    pub fn perform(&self, action: Action) -> std::result::Result<(), String> {
        let context = GUID::zeroed();
        match action {
            Action::SetDevice { id, .. } => {
                default_device::set(&id).map_err(|e| format!("Could not switch device: {e}"))
            }
            Action::SetVolume { flow, value } => {
                let volume = self.volume_of(flow)?;
                // SAFETY: a plain call on an object this struct owns.
                unsafe { volume.SetMasterVolumeLevelScalar(value.clamp(0.0, 1.0), &context) }
                    .map_err(|e| format!("Could not set the volume: {e}"))
            }
            Action::SetMuted { flow, muted } => {
                let volume = self.volume_of(flow)?;
                // SAFETY: a plain call on an object this struct owns.
                unsafe { volume.SetMute(muted, &context) }
                    .map_err(|e| format!("Could not change mute: {e}"))
            }
            _ => Err("That is not a device action".to_owned()),
        }
    }
}
