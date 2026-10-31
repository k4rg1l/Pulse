//! API keys, held in Windows Credential Manager. The client can write a key and
//! ask whether one is set. Nothing returns a key to the client.

use std::collections::HashMap;

use keyring_core::{Entry, Error};

const SERVICE: &str = "pulse";

pub fn init() -> Result<(), String> {
    let store = windows_native_keyring_store::Store::new().map_err(|e| e.to_string())?;
    keyring_core::set_default_store(store);
    Ok(())
}

fn entry(source_id: &str, name: &str) -> Result<Entry, Error> {
    Entry::new(SERVICE, &format!("{source_id}/{name}"))
}

pub fn is_set(source_id: &str, name: &str) -> Result<bool, String> {
    match entry(source_id, name).and_then(|entry| entry.get_password()) {
        Ok(_) => Ok(true),
        Err(Error::NoEntry) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

pub fn set(source_id: &str, name: &str, value: &str) -> Result<(), String> {
    entry(source_id, name)
        .and_then(|entry| entry.set_password(value))
        .map_err(|e| e.to_string())
}

/// The given keys for one source. Keys that are not set are left out, so the
/// source can report which one it is missing.
pub fn load(source_id: &str, names: &[&str]) -> HashMap<String, String> {
    names
        .iter()
        .filter_map(|&name| {
            let value = entry(source_id, name).and_then(|entry| entry.get_password()).ok()?;
            Some((name.to_owned(), value))
        })
        .collect()
}
