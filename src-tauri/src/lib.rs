use std::{
    sync::mpsc::{self, Receiver, Sender},
    sync::Arc,
    time::Duration,
};

use eframe::egui;
use fred::{prelude::*, types::Scanner};
use futures_util::TryStreamExt;
use uuid::Uuid;

mod profile;
use profile::ConnectionProfile;

enum UiEvent {
    Connected(Result<RedisClient, String>),
    Keys(Result<Vec<String>, String>),
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

    fn scan_keys(&mut self) {
        let Some(client) = self.client.clone() else {
            return;
        };
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
                        keys.extend(
                            results
                                .into_iter()
                                .map(|key| key.as_str_lossy().to_string()),
                        );
                    }
                    page.next().map_err(|error| error.to_string())?;
                }
                Ok(keys)
            }
            .await;
            let _ = sender.send(UiEvent::Keys(result));
        });
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
                    self.status = format!("Connected · {} keys", self.keys.len());
                }
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

                columns[1].heading("Key browser");
                columns[1].separator();
                if self.client.is_none() {
                    columns[1].centered_and_justified(|ui| {
                        ui.label("Connect to a Valkey instance to browse its keys.");
                    });
                } else if self.keys.is_empty() {
                    columns[1].label("No keys found for this pattern.");
                } else {
                    egui::ScrollArea::vertical().show(&mut columns[1], |ui| {
                        for key in &self.keys {
                            ui.horizontal(|ui| {
                                ui.label("◆");
                                ui.monospace(key);
                            });
                            ui.separator();
                        }
                    });
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
            .with_min_inner_size([760.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Valkey Manager",
        options,
        Box::new(|_context| Ok(Box::<ValkeyManagerApp>::default())),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_endpoint_is_a_valid_valkey_url() {
        assert!(RedisConfig::from_url("redis://127.0.0.1:6379").is_ok());
    }
}
