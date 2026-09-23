use std::{
    sync::mpsc::{self, Receiver, Sender},
    sync::Arc,
    time::Duration,
};

use eframe::egui;
use fred::{prelude::*, types::Scanner};
use futures_util::TryStreamExt;

enum UiEvent {
    Connected(Result<RedisClient, String>),
    Keys(Result<Vec<String>, String>),
}

struct ValkeyManagerApp {
    endpoint: String,
    pattern: String,
    status: String,
    connecting: bool,
    client: Option<RedisClient>,
    runtime: Arc<tokio::runtime::Runtime>,
    keys: Vec<String>,
    sender: Sender<UiEvent>,
    receiver: Receiver<UiEvent>,
}

impl Default for ValkeyManagerApp {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            endpoint: "redis://127.0.0.1:6379".to_owned(),
            pattern: "*".to_owned(),
            status: "Not connected".to_owned(),
            connecting: false,
            client: None,
            runtime: Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("create Tokio runtime"),
            ),
            keys: Vec::new(),
            sender,
            receiver,
        }
    }
}

impl ValkeyManagerApp {
    fn connect(&mut self) {
        let endpoint = self.endpoint.trim().to_owned();
        let sender = self.sender.clone();
        self.status = "Connecting…".to_owned();
        self.connecting = true;
        self.client = None;

        self.runtime.spawn(async move {
            let result = async {
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
                columns[0].label("Redis URL");
                columns[0].add(
                    egui::TextEdit::singleline(&mut self.endpoint)
                        .hint_text("redis://127.0.0.1:6379")
                        .desired_width(f32::INFINITY),
                );
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
