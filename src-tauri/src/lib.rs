// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::Result;
use fred::types::RedisConfig;
use futures_util::future::FutureExt;
use futures_util::stream::StreamExt;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf, time::Duration};
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

// ----- Valkey client (fred) -----
use fred::prelude::*;

static REGISTRY: Lazy<RwLock<Registry>> = Lazy::new(|| RwLock::new(Registry::default()));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Topology {
    STANDALONE,
    CLUSTER,
    SENTINEL,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstanceCfg {
    id: Uuid,
    name: String,
    topology: Topology,
    use_tls: bool,
    username: Option<String>,
    password: Option<String>,
    db: Option<u8>,
    // standalone
    host: Option<String>,
    port: Option<u16>,
    // cluster
    cluster_nodes: Vec<String>, // host:port
    // sentinel
    sentinel_master_id: Option<String>,
    sentinel_nodes: Vec<String>, // host:port
    timeout_ms: Option<u64>,
}

impl Default for InstanceCfg {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "Valkey".into(),
            topology: Topology::STANDALONE,
            use_tls: false,
            username: None,
            password: None,
            db: Some(0),
            host: Some("127.0.0.1".into()),
            port: Some(6379),
            cluster_nodes: vec![],
            sentinel_master_id: None,
            sentinel_nodes: vec![],
            timeout_ms: Some(5000),
        }
    }
}

#[derive(Default)]
struct Registry {
    configs: Vec<InstanceCfg>,
    clients: HashMap<Uuid, RedisClient>,
    // pubsub subscriptions: sub_id -> (instance_id, pattern?)
    subs: HashMap<Uuid, (Uuid, bool, String)>,
}

fn config_dir(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir().expect("app dir")
}

fn cfg_path(app: &AppHandle) -> PathBuf {
    config_dir(app).join("instances.json")
}

fn save_configs(app: &AppHandle) -> Result<()> {
    let reg = REGISTRY.read();
    fs::create_dir_all(config_dir(app))?;
    fs::write(cfg_path(app), serde_json::to_vec_pretty(&reg.configs)?)?;
    Ok(())
}

fn load_configs(app: &AppHandle) -> Result<Vec<InstanceCfg>> {
    let p = cfg_path(app);
    if !p.exists() {
        return Ok(vec![]);
    }
    Ok(serde_json::from_slice(&fs::read(p)?)?)
}

/// Build a fred::RedisConfig from our InstanceCfg
fn build_redis_config(cfg: &InstanceCfg) -> Result<RedisConfig> {
    let mut config = match &cfg.topology {
        Topology::STANDALONE => {
            let host = cfg.host.as_deref().unwrap_or("127.0.0.1");
            let port = cfg.port.unwrap_or(6379);
            let url = format!(
                "redis{}://{}:{}",
                if cfg.use_tls { "s" } else { "" },
                host,
                port
            );
            RedisConfig::from_url(&url)?
        }
        Topology::CLUSTER => {
            let url = format!(
                "redis{}://{}",
                if cfg.use_tls { "s" } else { "" },
                cfg.cluster_nodes.join(",")
            );
            RedisConfig::from_url_clustered(&url)?
        }
        Topology::SENTINEL => {
            let service_name = cfg.sentinel_master_id.as_deref().unwrap_or("mymaster");
            let url = format!(
                "redis-sentinel{}://{}/{}?sentinel_password={}",
                if cfg.use_tls { "s" } else { "" },
                cfg.sentinel_nodes.join(","),
                service_name,
                cfg.password.as_deref().unwrap_or_default()
            );
            RedisConfig::from_url_sentinel(&url)?
        }
    };

    config.username = cfg.username.clone();
    config.password = cfg.password.clone();
    config.database = cfg.db;
    Ok(config)
}

async fn ensure_connected(id: Uuid) -> Result<()> {
    let mut reg = REGISTRY.write();
    if reg.clients.contains_key(&id) {
        return Ok(());
    }
    let cfg = reg
        .configs
        .iter()
        .find(|c| c.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("config not found"))?;
    let config = build_redis_config(&cfg)?;
    let client = RedisClient::new(config, None, None, None);
    let timeout = Duration::from_millis(cfg.timeout_ms.unwrap_or(5000));
    client.connect();
    tokio::time::timeout(timeout, client.wait_for_connect()).await??;
    reg.clients.insert(id, client);
    Ok(())
}

fn get_client(id: Uuid) -> Result<RedisClient> {
    let reg = REGISTRY.read();
    reg.clients
        .get(&id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("not connected"))
}

// ---------- Tauri commands (configs) ----------
#[tauri::command]
fn list_instances(app: AppHandle) -> Result<Vec<InstanceCfg>, String> {
    let mut reg = REGISTRY.write();
    if reg.configs.is_empty() {
        if let Ok(cfgs) = load_configs(&app) {
            reg.configs = cfgs;
        }
    }
    Ok(reg.configs.clone())
}

#[tauri::command]
fn save_instance(app: AppHandle, cfg: InstanceCfg) -> Result<(), String> {
    let mut reg = REGISTRY.write();
    if let Some(i) = reg.configs.iter_mut().position(|c| c.id == cfg.id) {
        reg.configs[i] = cfg;
    } else {
        reg.configs.push(cfg);
    }
    save_configs(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_instance(app: AppHandle, id: Uuid) -> Result<(), String> {
    {
        let mut reg = REGISTRY.write();
        reg.configs.retain(|c| c.id != id);
        if let Some(c) = reg.clients.remove(&id) {
            tokio::spawn(async move {
                let _ = c.quit().await;
            });
        }
    }
    save_configs(&app).map_err(|e| e.to_string())
}

// ---------- Tauri commands (connectivity) ----------
#[tauri::command]
async fn connect_instance(id: Uuid) -> Result<(), String> {
    ensure_connected(id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn disconnect_instance(id: Uuid) -> Result<(), String> {
    let mut reg = REGISTRY.write();
    if let Some(c) = reg.clients.remove(&id) {
        c.quit().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------- Key browser / inspectors ----------
#[tauri::command]
async fn scan_keys(
    id: Uuid,
    pattern: String,
    cursor: Option<u64>,
    count: Option<u32>,
) -> Result<(u64, Vec<String>), String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    let (next, keys): (u64, Vec<String>) = client
        .scan(cursor.unwrap_or(0), Some(pattern), count.map(|c| c as u64))
        .await
        .map_err(|e| e.to_string())?;
    Ok((next, keys))
}

#[tauri::command]
async fn key_type(id: Uuid, key: String) -> Result<String, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    let t: String = client.r#type(&key).await.map_err(|e| e.to_string())?;
    Ok(t)
}

#[tauri::command]
async fn get_ttl(id: Uuid, key: String) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    client.ttl(&key).await.map_err(|e| e.to_string())
}
#[tauri::command]
async fn set_ttl(id: Uuid, key: String, ttl_seconds: Option<i64>) -> Result<bool, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    let ok = if let Some(s) = ttl_seconds {
        client.expire(&key, s).await.map_err(|e| e.to_string())?
    } else {
        client.persist(&key).await.map_err(|e| e.to_string())?
    };
    Ok(ok)
}
#[tauri::command]
async fn rename_key(id: Uuid, old_key: String, new_key: String) -> Result<bool, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    client
        .rename(&old_key, &new_key)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn del_key(id: Uuid, key: String) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    client.del(&[key]).await.map_err(|e| e.to_string())
}

// STRING
#[tauri::command]
async fn get_string(id: Uuid, key: String) -> Result<Option<String>, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    client.get(&key).await.map_err(|e| e.to_string())
}
#[tauri::command]
async fn set_string(
    id: Uuid,
    key: String,
    value: String,
    ttl_seconds: Option<i64>,
) -> Result<(), String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    if let Some(s) = ttl_seconds {
        client
            .setex(&key, value, s)
            .await
            .map_err(|e| e.to_string())?;
    } else {
        client
            .set(&key, value, None, None, false)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// LIST
#[tauri::command]
async fn list_range(id: Uuid, key: String, start: i64, stop: i64) -> Result<Vec<String>, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    client
        .lrange(&key, start, stop)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn list_push(id: Uuid, key: String, left: bool, values: Vec<String>) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    if left {
        client.lpush(&key, values).await
    } else {
        client.rpush(&key, values).await
    }
    .map_err(|e| e.to_string())
}
#[tauri::command]
async fn list_remove(id: Uuid, key: String, value: String, count: i64) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    client
        .lrem(&key, count, &value)
        .await
        .map_err(|e| e.to_string())
}

// SET
#[tauri::command]
async fn set_members(id: Uuid, key: String, limit: u32) -> Result<Vec<String>, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    // SSCAN would be better for very large sets; here we use SSCAN with cursor.
    let mut cur = 0;
    let mut out = vec![];
    loop {
        let (next, vals): (u64, Vec<String>) = client
            .sscan(&key, cur, Some(limit))
            .await
            .map_err(|e| e.to_string())?;
        out.extend(vals);
        if next == 0 || (out.len() as u32) >= limit {
            break;
        }
        cur = next;
    }
    Ok(out)
}
#[tauri::command]
async fn sadd(id: Uuid, key: String, members: Vec<String>) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    client.sadd(&key, members).await.map_err(|e| e.to_string())
}
#[tauri::command]
async fn srem(id: Uuid, key: String, members: Vec<String>) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    client.srem(&key, members).await.map_err(|e| e.to_string())
}

// ZSET
#[tauri::command]
async fn zrange_withscores(
    id: Uuid,
    key: String,
    start: i64,
    stop: i64,
) -> Result<Vec<(String, f64)>, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    client
        .zrange_withscores(&key, start, stop)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn zadd(id: Uuid, key: String, member: String, score: f64) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    get_client(id)
        .map_err(|e| e.to_string())?
        .zadd(&key, None, None, false, false, vec![(score, member)])
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn zrem(id: Uuid, key: String, member: String) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    get_client(id)
        .map_err(|e| e.to_string())?
        .zrem(&key, &[member])
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn zincrby(id: Uuid, key: String, member: String, by: f64) -> Result<f64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    get_client(id)
        .map_err(|e| e.to_string())?
        .zincrby(&key, by, &member)
        .await
        .map_err(|e| e.to_string())
}

// HASH
#[tauri::command]
async fn hgetall(id: Uuid, key: String) -> Result<Vec<(String, String)>, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let m: Vec<String> = get_client(id)
        .map_err(|e| e.to_string())?
        .hgetall(&key)
        .await
        .map_err(|e| e.to_string())?;
    Ok(m.chunks(2)
        .filter_map(|c| {
            if c.len() == 2 {
                Some((c[0].clone(), c[1].clone()))
            } else {
                None
            }
        })
        .collect())
}
#[tauri::command]
async fn hset(id: Uuid, key: String, field: String, value: String) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    get_client(id)
        .map_err(|e| e.to_string())?
        .hset(&key, &[(field, value)])
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn hdel(id: Uuid, key: String, field: String) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    get_client(id)
        .map_err(|e| e.to_string())?
        .hdel(&key, &[field])
        .await
        .map_err(|e| e.to_string())
}

// STREAM (quick XADD)
#[tauri::command]
async fn xadd(id: Uuid, key: String, fields: Vec<(String, String)>) -> Result<String, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    get_client(id)
        .map_err(|e| e.to_string())?
        .xadd(&key, false, None, None, &fields)
        .await
        .map_err(|e| e.to_string())
}

// PUB/SUB
#[derive(Serialize, Clone)]
struct PubSubMsg {
    channel: String,
    message: String,
}

#[tauri::command]
async fn subscribe(app: AppHandle, id: Uuid, channel: String) -> Result<Uuid, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    let sub_id = Uuid::new_v4();
    let app_clone = app.clone();
    tokio::spawn(async move {
        let mut sub = client.clone();
        if sub.subscribe(&[channel.clone()]).await.is_ok() {
            while let Some(Ok(msg)) = sub.next().await {
                if let Some(ch) = msg.channel {
                    if let Some(s) = msg.value.as_string() {
                        let _ = app_clone.emit(
                            "pubsub://message",
                            PubSubMsg {
                                channel: ch,
                                message: s.to_string(),
                            },
                        );
                    }
                }
            }
        }
    });
    REGISTRY.write().subs.insert(sub_id, (id, false, channel));
    Ok(sub_id)
}

#[tauri::command]
async fn psubscribe(app: AppHandle, id: Uuid, pattern: String) -> Result<Uuid, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    let client = get_client(id).map_err(|e| e.to_string())?;
    let sub_id = Uuid::new_v4();
    let app_clone = app.clone();
    tokio::spawn(async move {
        let mut sub = client.clone();
        if sub.psubscribe(&[pattern.clone()]).await.is_ok() {
            while let Some(Ok(msg)) = sub.next().await {
                if let Some(ch) = msg.channel {
                    if let Some(s) = msg.value.as_string() {
                        let _ = app_clone.emit(
                            "pubsub://message",
                            PubSubMsg {
                                channel: ch,
                                message: s.to_string(),
                            },
                        );
                    }
                }
            }
        }
    });
    REGISTRY.write().subs.insert(sub_id, (id, true, pattern));
    Ok(sub_id)
}

#[tauri::command]
async fn unsubscribe(id: Uuid, sub_id: Uuid) -> Result<(), String> {
    let (inst, is_pat, name) = REGISTRY
        .write()
        .subs
        .remove(&sub_id)
        .ok_or_else(|| "sub not found".to_string())?;
    ensure_connected(inst).await.map_err(|e| e.to_string())?;
    let client = get_client(inst).map_err(|e| e.to_string())?;
    if is_pat {
        client.punsubscribe(&[name]).await
    } else {
        client.unsubscribe(&[name]).await
    }
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn publish(id: Uuid, channel: String, message: String) -> Result<i64, String> {
    ensure_connected(id).await.map_err(|e| e.to_string())?;
    get_client(id)
        .map_err(|e| e.to_string())?
        .publish(&channel, message)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn new_instance() -> InstanceCfg {
    InstanceCfg::default()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            list_instances,
            save_instance,
            remove_instance,
            connect_instance,
            disconnect_instance,
            scan_keys,
            key_type,
            get_ttl,
            set_ttl,
            rename_key,
            del_key,
            get_string,
            set_string,
            list_range,
            list_push,
            list_remove,
            set_members,
            sadd,
            srem,
            zrange_withscores,
            zadd,
            zrem,
            zincrby,
            hgetall,
            hset,
            hdel,
            xadd,
            subscribe,
            psubscribe,
            unsubscribe,
            publish,
            new_instance
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
