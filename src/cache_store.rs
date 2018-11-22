use futures::{Future, Stream};
use std::io;

pub trait CacheStore {
  fn get(
    &self,
    key: String,
  ) -> CacheResult<Option<Box<Stream<Item = Vec<u8>, Error = CacheError> + Send>>>;

  fn set(
    &self,
    key: String,
    data_stream: Box<Stream<Item = Vec<u8>, Error = ()> + Send>,
    maybe_ttl: Option<u32>,
  ) -> Box<Future<Item = (), Error = CacheError> + Send>;

  fn del(&self, key: String) -> Box<Future<Item = (), Error = CacheError> + Send>;
  fn expire(&self, key: String, ttl: u32) -> Box<Future<Item = (), Error = CacheError> + Send>;
}

#[derive(Debug)]
pub enum CacheError {
  Unknown,
  NotFound,
  Failure(String),
  IoErr(io::Error),
}

impl From<io::Error> for CacheError {
  #[inline]
  fn from(err: io::Error) -> CacheError {
    CacheError::IoErr(err)
  }
}

pub type CacheResult<T> = Result<T, CacheError>;
