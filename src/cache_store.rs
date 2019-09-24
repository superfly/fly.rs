use futures::{Future, Stream};
use std::io;

use crate::cache_store_notifier::{CacheOperation, CacheStoreNotifierError};

pub type CacheStream = Box<Stream<Item = Vec<u8>, Error = CacheError> + Send>;
pub type EmptyCacheFuture = Box<Future<Item = (), Error = CacheError> + Send>;

pub trait CacheStore {
    fn get(&self, key: String)
        -> Box<Future<Item = Option<CacheEntry>, Error = CacheError> + Send>;

    fn set(
        &self,
        key: String,
        data_stream: Box<Stream<Item = Vec<u8>, Error = ()> + Send>,
        opts: CacheSetOptions,
    ) -> EmptyCacheFuture;

    fn del(&self, key: String) -> EmptyCacheFuture;
    fn expire(&self, key: String, ttl: u32) -> EmptyCacheFuture;
    fn ttl(&self, key: String) -> Box<Future<Item = i32, Error = CacheError> + Send>;
    fn purge_tag(&self, tag: String) -> EmptyCacheFuture;
    fn set_tags(&self, key: String, tags: Vec<String>) -> EmptyCacheFuture;

    fn notify(
        &self,
        op: CacheOperation,
        value: String,
    ) -> Box<Future<Item = (), Error = CacheStoreNotifierError> + Send>;
    fn set_meta(&self, key: String, meta: String) -> EmptyCacheFuture;
}

#[derive(Debug)]
pub enum CacheError {
    Unknown,
    NotFound,
    Failure(String),
    IoErr(io::Error),
}

#[derive(Debug)]
pub struct CacheSetOptions {
    pub ttl: Option<u32>,
    pub tags: Option<Vec<String>>,
    pub meta: Option<String>,
}

pub struct CacheEntry {
    pub meta: Option<String>,
    pub stream: CacheStream,
}

impl From<io::Error> for CacheError {
    #[inline]
    fn from(err: io::Error) -> CacheError {
        CacheError::IoErr(err)
    }
}

pub type CacheResult<T> = Result<T, CacheError>;
