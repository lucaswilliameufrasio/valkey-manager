// Native egui application and Valkey client services.
use std::{
    sync::mpsc::{self, Receiver, Sender},
    sync::Arc,
    time::Duration,
};

use eframe::egui;
use fred::{
    prelude::*,
    types::{ClusterHash, CustomCommand, Expiration, Scanner, SetOptions},
};
use futures_util::TryStreamExt;
use uuid::Uuid;

mod profile;
use profile::ConnectionProfile;

const MAX_KEYS_PER_SCAN: usize = 500;

enum UiEvent {
    Connected(Result<RedisClient, String>),
    Keys(Result<Vec<String>, String>),
    KeyDetails(String, Result<KeyDetails, String>),
    KeySaved(String, Result<(), String>),
    KeyDeleted(String, Result<(), String>),
    KeyRenamed(String, String, Result<(), String>),
    Disconnected(Result<(), String>),
    CommandResult(Result<String, String>),
    MonitorResult(Result<MonitorSnapshot, String>),
    KeyUpdated(String, Result<(), String>),
    KeyCreated(String, Result<(), String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WorkspaceTab {
    Keys,
    Console,
    Monitor,
}

#[derive(Clone)]
struct KeyDetails {
    kind: String,
    ttl_seconds: i64,
    value: KeyValue,
}

#[derive(Clone, Debug, PartialEq)]
pub enum KeyValue {
    String(String),
    List(Vec<String>),
    Hash(Vec<(String, String)>),
    Set(Vec<String>),
    SortedSet(Vec<(String, f64)>),
    Unsupported,
}

#[derive(Clone, Default)]
struct MonitorSnapshot {
    ping: String,
    version: String,
    uptime_seconds: String,
    connected_clients: String,
    used_memory: String,
    commands_processed: String,
    keys_in_database: String,
}

fn info_field(info: &str, name: &str) -> String {
    info.lines()
        .find_map(|line| line.strip_prefix(&format!("{name}:")))
        .unwrap_or("—")
        .to_owned()
}

fn parse_command(input: &str) -> Result<(String, Vec<String>), String> {
    let mut parts =
        shlex::split(input).ok_or_else(|| "Could not parse command arguments".to_owned())?;
    if parts.is_empty() {
        return Err("Enter a command first".to_owned());
    }
    let command = parts.remove(0).to_ascii_uppercase();
    Ok((command, parts))
}

pub async fn read_key_value(
    client: &RedisClient,
    key: &str,
    kind: &str,
) -> Result<KeyValue, String> {
    match kind {
        "string" => client
            .get::<Option<String>, _>(key)
            .await
            .map(|value| KeyValue::String(value.unwrap_or_default()))
            .map_err(|error| error.to_string()),
        "list" => client
            .lrange::<Vec<String>, _>(key, 0, 99)
            .await
            .map(KeyValue::List)
            .map_err(|error| error.to_string()),
        "hash" => {
            let scanner = client.hscan(key, "*", Some(100));
            futures_util::pin_mut!(scanner);
            let mut fields = Vec::new();
            while let Some(mut page) = scanner
                .try_next()
                .await
                .map_err(|error| error.to_string())?
            {
                if let Some(results) = page.take_results() {
                    fields.extend(results.iter().map(|(field, value)| {
                        (
                            field.as_str_lossy().to_string(),
                            value
                                .as_str_lossy()
                                .map(|value| value.into_owned())
                                .unwrap_or_default(),
                        )
                    }));
                }
                if fields.len() >= 100 {
                    break;
                }
                page.next().map_err(|error| error.to_string())?;
            }
            Ok(KeyValue::Hash(fields))
        }
        "set" => {
            let scanner = client.sscan(key, "*", Some(100));
            futures_util::pin_mut!(scanner);
            let mut members = Vec::new();
            while let Some(mut page) = scanner
                .try_next()
                .await
                .map_err(|error| error.to_string())?
            {
                if let Some(results) = page.take_results() {
                    members.extend(
                        results.into_iter().filter_map(|value| {
                            value.as_str_lossy().map(|value| value.into_owned())
                        }),
                    );
                }
                if members.len() >= 100 {
                    break;
                }
                page.next().map_err(|error| error.to_string())?;
            }
            Ok(KeyValue::Set(members))
        }
        "zset" => client
            .zrange::<Vec<(String, f64)>, _, _, _>(key, 0, 99, None, false, None, true)
            .await
            .map(KeyValue::SortedSet)
            .map_err(|error| error.to_string()),
        _ => Ok(KeyValue::Unsupported),
    }
}

struct ValkeyManagerApp {
    endpoint: String,
    password: String,
    pattern: String,
    profile_name: String,
    status: String,
    connecting: bool,
    client: Option<RedisClient>,
    runtime: Arc<tokio::runtime::Runtime>,
    profiles: Vec<ConnectionProfile>,
    selected_profile: Option<Uuid>,
    keys: Vec<String>,
    selected_key: Option<String>,
    key_details: Option<KeyDetails>,
    key_value: String,
    ttl_input: String,
    rename_input: String,
    confirm_delete: bool,
    active_tab: WorkspaceTab,
    command_input: String,
    command_output: String,
    monitor: Option<MonitorSnapshot>,
    new_key_name: String,
    new_key_value: String,
    new_key_ttl: String,
    list_item_input: String,
    hash_field_input: String,
    hash_value_input: String,
    set_member_input: String,
    sorted_member_input: String,
    sorted_score_input: String,
    pending_collection_remove: Option<(String, Vec<String>)>,
    sender: Sender<UiEvent>,
    receiver: Receiver<UiEvent>,
}

impl Default for ValkeyManagerApp {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        let (profiles, load_error) = match profile::load_profiles() {
            Ok(profiles) => (profiles, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        let selected = profiles.first();
        Self {
            endpoint: selected
                .map(|profile| profile.endpoint.clone())
                .unwrap_or_else(|| "redis://127.0.0.1:6379".to_owned()),
            password: String::new(),
            pattern: "*".to_owned(),
            profile_name: selected
                .map(|profile| profile.name.clone())
                .unwrap_or_else(|| "Local Valkey".to_owned()),
            status: load_error.unwrap_or_else(|| "Not connected".to_owned()),
            connecting: false,
            client: None,
            runtime: Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("create Tokio runtime"),
            ),
            selected_profile: selected.map(|profile| profile.id),
            profiles,
            keys: Vec::new(),
            selected_key: None,
            key_details: None,
            key_value: String::new(),
            ttl_input: String::new(),
            rename_input: String::new(),
            confirm_delete: false,
            active_tab: WorkspaceTab::Keys,
            command_input: String::new(),
            command_output: String::new(),
            monitor: None,
            new_key_name: String::new(),
            new_key_value: String::new(),
            new_key_ttl: String::new(),
            list_item_input: String::new(),
            hash_field_input: String::new(),
            hash_value_input: String::new(),
            set_member_input: String::new(),
            sorted_member_input: String::new(),
            sorted_score_input: String::new(),
            pending_collection_remove: None,
            sender,
            receiver,
        }
    }
}

impl ValkeyManagerApp {
    fn select_profile(&mut self, selected: Option<Uuid>) {
        self.selected_profile = selected;
        if let Some(profile) = self
            .profiles
            .iter()
            .find(|profile| Some(profile.id) == selected)
        {
            self.profile_name = profile.name.clone();
            self.endpoint = profile.endpoint.clone();
            self.password.clear();
        }
    }

    fn new_profile(&mut self) {
        self.select_profile(None);
        self.profile_name = "New Valkey".to_owned();
        self.endpoint = "redis://127.0.0.1:6379".to_owned();
        self.password.clear();
    }

    fn save_profile(&mut self) {
        let prepared =
            profile::prepare_profile(self.selected_profile, &self.profile_name, &self.endpoint);
        let (saved_profile, uri_password) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.status = error;
                return;
            }
        };

        let password = if self.password.is_empty() {
            uri_password
        } else {
            Some(self.password.clone())
        };
        if let Some(password) = password.as_deref() {
            if let Err(error) = profile::save_password(saved_profile.id, password) {
                self.status = format!("Could not save password to system keychain: {error}");
                return;
            }
        }

        if let Some(existing) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.id == saved_profile.id)
        {
            *existing = saved_profile.clone();
        } else {
            self.profiles.push(saved_profile.clone());
        }

        if let Err(error) = profile::save_profiles(&self.profiles) {
            self.status = format!("Could not save connection profile: {error}");
            return;
        }
        self.selected_profile = Some(saved_profile.id);
        self.profile_name = saved_profile.name;
        self.endpoint = saved_profile.endpoint;
        self.password.clear();
        self.status = if password.is_some() {
            "Profile saved · password stored in system keychain".to_owned()
        } else {
            "Profile saved".to_owned()
        };
    }

    fn delete_selected_profile(&mut self) {
        let Some(selected) = self.selected_profile else {
            return;
        };
        let remaining = self
            .profiles
            .iter()
            .filter(|profile| profile.id != selected)
            .cloned()
            .collect::<Vec<_>>();
        if let Err(error) = profile::save_profiles(&remaining) {
            self.status = format!("Could not remove connection profile: {error}");
            return;
        }
        self.profiles = remaining;
        if let Err(error) = profile::delete_password(selected) {
            self.status =
                format!("Profile removed, but its keychain entry could not be deleted: {error}");
        } else {
            self.status = "Profile removed".to_owned();
        }
        self.password.clear();
        self.select_profile(self.profiles.first().map(|profile| profile.id));
    }

    fn connect(&mut self) {
        let (connection_profile, uri_password) =
            match profile::prepare_profile(self.selected_profile, "Connection", &self.endpoint) {
                Ok(prepared) => prepared,
                Err(error) => {
                    self.status = error;
                    return;
                }
            };
        self.endpoint = connection_profile.endpoint.clone();
        let entered_password = self.password.clone();
        let selected_profile = self.profiles.iter().find(|profile| {
            Some(profile.id) == self.selected_profile
                && connection_profile.endpoint == profile.endpoint
        });
        let selected_profile_id = selected_profile.map(|profile| profile.id);
        let sender = self.sender.clone();
        self.status = "Connecting…".to_owned();
        self.connecting = true;
        self.client = None;

        self.runtime.spawn(async move {
            let result = async {
                let password = match (entered_password, uri_password) {
                    (password, _) if !password.is_empty() => Some(password),
                    (_, Some(password)) => Some(password),
                    (_, None) => match selected_profile_id {
                        Some(id) => profile::load_password(id)?,
                        None => None,
                    },
                };
                let endpoint = profile::endpoint_with_password(
                    &connection_profile.endpoint,
                    password.as_deref(),
                )?;
                let config = RedisConfig::from_url(&endpoint).map_err(|error| error.to_string())?;
                let client = RedisClient::new(config, None, None, None);
                drop(client.connect());
                match tokio::time::timeout(Duration::from_secs(8), client.wait_for_connect()).await
                {
                    Ok(Ok(())) => Ok(client),
                    Ok(Err(error)) => {
                        let _ = client.quit().await;
                        Err(error.to_string())
                    }
                    Err(_) => {
                        let _ = client.quit().await;
                        Err("Connection timed out".to_owned())
                    }
                }
            }
            .await;
            let _ = sender.send(UiEvent::Connected(result));
        });
    }

    fn disconnect(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.status = "Disconnecting…".to_owned();
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = client.quit().await.map_err(|error| error.to_string());
            let _ = sender.send(UiEvent::Disconnected(result));
        });
    }

    fn scan_keys(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.selected_key = None;
        self.key_details = None;
        self.key_value.clear();
        self.ttl_input.clear();
        let pattern = self.pattern.trim().to_owned();
        let sender = self.sender.clone();
        self.status = "Scanning keys…".to_owned();

        self.runtime.spawn(async move {
            let result = async {
                let mut scanner = client.scan(pattern, Some(100), None);
                let mut keys = Vec::new();
                while let Some(mut page) = scanner
                    .try_next()
                    .await
                    .map_err(|error| error.to_string())?
                {
                    if let Some(results) = page.take_results() {
                        let remaining = MAX_KEYS_PER_SCAN.saturating_sub(keys.len());
                        keys.extend(
                            results
                                .into_iter()
                                .take(remaining)
                                .map(|key| key.as_str_lossy().to_string()),
                        );
                    }
                    if keys.len() >= MAX_KEYS_PER_SCAN {
                        break;
                    }
                    page.next().map_err(|error| error.to_string())?;
                }
                Ok(keys)
            }
            .await;
            let _ = sender.send(UiEvent::Keys(result));
        });
    }

    fn load_key(&mut self, key: String) {
        let Some(client) = self.client.clone() else {
            return;
        };
        self.selected_key = Some(key.clone());
        self.key_details = None;
        self.key_value.clear();
        self.ttl_input.clear();
        self.rename_input = key.clone();
        self.status = format!("Loading {key}…");
        let sender = self.sender.clone();

        self.runtime.spawn(async move {
            let result = async {
                let kind: String = client
                    .custom(
                        CustomCommand::new("TYPE", ClusterHash::FirstKey, false),
                        vec![key.clone()],
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let ttl_seconds = client.ttl(&key).await.map_err(|error| error.to_string())?;
                let value = read_key_value(&client, &key, &kind).await?;
                Ok(KeyDetails {
                    kind,
                    ttl_seconds,
                    value,
                })
            }
            .await;
            let _ = sender.send(UiEvent::KeyDetails(key, result));
        });
    }

    fn save_selected_string(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(key) = self.selected_key.clone() else {
            return;
        };
        let ttl_seconds = match self.ttl_input.trim() {
            "" => None,
            value => match value.parse::<i64>() {
                Ok(seconds) if seconds > 0 => Some(seconds),
                _ => {
                    self.status = "TTL must be a positive number of seconds".to_owned();
                    return;
                }
            },
        };
        let value = self.key_value.clone();
        let sender = self.sender.clone();
        self.status = format!("Saving {key}…");

        self.runtime.spawn(async move {
            let result = async {
                if let Some(seconds) = ttl_seconds {
                    client
                        .set::<(), _, _>(&key, value, Some(Expiration::EX(seconds)), None, false)
                        .await
                        .map_err(|error| error.to_string())?;
                } else {
                    client
                        .set::<(), _, _>(&key, value, None, None, false)
                        .await
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            }
            .await;
            let _ = sender.send(UiEvent::KeySaved(key, result));
        });
    }

    fn delete_selected_key(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(key) = self.selected_key.clone() else {
            return;
        };
        let sender = self.sender.clone();
        self.confirm_delete = false;
        self.status = format!("Deleting {key}…");

        self.runtime.spawn(async move {
            let result = client
                .del::<i64, _>(&[key.as_str()])
                .await
                .map(|_| ())
                .map_err(|error| error.to_string());
            let _ = sender.send(UiEvent::KeyDeleted(key, result));
        });
    }

    fn rename_selected_key(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let Some(key) = self.selected_key.clone() else {
            return;
        };
        let new_key = self.rename_input.trim().to_owned();
        if new_key.is_empty() || new_key == key {
            self.status = "Enter a different, non-empty key name".to_owned();
            return;
        }
        let sender = self.sender.clone();
        self.status = format!("Renaming {key}…");

        self.runtime.spawn(async move {
            let result: Result<(), String> = match client.renamenx(&key, &new_key).await {
                Ok(true) => Ok(()),
                Ok(false) => Err("Destination key already exists".to_owned()),
                Err(error) => Err(error.to_string()),
            };
            let _ = sender.send(UiEvent::KeyRenamed(key, new_key, result));
        });
    }

    fn create_string_key(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let key = self.new_key_name.trim().to_owned();
        if key.is_empty() {
            self.status = "Enter a key name".to_owned();
            return;
        }
        let ttl = match self.new_key_ttl.trim() {
            "" => None,
            value => match value.parse::<i64>() {
                Ok(seconds) if seconds > 0 => Some(Expiration::EX(seconds)),
                _ => {
                    self.status = "TTL must be a positive number of seconds".to_owned();
                    return;
                }
            },
        };
        let value = self.new_key_value.clone();
        let sender = self.sender.clone();
        self.status = format!("Creating {key}…");

        self.runtime.spawn(async move {
            let result = client
                .set::<Option<String>, _, _>(&key, value, ttl, Some(SetOptions::NX), false)
                .await
                .map_err(|error| error.to_string())
                .and_then(|created| {
                    created
                        .map(|_| ())
                        .ok_or_else(|| "A key with this name already exists".to_owned())
                });
            let _ = sender.send(UiEvent::KeyCreated(key, result));
        });
    }

    fn run_collection_command(&mut self, command: String, arguments: Vec<String>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let key = self.selected_key.clone().unwrap_or_default();
        let sender = self.sender.clone();
        self.status = format!("Running {command} on {key}…");
        self.runtime.spawn(async move {
            let result = client
                .custom::<RedisValue, _>(
                    CustomCommand::new(&command, ClusterHash::FirstKey, false),
                    arguments,
                )
                .await
                .map(|_| ())
                .map_err(|error| error.to_string());
            let _ = sender.send(UiEvent::KeyUpdated(key, result));
        });
    }

    fn run_console_command(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let (command, parts) = match parse_command(&self.command_input) {
            Ok(parsed) => parsed,
            Err(error) => {
                self.command_output = error;
                return;
            }
        };
        let sender = self.sender.clone();
        self.status = format!("Running {command}…");
        self.runtime.spawn(async move {
            let result = tokio::time::timeout(
                Duration::from_secs(30),
                client.custom::<RedisValue, _>(
                    CustomCommand::new(&command, ClusterHash::FirstKey, false),
                    parts,
                ),
            )
            .await
            .map_err(|_| "Command timed out after 30 seconds".to_owned())
            .and_then(|result| {
                result
                    .map(|value| format!("{value:?}"))
                    .map_err(|e| e.to_string())
            });
            let _ = sender.send(UiEvent::CommandResult(result));
        });
    }

    fn refresh_monitor(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let sender = self.sender.clone();
        self.status = "Refreshing server metrics…".to_owned();
        self.runtime.spawn(async move {
            let result: Result<MonitorSnapshot, String> = async {
                let ping: String = client.ping().await.map_err(|error| error.to_string())?;
                let info: String = client.info(None).await.map_err(|error| error.to_string())?;
                let keys_in_database: i64 =
                    client.dbsize().await.map_err(|error| error.to_string())?;
                Ok(MonitorSnapshot {
                    ping,
                    version: info_field(&info, "redis_version"),
                    uptime_seconds: info_field(&info, "uptime_in_seconds"),
                    connected_clients: info_field(&info, "connected_clients"),
                    used_memory: info_field(&info, "used_memory_human"),
                    commands_processed: info_field(&info, "total_commands_processed"),
                    keys_in_database: keys_in_database.to_string(),
                })
            }
            .await;
            let _ = sender.send(UiEvent::MonitorResult(result));
        });
    }

    fn render_keys(&mut self, ui: &mut egui::Ui) {
        ui.heading("Key browser");
        ui.separator();
        if self.client.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label("Connect to a Valkey instance to browse its keys.");
            });
            return;
        }
        ui.collapsing("Create string key", |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_key_name)
                        .hint_text("key name")
                        .desired_width(180.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_key_value)
                        .hint_text("value")
                        .desired_width(180.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut self.new_key_ttl)
                        .hint_text("TTL seconds")
                        .desired_width(110.0),
                );
                if ui.button("Create").clicked() {
                    self.create_string_key();
                }
            });
        });
        if self.keys.is_empty() {
            ui.label("No keys found for this pattern.");
        } else {
            let mut requested_key = None;
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for key in &self.keys {
                        if ui
                            .selectable_label(self.selected_key.as_ref() == Some(key), key)
                            .clicked()
                        {
                            requested_key = Some(key.clone());
                        }
                    }
                });
            if let Some(key) = requested_key {
                self.load_key(key);
            }
        }

        if let (Some(key), Some(details)) = (self.selected_key.clone(), self.key_details.clone()) {
            ui.separator();
            ui.horizontal(|ui| {
                ui.heading("Key details");
                ui.monospace(&key);
            });
            let ttl = match details.ttl_seconds {
                -2 => "Missing".to_owned(),
                -1 => "Persistent".to_owned(),
                seconds => format!("{seconds}s"),
            };
            ui.horizontal(|ui| {
                ui.label(format!("Type: {}", details.kind));
                ui.separator();
                ui.label(format!("TTL: {ttl}"));
            });

            match &details.value {
                KeyValue::String(_) => {
                    ui.label("Value");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.key_value)
                            .desired_rows(8)
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        ui.label("TTL seconds (blank = persistent)");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.ttl_input).desired_width(100.0),
                        );
                        if ui.button("Save value").clicked() {
                            self.save_selected_string();
                        }
                    });
                }
                KeyValue::List(values) => {
                    ui.label("List entries (first 100)");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.list_item_input)
                                .hint_text("append item")
                                .desired_width(220.0),
                        );
                        if ui.button("Append").clicked() && !self.list_item_input.is_empty() {
                            self.run_collection_command(
                                "RPUSH".to_owned(),
                                vec![key.clone(), self.list_item_input.clone()],
                            );
                            self.list_item_input.clear();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for (index, value) in values.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.monospace(format!("{index}: {value}"));
                                    if ui.small_button("Remove").clicked() {
                                        self.pending_collection_remove = Some((
                                            "LREM".to_owned(),
                                            vec![key.clone(), "1".to_owned(), value.clone()],
                                        ));
                                    }
                                });
                            }
                        });
                }
                KeyValue::Hash(fields) => {
                    ui.label("Hash fields (first 100)");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.hash_field_input)
                                .hint_text("field")
                                .desired_width(160.0),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.hash_value_input)
                                .hint_text("value")
                                .desired_width(160.0),
                        );
                        if ui.button("Add / update field").clicked()
                            && !self.hash_field_input.is_empty()
                        {
                            self.run_collection_command(
                                "HSET".to_owned(),
                                vec![
                                    key.clone(),
                                    self.hash_field_input.clone(),
                                    self.hash_value_input.clone(),
                                ],
                            );
                            self.hash_field_input.clear();
                            self.hash_value_input.clear();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for (field, value) in fields {
                                ui.horizontal(|ui| {
                                    ui.monospace(field);
                                    ui.label("→");
                                    ui.monospace(value);
                                    if ui.small_button("Remove").clicked() {
                                        self.pending_collection_remove = Some((
                                            "HDEL".to_owned(),
                                            vec![key.clone(), field.clone()],
                                        ));
                                    }
                                });
                            }
                        });
                }
                KeyValue::Set(members) => {
                    ui.label("Set members (first 100)");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.set_member_input)
                                .hint_text("member")
                                .desired_width(220.0),
                        );
                        if ui.button("Add member").clicked() && !self.set_member_input.is_empty() {
                            self.run_collection_command(
                                "SADD".to_owned(),
                                vec![key.clone(), self.set_member_input.clone()],
                            );
                            self.set_member_input.clear();
                        }
                    });
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for member in members {
                                ui.horizontal(|ui| {
                                    ui.monospace(member);
                                    if ui.small_button("Remove").clicked() {
                                        self.pending_collection_remove = Some((
                                            "SREM".to_owned(),
                                            vec![key.clone(), member.clone()],
                                        ));
                                    }
                                });
                            }
                        });
                }
                KeyValue::SortedSet(members) => {
                    ui.label("Sorted-set members (first 100)");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.sorted_member_input)
                                .hint_text("member")
                                .desired_width(170.0),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.sorted_score_input)
                                .hint_text("score")
                                .desired_width(90.0),
                        );
                        if ui.button("Add / update score").clicked() {
                            match self.sorted_score_input.parse::<f64>() {
                                Ok(score)
                                    if score.is_finite()
                                        && !self.sorted_member_input.is_empty() =>
                                {
                                    self.run_collection_command(
                                        "ZADD".to_owned(),
                                        vec![
                                            key.clone(),
                                            score.to_string(),
                                            self.sorted_member_input.clone(),
                                        ],
                                    );
                                    self.sorted_member_input.clear();
                                    self.sorted_score_input.clear();
                                }
                                _ => self.status = "Enter a member and numeric score".to_owned(),
                            }
                        }
                    });
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            for (member, score) in members {
                                ui.horizontal(|ui| {
                                    ui.monospace(member);
                                    ui.label(format!("score: {score}"));
                                    if ui.small_button("Remove").clicked() {
                                        self.pending_collection_remove = Some((
                                            "ZREM".to_owned(),
                                            vec![key.clone(), member.clone()],
                                        ));
                                    }
                                });
                            }
                        });
                }
                KeyValue::Unsupported => {
                    ui.label(format!(
                        "Value display is not supported for {}.",
                        details.kind
                    ));
                }
            }

            if let Some((command, arguments)) = self.pending_collection_remove.clone() {
                let target = arguments.last().cloned().unwrap_or_default();
                ui.group(|ui| {
                    ui.label(format!("Remove {target} from this key?"));
                    ui.horizontal(|ui| {
                        if ui.button("Confirm remove").clicked() {
                            self.pending_collection_remove = None;
                            self.run_collection_command(command, arguments);
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_collection_remove = None;
                        }
                    });
                });
            }

            ui.horizontal(|ui| {
                ui.label("Rename to");
                ui.add(egui::TextEdit::singleline(&mut self.rename_input).desired_width(220.0));
                if ui.button("Rename safely").clicked() {
                    self.rename_selected_key();
                }
                if ui.button("Delete…").clicked() {
                    self.confirm_delete = true;
                }
            });
            if self.confirm_delete {
                ui.group(|ui| {
                    ui.label(format!("Permanently delete {key}?"));
                    ui.horizontal(|ui| {
                        if ui.button("Confirm delete").clicked() {
                            self.delete_selected_key();
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm_delete = false;
                        }
                    });
                });
            }
        }
    }

    fn render_console(&mut self, ui: &mut egui::Ui) {
        ui.heading("Command console");
        ui.label("Arguments support shell-style quotes; no shell is invoked.");
        ui.horizontal(|ui| {
            let input = ui.add_enabled(
                self.client.is_some(),
                egui::TextEdit::singleline(&mut self.command_input)
                    .hint_text("GET my:key")
                    .desired_width(f32::INFINITY),
            );
            let run = ui
                .add_enabled(self.client.is_some(), egui::Button::new("Run"))
                .clicked();
            if run || (input.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)))
            {
                self.run_console_command();
            }
        });
        ui.label("Response");
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.command_output)
                    .desired_rows(16)
                    .desired_width(f32::INFINITY)
                    .interactive(false),
            );
        });
    }

    fn render_monitor(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Server monitor");
            if ui
                .add_enabled(self.client.is_some(), egui::Button::new("Refresh"))
                .clicked()
            {
                self.refresh_monitor();
            }
        });
        ui.separator();
        if let Some(snapshot) = &self.monitor {
            egui::Grid::new("server-metrics")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    ui.label("PING");
                    ui.monospace(&snapshot.ping);
                    ui.end_row();
                    ui.label("Valkey version");
                    ui.monospace(&snapshot.version);
                    ui.end_row();
                    ui.label("Uptime (seconds)");
                    ui.monospace(&snapshot.uptime_seconds);
                    ui.end_row();
                    ui.label("Connected clients");
                    ui.monospace(&snapshot.connected_clients);
                    ui.end_row();
                    ui.label("Memory used");
                    ui.monospace(&snapshot.used_memory);
                    ui.end_row();
                    ui.label("Commands processed");
                    ui.monospace(&snapshot.commands_processed);
                    ui.end_row();
                    ui.label("Keys in selected database");
                    ui.monospace(&snapshot.keys_in_database);
                    ui.end_row();
                });
        } else {
            ui.label("Refresh to load server health and INFO metrics.");
        }
    }

    fn receive_events(&mut self) {
        let events = self.receiver.try_iter().collect::<Vec<_>>();
        for event in events {
            match event {
                UiEvent::Connected(Ok(client)) => {
                    self.connecting = false;
                    self.password.clear();
                    self.client = Some(client);
                    self.status = "Connected".to_owned();
                    self.scan_keys();
                }
                UiEvent::Connected(Err(error)) => {
                    self.connecting = false;
                    self.status = error;
                }
                UiEvent::Keys(Err(error)) => {
                    self.status = error;
                }
                UiEvent::Keys(Ok(keys)) => {
                    self.keys = keys;
                    self.status = if self.keys.len() >= MAX_KEYS_PER_SCAN {
                        format!("Connected · {}+ keys", self.keys.len())
                    } else {
                        format!("Connected · {} keys", self.keys.len())
                    };
                }
                UiEvent::KeyDetails(key, Ok(details)) => {
                    if self.selected_key.as_deref() == Some(&key) {
                        self.key_value = match &details.value {
                            KeyValue::String(value) => value.clone(),
                            _ => String::new(),
                        };
                        self.ttl_input = if details.ttl_seconds > 0 {
                            details.ttl_seconds.to_string()
                        } else {
                            String::new()
                        };
                        self.rename_input = key.clone();
                        self.status = format!("Loaded {key}");
                        self.key_details = Some(details);
                    }
                }
                UiEvent::KeyDetails(key, Err(error)) => {
                    if self.selected_key.as_deref() == Some(&key) {
                        self.status = error;
                        self.key_details = None;
                    }
                }
                UiEvent::KeySaved(key, Ok(())) => {
                    if self.selected_key.as_deref() == Some(&key) {
                        self.status = format!("Saved {key}");
                        self.load_key(key);
                    }
                }
                UiEvent::KeySaved(_, Err(error)) => self.status = error,
                UiEvent::KeyDeleted(key, Ok(())) => {
                    if self.selected_key.as_deref() == Some(&key) {
                        self.selected_key = None;
                        self.key_details = None;
                        self.key_value.clear();
                        self.keys.retain(|existing| existing != &key);
                        self.status = format!("Deleted {key}");
                        self.scan_keys();
                    }
                }
                UiEvent::KeyDeleted(_, Err(error)) => self.status = error,
                UiEvent::KeyRenamed(old_key, new_key, Ok(())) => {
                    if self.selected_key.as_deref() == Some(&old_key) {
                        self.selected_key = Some(new_key.clone());
                        self.status = format!("Renamed {old_key} to {new_key}");
                        self.scan_keys();
                        self.load_key(new_key);
                    }
                }
                UiEvent::KeyRenamed(_, _, Err(error)) => self.status = error,
                UiEvent::Disconnected(Ok(())) => {
                    self.client = None;
                    self.keys.clear();
                    self.selected_key = None;
                    self.key_details = None;
                    self.status = "Not connected".to_owned();
                }
                UiEvent::Disconnected(Err(error)) => self.status = error,
                UiEvent::CommandResult(Ok(output)) => {
                    self.command_output = output;
                    self.status = "Command completed".to_owned();
                }
                UiEvent::CommandResult(Err(error)) => {
                    self.command_output = error.clone();
                    self.status = error;
                }
                UiEvent::MonitorResult(Ok(snapshot)) => {
                    self.monitor = Some(snapshot);
                    self.status = "Server metrics updated".to_owned();
                }
                UiEvent::MonitorResult(Err(error)) => self.status = error,
                UiEvent::KeyUpdated(key, Ok(())) => {
                    if self.selected_key.as_deref() == Some(&key) {
                        self.status = format!("Updated {key}");
                        self.load_key(key);
                    }
                }
                UiEvent::KeyUpdated(_, Err(error)) => self.status = error,
                UiEvent::KeyCreated(key, Ok(())) => {
                    self.new_key_name.clear();
                    self.new_key_value.clear();
                    self.new_key_ttl.clear();
                    self.status = format!("Created {key}");
                    self.scan_keys();
                    self.load_key(key);
                }
                UiEvent::KeyCreated(_, Err(error)) => self.status = error,
            }
        }
    }
}

impl eframe::App for ValkeyManagerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.receive_events();
        ui.ctx().request_repaint_after(Duration::from_millis(100));
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.heading("Valkey Manager");
                ui.separator();
                ui.label(&self.status);
            });
            ui.separator();
            ui.columns(2, |columns| {
                columns[0].set_width(300.0);
                columns[0].heading("Connection");
                columns[0].add_space(8.0);
                let selected_name = self
                    .profiles
                    .iter()
                    .find(|profile| Some(profile.id) == self.selected_profile)
                    .map(|profile| profile.name.as_str())
                    .unwrap_or("Unsaved connection");
                let mut selected = self.selected_profile;
                egui::ComboBox::from_label("Saved profile")
                    .selected_text(selected_name)
                    .show_ui(&mut columns[0], |ui| {
                        for profile in &self.profiles {
                            ui.selectable_value(&mut selected, Some(profile.id), &profile.name);
                        }
                    });
                if selected != self.selected_profile {
                    self.select_profile(selected);
                }
                columns[0].label("Profile name");
                columns[0].add(
                    egui::TextEdit::singleline(&mut self.profile_name).desired_width(f32::INFINITY),
                );
                columns[0].label("Redis URL");
                let endpoint_response = columns[0].add(
                    egui::TextEdit::singleline(&mut self.endpoint)
                        .hint_text("redis://127.0.0.1:6379")
                        .desired_width(f32::INFINITY),
                );
                if endpoint_response.changed() && self.connecting {
                    self.status = "Finish or retry the current connection before editing".into();
                }
                columns[0].label("Password (optional)");
                columns[0].add(
                    egui::TextEdit::singleline(&mut self.password)
                        .password(true)
                        .desired_width(f32::INFINITY),
                );
                columns[0].horizontal(|ui| {
                    if ui.button("Save profile").clicked() {
                        self.save_profile();
                    }
                    if ui.button("New").clicked() {
                        self.new_profile();
                    }
                    if ui
                        .add_enabled(self.selected_profile.is_some(), egui::Button::new("Remove"))
                        .clicked()
                    {
                        self.delete_selected_profile();
                    }
                });
                columns[0].add_space(8.0);
                if columns[0]
                    .add_enabled(
                        self.client.is_none() && !self.connecting,
                        egui::Button::new("Connect"),
                    )
                    .clicked()
                {
                    self.connect();
                }
                if self.client.is_some() {
                    columns[0].label("Connection is active");
                    if columns[0].button("Disconnect").clicked() {
                        self.disconnect();
                    }
                }
                columns[0].add_space(16.0);
                columns[0].label("Key pattern");
                columns[0].horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.pattern).desired_width(f32::INFINITY),
                    );
                    if ui
                        .add_enabled(self.client.is_some(), egui::Button::new("Scan"))
                        .clicked()
                    {
                        self.scan_keys();
                    }
                });

                columns[1].horizontal(|ui| {
                    if ui
                        .selectable_label(self.active_tab == WorkspaceTab::Keys, "Keys")
                        .clicked()
                    {
                        self.active_tab = WorkspaceTab::Keys;
                    }
                    if ui
                        .selectable_label(self.active_tab == WorkspaceTab::Console, "Console")
                        .clicked()
                    {
                        self.active_tab = WorkspaceTab::Console;
                    }
                    if ui
                        .selectable_label(self.active_tab == WorkspaceTab::Monitor, "Monitor")
                        .clicked()
                    {
                        self.active_tab = WorkspaceTab::Monitor;
                    }
                });
                columns[1].separator();
                match self.active_tab {
                    WorkspaceTab::Keys => self.render_keys(&mut columns[1]),
                    WorkspaceTab::Console => self.render_console(&mut columns[1]),
                    WorkspaceTab::Monitor => self.render_monitor(&mut columns[1]),
                }
            });
        });
    }
}

pub fn run() -> eframe::Result {
    tracing_subscriber::fmt::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([760.0, 480.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Valkey Manager",
        options,
        Box::new(|_context| Ok(Box::<ValkeyManagerApp>::default())),
    )
}

fn app_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../assets/icons/valkey-manager.png"))
        .expect("embedded Valkey Manager icon is a valid image")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_endpoint_is_a_valid_valkey_url() {
        assert!(RedisConfig::from_url("redis://127.0.0.1:6379").is_ok());
    }

    #[test]
    fn embedded_app_icon_has_rgba_pixels() {
        let icon = app_icon();
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert_eq!((icon.width, icon.height), (128, 128));
    }

    #[test]
    fn console_parser_keeps_quoted_arguments_together() {
        let (command, arguments) = parse_command("set greeting \"hello valkey\"").unwrap();

        assert_eq!(command, "SET");
        assert_eq!(arguments, ["greeting", "hello valkey"]);
    }

    #[test]
    fn console_parser_rejects_empty_or_unclosed_input() {
        assert!(parse_command("   ").is_err());
        assert!(parse_command("set key \"unfinished").is_err());
    }

    #[test]
    fn info_parser_reads_fields_without_confusing_similar_names() {
        let info = "# Server\nredis_version:8.0.1\nredis_git_sha1:abcd\n";

        assert_eq!(info_field(info, "redis_version"), "8.0.1");
        assert_eq!(info_field(info, "version"), "—");
    }
}
