use super::REDIS_POOL;
use r2d2_redis::redis;

pub fn fetch_libs<'a>(keys: &'a [String]) -> Result<Vec<(&'a String, Option<String>)>, String> {
    let conn = match REDIS_POOL.get() {
        Ok(c) => c,
        Err(e) => return Err(format!("error getting pool connection: {}", e)),
    };

    fetch_libs_internal(&conn, keys)
}

fn fetch_libs_internal<'a>(
    conn: &redis::Connection,
    keys: &'a [String],
) -> Result<Vec<(&'a String, Option<String>)>, String> {
    let libs: Vec<Option<String>> = match redis::cmd("HMGET")
        .arg("libs")
        .arg(keys)
        .query::<Vec<Option<String>>>(&*conn)
    {
        Ok(result) => result,
        Err(e) => return Err(format!("error getting libs: {}", e)),
    };

    Ok(keys.iter().zip(libs.into_iter()).collect())
}

#[test]
fn test_fetch_libs() {
    let conn = redis::Client::open("redis://127.0.0.1/")
        .unwrap()
        .get_connection()
        .unwrap();

    redis::cmd("HMSET")
        .arg("libs")
        .arg(&["a", "a-source"])
        .arg(&["c", "c-source"])
        .execute(&conn);

    let keys = ["a".to_owned(), "b".to_owned(), "c".to_owned()];

    let results = fetch_libs_internal(&conn, &keys).unwrap();
    assert_eq!(3, results.len());
    assert_eq!("a", results[0].0);
    assert_eq!(Some("a-source".to_owned()), results[0].1);

    assert_eq!("b", results[1].0);
    assert_eq!(None, results[1].1);
    assert_eq!("c", results[2].0);
    assert_eq!(Some("c-source".to_owned()), results[2].1);
}
