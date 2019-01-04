use futures::Future;

pub trait AcmeStore {
  fn validate_challenge(
    &self,
    hostname: String,
    token: String,
  ) -> Box<Future<Item = Option<String>, Error = AcmeError> + Send>;
}

#[derive(Debug, PartialEq)]
pub enum AcmeError {
  Unknown,
  Failure(String),
}
