use futures::{Future, Stream};
use std::io;

pub type FsStream = Box<Stream<Item = Vec<u8>, Error = FsError> + Send>;

pub trait FsStore {
    fn read(&self, path: String) -> Box<Future<Item = Option<FsEntry>, Error = FsError> + Send>;
}

pub struct FsEntry {
    pub stream: FsStream,
}

#[derive(Debug)]
pub enum FsError {
    Unknown,
    NotFound,
    Failure(String),
    IoErr(io::Error),
}

impl From<io::Error> for FsError {
    #[inline]
    fn from(err: io::Error) -> FsError {
        FsError::IoErr(err)
    }
}

pub type FsResult<T> = Result<T, FsError>;
