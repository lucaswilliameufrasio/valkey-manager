use fred::{
    prelude::*,
    types::{ClusterHash, CustomCommand, Expiration},
};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires a local Valkey instance; set VALKEY_TEST_URL to run"]
async fn standalone_key_lifecycle_uses_valkey_protocol() {
    let endpoint =
        std::env::var("VALKEY_TEST_URL").unwrap_or_else(|_| "redis://127.0.0.1:16479".to_owned());
    let config = RedisConfig::from_url(&endpoint).expect("valid Valkey test URL");
    let client = RedisClient::new(config, None, None, None);
    drop(client.connect());
    client
        .wait_for_connect()
        .await
        .expect("connect to local Valkey");

    let source = format!("valkey-manager:test:{}", Uuid::new_v4());
    let destination = format!("{source}:renamed");
    let occupied = format!("{source}:occupied");

    client
        .set::<(), _, _>(&source, "initial", None, None, false)
        .await
        .expect("create source key");
    let value: Option<String> = client.get(&source).await.expect("read source key");
    assert_eq!(value.as_deref(), Some("initial"));

    let kind: String = client
        .custom(
            CustomCommand::new("TYPE", ClusterHash::FirstKey, false),
            vec![source.clone()],
        )
        .await
        .expect("read key type");
    assert_eq!(kind, "string");

    client
        .set::<(), _, _>(&source, "with ttl", Some(Expiration::EX(30)), None, false)
        .await
        .expect("update value and TTL");
    let ttl: i64 = client.ttl(&source).await.expect("read key TTL");
    assert!((1..=30).contains(&ttl));

    client
        .set::<(), _, _>(&occupied, "keep", None, None, false)
        .await
        .expect("create occupied destination");
    let renamed: bool = client
        .renamenx(&source, &destination)
        .await
        .expect("rename to free key");
    assert!(renamed);
    let overwritten: bool = client
        .renamenx(&destination, &occupied)
        .await
        .expect("reject rename to occupied key");
    assert!(!overwritten);

    let deleted: i64 = client
        .del(&[destination.as_str(), occupied.as_str()])
        .await
        .expect("delete test keys");
    assert_eq!(deleted, 2);
    client.quit().await.expect("close test connection");
}
