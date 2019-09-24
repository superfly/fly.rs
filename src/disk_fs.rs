use crate::fs_store::*;
use futures::{future, Future, Stream};

use crate::runtime::EVENT_LOOP;

use std::io;
use tokio::codec::{BytesCodec, FramedRead};

use futures::sync::{mpsc, oneshot};

pub struct DiskFsStore {}

impl DiskFsStore {
    pub fn new() -> Self {
        DiskFsStore {}
    }
}

impl FsStore for DiskFsStore {
    fn read(&self, path: String) -> Box<Future<Item = Option<FsEntry>, Error = FsError> + Send> {
        let (tx, rx) = oneshot::channel::<Result<Option<FsStream>, FsError>>();

        EVENT_LOOP.0.spawn(future::lazy(move || {
            tokio::fs::File::open(path).then(move |fileres| {
                if let Err(e) = fileres {
                    if e.kind() == io::ErrorKind::NotFound {
                        if let Err(_) = tx.send(Ok(None)) {
                            error!("unknown error sending into channel");
                        }
                        return Ok(());
                    }
                    if let Err(_) = tx.send(Err(e.into())) {
                        error!("unknown error sending into channel");
                    }

                    return Err(());
                }

                let file = fileres.unwrap();
                match file.metadata().wait() {
                    Err(e) => {
                        if let Err(_) = tx.send(Err(e.into())) {
                            error!("unknown error sending into channel");
                        }
                        return Err(());
                    }
                    Ok((file, meta)) => {
                        if !meta.is_file() {
                            if let Err(_) = tx.send(Ok(None)) {
                                error!("unknown error sending into channel");
                            }
                            return Ok(());
                        }
                        let (btx, brx) = mpsc::unbounded::<Result<Vec<u8>, FsError>>();
                        let btxerr = btx.clone();
                        EVENT_LOOP.0.spawn(
                            FramedRead::new(file, BytesCodec::new())
                                .map_err(move |e| {
                                    if let Err(e) = btxerr.clone().unbounded_send(Err(e.into())) {
                                        error!("error sending into channel: {}", e);
                                    }
                                })
                                .for_each(move |chunk| {
                                    if let Err(e) = btx.clone().unbounded_send(Ok(chunk.to_vec())) {
                                        error!("error sending into channel: {}", e);
                                    }
                                    Ok(())
                                })
                                .and_then(|_| Ok(())),
                        );

                        if let Err(_) = tx.send(Ok(Some(Box::new(
                            brx.map_err(|_| {
                                FsError::Failure("error receiving fs chunk".to_string())
                            })
                            .and_then(|chunk_res| match chunk_res {
                                Ok(c) => Ok(c),
                                Err(e) => Err(e),
                            }),
                        )))) {
                            error!("unknown error sending stream info channel");
                        }
                        Ok(())
                    }
                }
            })
        }));

        Box::new(
            rx.map_err(|_| FsError::Failure("error receiving fs response".to_string()))
                .and_then(move |res| match res {
                    Err(e) => Err(e),
                    Ok(maybe_stream) => match maybe_stream {
                        Some(stream) => Ok(Some(FsEntry { stream })),
                        None => Ok(None),
                    },
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_disk_fs_read() {
        let store = DiskFsStore::new();
        let path = "README.md";

        assert_eq!(
            store
                .read(path.to_string())
                .wait()
                .unwrap()
                .unwrap()
                .stream
                .concat2()
                .wait()
                .unwrap(),
            fs::read(path).unwrap()
        );

        assert!(store.read("notfound".to_string()).wait().unwrap().is_none());
    }
}
