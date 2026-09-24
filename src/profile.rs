// Local connection-profile storage and secure credential access.
use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

const KEYRING_SERVICE: &str = "com.lucaseufrasio.valkey-manager";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ConnectionProfile {
    pub id: Uuid,
    pub name: String,
    /// Redis-protocol URL with its password removed.
    pub endpoint: String,
}

#[derive(Deserialize, Serialize)]
struct ProfileStore {
    profiles: Vec<ConnectionProfile>,
}

pub fn prepare_profile(
    id: Option<Uuid>,
    name: &str,
    endpoint: &str,
) -> Result<(ConnectionProfile, Option<String>), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Profile name cannot be empty".to_owned());
    }

    let mut endpoint = Url::parse(endpoint.trim()).map_err(|error| error.to_string())?;
    if !matches!(endpoint.scheme(), "redis" | "rediss") {
        return Err("Endpoint must use redis:// or rediss://".to_owned());
    }

    let password = endpoint.password().map(str::to_owned);
    endpoint
        .set_password(None)
        .map_err(|_| "Unable to remove password from saved endpoint".to_owned())?;

    Ok((
        ConnectionProfile {
            id: id.unwrap_or_else(Uuid::new_v4),
            name: name.to_owned(),
            endpoint: endpoint.into(),
        },
        password,
    ))
}

pub fn endpoint_with_password(endpoint: &str, password: Option<&str>) -> Result<String, String> {
    let mut endpoint = Url::parse(endpoint).map_err(|error| error.to_string())?;
    if let Some(password) = password {
        endpoint
            .set_password(Some(password))
            .map_err(|_| "Unable to add password to endpoint".to_owned())?;
    }
    Ok(endpoint.into())
}

pub fn save_password(profile_id: Uuid, password: &str) -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, &profile_id.to_string())
        .map_err(|error| error.to_string())?
        .set_password(password)
        .map_err(|error| error.to_string())
}

pub fn load_password(profile_id: Uuid) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &profile_id.to_string())
        .map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub fn delete_password(profile_id: Uuid) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, &profile_id.to_string())
        .map_err(|error| error.to_string())?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn load_profiles() -> Result<Vec<ConnectionProfile>, String> {
    load_profiles_from(&profiles_path()?)
}

fn load_profiles_from(path: &Path) -> Result<Vec<ConnectionProfile>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let store: ProfileStore = toml::from_str(&contents).map_err(|error| error.to_string())?;
    Ok(store.profiles)
}

pub fn save_profiles(profiles: &[ConnectionProfile]) -> Result<(), String> {
    save_profiles_to(&profiles_path()?, profiles)
}

fn save_profiles_to(path: &Path, profiles: &[ConnectionProfile]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let contents = toml::to_string_pretty(&ProfileStore {
        profiles: profiles.to_vec(),
    })
    .map_err(|error| error.to_string())?;
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn profiles_path() -> Result<PathBuf, String> {
    ProjectDirs::from("com", "lucaseufrasio", "valkey-manager")
        .map(|directories| directories.config_dir().join("profiles.toml"))
        .ok_or_else(|| "Could not locate the application config directory".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_separates_password_from_saved_endpoint() {
        let (profile, password) = prepare_profile(
            None,
            "Local",
            "redis://alice:private-value@localhost:6379/0",
        )
        .unwrap();

        assert_eq!(password.as_deref(), Some("private-value"));
        let saved = toml::to_string(&profile).unwrap();
        assert!(!saved.contains("private-value"));
        let saved_url = Url::parse(&profile.endpoint).unwrap();
        assert_eq!(saved_url.username(), "alice");
        assert_eq!(saved_url.password(), None);

        let config = toml::to_string(&ProfileStore {
            profiles: vec![profile.clone()],
        })
        .unwrap();
        let restored: ProfileStore = toml::from_str(&config).unwrap();
        assert_eq!(restored.profiles[0].id, profile.id);
        assert_eq!(restored.profiles[0].endpoint, profile.endpoint);
    }

    #[test]
    fn profile_rejects_non_valkey_protocol_schemes() {
        let error = prepare_profile(None, "Wrong scheme", "https://localhost:6379").unwrap_err();
        assert!(error.contains("redis:// or rediss://"));
    }

    #[test]
    fn profiles_round_trip_on_disk_without_passwords() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("profiles.toml");
        let (profile, password) = prepare_profile(
            None,
            "Integration",
            "rediss://user:secret@valkey.example:6380",
        )
        .unwrap();

        save_profiles_to(&path, std::slice::from_ref(&profile)).unwrap();
        let loaded = load_profiles_from(&path).unwrap();

        assert_eq!(password.as_deref(), Some("secret"));
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, profile.id);
        assert!(!fs::read_to_string(path).unwrap().contains("secret"));
    }
}
