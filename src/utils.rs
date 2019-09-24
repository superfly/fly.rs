use crate::errors::FlyError;
use crate::js::*;
use crate::msg;
use crate::runtime::{JsRuntime, Runtime};
use flatbuffers::FlatBufferBuilder;
use futures::{
    future,
    sync::{mpsc, oneshot},
    Future, Stream,
};
use libfly::*;
use std::ptr;
use tokio_signal::unix::{Signal, SIGINT, SIGTERM};

// Buf represents a byte array returned from a "Op".
// The message might be empty (which will be translated into a null object on
// the javascript side) or it is a heap allocated opaque sequence of bytes.
// Usually a flatbuffer message.
pub type Buf = Option<Box<[u8]>>;

// JS promises in Fly map onto a specific Future
// which yields either a FlyError or a byte array.
pub type Op = Future<Item = Buf, Error = FlyError> + Send;
pub type Handler = fn(&mut Runtime, &msg::Base, fly_buf) -> Box<Op>;

pub fn take_last_n(str: &str, n: usize) -> Option<&str> {
    if str.len() >= n {
        Some(&str[str.len() - n..])
    } else {
        None
    }
}

pub fn serialize_response(
    cmd_id: u32,
    builder: &mut FlatBufferBuilder,
    mut args: msg::BaseArgs,
) -> Buf {
    args.cmd_id = cmd_id;
    let base = msg::Base::create(builder, &args);
    msg::finish_base_buffer(builder, base);
    let data = builder.finished_data();
    let vec = data.to_vec();
    Some(vec.into_boxed_slice())
}

pub fn build_error(cmd_id: u32, err: FlyError) -> Buf {
    let builder = &mut FlatBufferBuilder::new();
    let errmsg_offset = builder.create_string(&format!("{}", err));
    serialize_response(
        cmd_id,
        builder,
        msg::BaseArgs {
            error: Some(errmsg_offset),
            error_kind: err.kind(),
            ..Default::default()
        },
    )
}

pub fn null_buf() -> fly_buf {
    fly_buf {
        alloc_ptr: 0 as *mut u8,
        alloc_len: 0,
        data_ptr: 0 as *mut u8,
        data_len: 0,
    }
}

pub fn ok_future(buf: Buf) -> Box<Op> {
    Box::new(future::ok(buf))
}

pub fn odd_future(err: FlyError) -> Box<Op> {
    Box::new(future::err(err))
}

pub fn fly_buf_from(x: Box<[u8]>) -> fly_buf {
    let len = x.len();
    let ptr = Box::into_raw(x);
    fly_buf {
        alloc_ptr: 0 as *mut u8,
        alloc_len: 0,
        data_ptr: ptr as *mut u8,
        data_len: len,
    }
}

pub fn send_body_stream(ptr: JsRuntime, req_id: u32, stream: JsBody) {
    let rt = ptr.to_runtime();

    match stream {
        JsBody::BoxedStream(s) => rt.spawn(
            s.map_err(|e| error!("error sending boxed stream: {}", e))
                .for_each(move |v| {
                    send_stream_chunk(ptr, req_id, v.as_ptr() as *mut u8, v.len(), false);
                    Ok(())
                })
                .and_then(move |_| {
                    send_done_stream(ptr, req_id);
                    Ok(())
                }),
        ),
        JsBody::Static(v) => {
            rt.spawn(future::lazy(move || {
                send_stream_chunk(ptr, req_id, v.as_ptr() as *mut u8, v.len(), true);
                Ok(())
            }));
        }
        JsBody::Stream(rx) => {
            rt.spawn(
                rx.map_err(move |e| error!("error reading from stream channel: {:?}", e))
                    .for_each(move |v| {
                        send_stream_chunk(ptr, req_id, v.as_ptr() as *mut u8, v.len(), false);
                        Ok(())
                    })
                    .and_then(move |_| {
                        send_done_stream(ptr, req_id);
                        Ok(())
                    }),
            );
        }
    };
}

pub fn send_stream_chunk(ptr: JsRuntime, req_id: u32, chunk: *mut u8, len: usize, done: bool) {
    let builder = &mut FlatBufferBuilder::new();
    let chunk_msg = msg::StreamChunk::create(
        builder,
        &msg::StreamChunkArgs {
            id: req_id,
            done: done,
        },
    );
    ptr.send(
        fly_buf_from(
            serialize_response(
                0,
                builder,
                msg::BaseArgs {
                    msg: Some(chunk_msg.as_union_value()),
                    msg_type: msg::Any::StreamChunk,
                    ..Default::default()
                },
            )
            .unwrap(),
        ),
        Some(fly_buf {
            alloc_ptr: ptr::null_mut() as *mut u8,
            alloc_len: 0,
            data_ptr: chunk,
            data_len: len,
        }),
    );
}

pub fn send_done_stream(ptr: JsRuntime, req_id: u32) {
    let builder = &mut FlatBufferBuilder::new();
    let chunk_msg = msg::StreamChunk::create(
        builder,
        &msg::StreamChunkArgs {
            id: req_id,
            done: true,
        },
    );
    ptr.send(
        fly_buf_from(
            serialize_response(
                0,
                builder,
                msg::BaseArgs {
                    msg: Some(chunk_msg.as_union_value()),
                    msg_type: msg::Any::StreamChunk,
                    ..Default::default()
                },
            )
            .unwrap(),
        ),
        None,
    );
}

pub enum EventResponseChannel {
    Http(oneshot::Receiver<JsHttpResponse>),
    Dns(oneshot::Receiver<JsDnsResponse>),
}

#[derive(Debug)]
pub enum EventDispatchError {
    PoisonedLock,
    Http(mpsc::SendError<JsHttpRequest>),
    Dns(mpsc::SendError<JsDnsRequest>),
}

pub fn signal_monitor() -> (
    impl Future<Item = (), Error = ()> + Send + 'static,
    oneshot::Receiver<()>,
) {
    let (sigtx, sigrx) = oneshot::channel();
    (
        Signal::new(SIGTERM)
            .join(Signal::new(SIGINT))
            .map_err(|error| {
                error!("Failed to set up process signal monitoring: {:?}", error);
            })
            .and_then(|(sigterms, sigints)| {
                // Stream of all signals we care about
                let signals = sigterms.select(sigints);
                // Take only the first signal in the stream and log that it was triggered
                signals
                    .take(1)
                    .map_err(|error| {
                        error!("Error while listening on process signals: {:?}", error);
                    })
                    .for_each(|signal| {
                        let signal_name = match signal {
                            SIGTERM => "SIGTERM",
                            SIGINT => "SIGINT",
                            _ => unreachable!(),
                        };
                        info!("Received {}, gracefully shutting down", signal_name);
                        Ok(())
                    })
            })
            .and_then(move |_| {
                sigtx.send(()).ok(); // don't care.
                Ok(())
            }),
        sigrx,
    )
}
