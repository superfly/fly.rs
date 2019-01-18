use futures::Future;

pub trait DataStore {
  fn get(
    &self,
    coll: String,
    key: String,
  ) -> Box<Future<Item = Option<String>, Error = DataError> + Send>;
  fn del(&self, coll: String, key: String) -> Box<Future<Item = (), Error = DataError> + Send>;
  fn put(
    &self,
    coll: String,
    key: String,
    data: String,
  ) -> Box<Future<Item = (), Error = DataError> + Send>;
  fn incr(
    &self,
    coll: String,
    key: String,
    field: String,
    amount: i32,
  ) -> Box<Future<Item = (), Error = DataError> + Send>;
  fn drop_coll(&self, coll: String) -> Box<Future<Item = (), Error = DataError> + Send>;
}

#[derive(Debug, PartialEq)]
pub enum DataError {
  Unknown,
  Failure(String),
}

pub type DataResult<T> = Result<T, DataError>;
