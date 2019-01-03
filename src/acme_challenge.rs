use futures::{Future};

pub trait AcmeChallengeStore {
  fn check_token(
    &self,
    hostname: String,
    token: String,
  ) -> Box<Future<Item = bool, Error = AcmeChallengeError> + Send>;
}

#[derive(Debug, PartialEq)]
pub enum AcmeChallengeError {
  Unknown,
  Failure(String),
}
