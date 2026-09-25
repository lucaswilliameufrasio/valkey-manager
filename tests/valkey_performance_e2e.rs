use std::time::{Duration, Instant};

use fred::{prelude::*, types::Expiration};
use futures_util::future::join_all;
use uuid::Uuid;
use valkey_manager_lib::{read_key_value, scan_key_names, KeyValue};

const PERF_BUDGET: Duration = Duration::from_secs(2);
const READ_BUDGET: Duration = Duration::from_millis(250);

async fn connect_to_valkey() -> RedisClient {
    let endpoint =
        std::env::var("VALKEY_TEST_URL").unwrap_or_else(|_| "redis://127.0.0.1:16479".to_owned());
    let config = RedisConfig::from_url(&endpoint).expect("valid local Valkey test URL");
    let client = RedisClient::new(config, None, None, None);
    drop(client.connect());
    tokio::time::timeout(Duration::from_secs(8), client.wait_for_connect())
        .await
        .expect("Valkey connection should become ready")
        .expect("connect to local Valkey");
    client
}

async fn seed_string_keys(client: &RedisClient, prefix: &str, count: usize) -> Vec<String> {
    let keys = (0..count)
        .map(|index| format!("{prefix}{index:05}"))
        .collect::<Vec<_>>();

    for batch in keys.chunks(64) {
        let writes = batch.iter().map(|key| {
            client.set::<(), _, _>(
                key,
                format!("payload:{key}"),
                Some(Expiration::EX(300)),
                None,
                false,
            )
        });
        for result in join_all(writes).await {
            result.expect("seed performance fixture key");
        }
    }

    keys
}

async fn delete_fixture_keys(client: &RedisClient, keys: &[String]) {
    for batch in keys.chunks(128) {
        let references = batch.iter().map(String::as_str).collect::<Vec<_>>();
        client
            .del::<i64, _>(references)
            .await
            .expect("remove only this test's namespaced keys");
    }
}

fn percentile(samples: &[Duration], percentile: usize) -> Duration {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let index = (sorted.len() * percentile).div_ceil(100).saturating_sub(1);
    sorted[index.min(sorted.len() - 1)]
}

#[tokio::test]
#[ignore = "requires the pinned Valkey 9 service; run scripts/test-e2e-performance.sh"]
async fn keyspace_scan_stays_bounded_and_within_latency_budget() {
    let client = connect_to_valkey().await;
    let prefix = format!("valkey-manager:e2e:{}:", Uuid::new_v4());
    let keys = seed_string_keys(&client, &prefix, 2_000).await;
    let pattern = format!("{prefix}*");
    let mut scan_times = Vec::with_capacity(5);

    for _ in 0..5 {
        let started = Instant::now();
        let found = scan_key_names(&client, &pattern)
            .await
            .expect("scan the real Valkey keyspace");
        scan_times.push(started.elapsed());

        assert_eq!(found.len(), 500, "the UI scan limit is 500 keys");
        assert!(found.iter().all(|key| key.starts_with(&prefix)));
    }

    let p95 = percentile(&scan_times, 95);
    eprintln!(
        "E2E key scan: 2,000 matching keys, 500 returned, p50={:?}, p95={p95:?}",
        percentile(&scan_times, 50)
    );
    assert!(
        p95 < PERF_BUDGET,
        "key scan p95 {p95:?} exceeded {PERF_BUDGET:?}"
    );

    delete_fixture_keys(&client, &keys).await;
    client.quit().await.expect("close Valkey test connection");
}

#[tokio::test]
#[ignore = "requires the pinned Valkey 9 service; run scripts/test-e2e-performance.sh"]
async fn selected_string_inspection_stays_within_latency_budget() {
    let client = connect_to_valkey().await;
    let prefix = format!("valkey-manager:e2e:{}:", Uuid::new_v4());
    let keys = seed_string_keys(&client, &prefix, 128).await;
    let mut read_times = Vec::with_capacity(100);

    for key in keys.iter().take(100) {
        let started = Instant::now();
        let value = read_key_value(&client, key, "string")
            .await
            .expect("inspect a selected key through the app's value reader");
        read_times.push(started.elapsed());
        assert_eq!(value, KeyValue::String(format!("payload:{key}")));
    }

    let p95 = percentile(&read_times, 95);
    eprintln!(
        "E2E selected-key reads: 100 string keys, p50={:?}, p95={p95:?}",
        percentile(&read_times, 50)
    );
    assert!(
        p95 < READ_BUDGET,
        "key read p95 {p95:?} exceeded {READ_BUDGET:?}"
    );

    delete_fixture_keys(&client, &keys).await;
    client.quit().await.expect("close Valkey test connection");
}
