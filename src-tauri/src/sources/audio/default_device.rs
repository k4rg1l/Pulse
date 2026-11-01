//! Choosing the default device. Windows has no public way to do this, so this uses
//! the same private interface as its own Sound settings and apps like EarTrumpet.
//! It lives alone here so that if Windows ever changes it, only this control fails.

use windows::{
    Win32::{
        Media::Audio::{ERole, eCommunications, eConsole, eMultimedia},
        System::Com::{CLSCTX_ALL, CoCreateInstance},
    },
    core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, PCWSTR, Result, interface},
};

const POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

/// Only the last method is called. The others hold its place in the interface.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    unsafe fn get_mix_format(&self) -> HRESULT;
    unsafe fn get_device_format(&self) -> HRESULT;
    unsafe fn reset_device_format(&self) -> HRESULT;
    unsafe fn set_device_format(&self) -> HRESULT;
    unsafe fn get_processing_period(&self) -> HRESULT;
    unsafe fn set_processing_period(&self) -> HRESULT;
    unsafe fn get_share_mode(&self) -> HRESULT;
    unsafe fn set_share_mode(&self) -> HRESULT;
    unsafe fn get_property_value(&self) -> HRESULT;
    unsafe fn set_property_value(&self) -> HRESULT;
    unsafe fn set_default_endpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT;
}

/// Makes a device the default for everything, as Sound settings does.
pub fn set(device_id: &str) -> Result<()> {
    let wide: Vec<u16> = device_id.encode_utf16().chain([0]).collect();
    // SAFETY: `wide` is a terminated string that outlives these calls, and the
    // interface matches the object Windows creates for this class.
    unsafe {
        let policy: IPolicyConfig = CoCreateInstance(&POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)?;
        for role in [eConsole, eMultimedia, eCommunications] {
            policy
                .set_default_endpoint(PCWSTR(wide.as_ptr()), role)
                .ok()?;
        }
    }
    Ok(())
}
