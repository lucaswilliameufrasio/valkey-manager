use fred::{
    prelude::*,
    types::{ClusterHash, CustomCommand, Expiration, SetOptions},
};
use uuid::Uuid;
use valkey_manager_lib::{read_key_value, KeyValue};

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

    let ping: String = client.ping().await.expect("ping local Valkey");
    assert_eq!(ping, "PONG");
    let info: String = client.info(None).await.expect("read server information");
    assert!(info.contains("redis_version:") || info.contains("valkey_version:"));
    let _database_size: i64 = client.dbsize().await.expect("read database key count");
    let console_ping: String = client
        .custom(
            CustomCommand::new("PING", ClusterHash::FirstKey, false),
            Vec::<String>::new(),
        )
        .await
        .expect("execute a console command");
    assert_eq!(console_ping, "PONG");

    let source = format!("valkey-manager:test:{}", Uuid::new_v4());
    let destination = format!("{source}:renamed");
    let occupied = format!("{source}:occupied");
    let created = format!("{source}:created");
    let list = format!("{source}:list");
    let hash = format!("{source}:hash");
    let set = format!("{source}:set");
    let sorted_set = format!("{source}:zset");

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

    let created_once: Option<String> = client
        .set(&created, "created value", None, Some(SetOptions::NX), false)
        .await
        .expect("create key without replacing existing data");
    assert_eq!(created_once.as_deref(), Some("OK"));
    let created_again: Option<String> = client
        .set(
            &created,
            "replacement value",
            None,
            Some(SetOptions::NX),
            false,
        )
        .await
        .expect("reject duplicate key creation");
    assert_eq!(created_again, None);

    client
        .rpush::<i64, _, _>(&list, vec!["first", "second"])
        .await
        .expect("create list value");
    client
        .custom::<i64, _>(
            CustomCommand::new("HSET", ClusterHash::FirstKey, false),
            vec![hash.clone(), "field".to_owned(), "value".to_owned()],
        )
        .await
        .expect("create hash value");
    client
        .sadd::<i64, _, _>(&set, vec!["member"])
        .await
        .expect("create set value");
    client
        .zadd::<i64, _, _>(
            &sorted_set,
            None,
            None,
            false,
            false,
            vec![(2.5, "scored-member")],
        )
        .await
        .expect("create sorted-set value");
    client
        .custom::<RedisValue, _>(
            CustomCommand::new("RPUSH", ClusterHash::FirstKey, false),
            vec![list.clone(), "third".to_owned()],
        )
        .await
        .expect("append to list");
    client
        .custom::<RedisValue, _>(
            CustomCommand::new("HSET", ClusterHash::FirstKey, false),
            vec![
                hash.clone(),
                "second-field".to_owned(),
                "second-value".to_owned(),
            ],
        )
        .await
        .expect("add hash field");
    client
        .custom::<RedisValue, _>(
            CustomCommand::new("SADD", ClusterHash::FirstKey, false),
            vec![set.clone(), "second-member".to_owned()],
        )
        .await
        .expect("add set member");
    client
        .custom::<RedisValue, _>(
            CustomCommand::new("ZADD", ClusterHash::FirstKey, false),
            vec![
                sorted_set.clone(),
                "3.0".to_owned(),
                "second-score".to_owned(),
            ],
        )
        .await
        .expect("add sorted-set member");

    assert_eq!(
        read_key_value(&client, &list, "list").await.unwrap(),
        KeyValue::List(vec![
            "first".to_owned(),
            "second".to_owned(),
            "third".to_owned()
        ])
    );
    let hash_value = read_key_value(&client, &hash, "hash").await.unwrap();
    assert!(
        matches!(hash_value, KeyValue::Hash(fields) if fields.len() == 2 && fields.contains(&("field".to_owned(), "value".to_owned())))
    );
    let set_value = read_key_value(&client, &set, "set").await.unwrap();
    assert!(
        matches!(set_value, KeyValue::Set(members) if members.len() == 2 && members.contains(&"member".to_owned()))
    );
    let zset_value = read_key_value(&client, &sorted_set, "zset").await.unwrap();
    assert!(
        matches!(zset_value, KeyValue::SortedSet(members) if members.len() == 2 && members.contains(&("scored-member".to_owned(), 2.5)))
    );

    let list_removed: i64 = client
        .custom(
            CustomCommand::new("LREM", ClusterHash::FirstKey, false),
            vec![list.clone(), "1".to_owned(), "first".to_owned()],
        )
        .await
        .expect("remove list entry");
    let hash_removed: i64 = client
        .custom(
            CustomCommand::new("HDEL", ClusterHash::FirstKey, false),
            vec![hash.clone(), "field".to_owned()],
        )
        .await
        .expect("remove hash field");
    let set_removed: i64 = client
        .custom(
            CustomCommand::new("SREM", ClusterHash::FirstKey, false),
            vec![set.clone(), "member".to_owned()],
        )
        .await
        .expect("remove set member");
    let sorted_set_removed: i64 = client
        .custom(
            CustomCommand::new("ZREM", ClusterHash::FirstKey, false),
            vec![sorted_set.clone(), "scored-member".to_owned()],
        )
        .await
        .expect("remove sorted-set member");
    assert_eq!(
        [list_removed, hash_removed, set_removed, sorted_set_removed],
        [1, 1, 1, 1]
    );

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
        .del(&[
            destination.as_str(),
            occupied.as_str(),
            created.as_str(),
            list.as_str(),
            hash.as_str(),
            set.as_str(),
            sorted_set.as_str(),
        ])
        .await
        .expect("delete test keys");
    assert_eq!(deleted, 7);
    client.quit().await.expect("close test connection");
}
