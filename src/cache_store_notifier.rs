use futures::Future;

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CacheOperation {
    Del,
    PurgeTag,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CacheNotifyMessage {
    pub op: CacheOperation,
    pub ns: String,
    pub value: String,
}

#[derive(Debug)]
pub enum CacheStoreNotifierError {
    Unknown,
    Failure(String),
    Unavailable,
}

pub trait CacheStoreNotifier {
    fn notify(
        &self,
        op: CacheOperation,
        ns: String,
        value: String,
    ) -> Box<Future<Item = (), Error = CacheStoreNotifierError> + Send>;
}
