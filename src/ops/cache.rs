use futures::sync::mpsc;

use flatbuffers::FlatBufferBuilder;
use msg;

use libfly::*;
use runtime::{JsRuntime, Op};
use utils::*;

use NEXT_EVENT_ID;

use futures::{future, Future, Stream};

use std::sync::atomic::Ordering;

use cache_store;

use std::ptr;

pub fn op_cache_del(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_cache_del().unwrap();
  let key = msg.key().unwrap().to_string();

  let rt = ptr.to_runtime();

  rt.spawn(
    rt.cache_store
      .del(key)
      .map_err(|e| error!("error cache del future! {:?}", e)),
  );

  ok_future(None)
}

pub fn op_cache_expire(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_cache_expire().unwrap();
  let key = msg.key().unwrap().to_string();
  let ttl = msg.ttl();

  let rt = ptr.to_runtime();

  rt.spawn(
    rt.cache_store
      .expire(key, ttl)
      .map_err(|e| error!("error cache expire future! {:?}", e)),
  );

  ok_future(None)
}

pub fn op_cache_set(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_cache_set().unwrap();
  let key = msg.key().unwrap().to_string();

  let stream_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

  let rt = ptr.to_runtime();

  let (sender, recver) = mpsc::unbounded::<Vec<u8>>();
  {
    rt.streams.lock().unwrap().insert(stream_id, sender);
  }

  let ttl = if msg.ttl() == 0 {
    None
  } else {
    Some(msg.ttl())
  };

  let fut = rt.cache_store.set(key, Box::new(recver), ttl);

  rt.spawn(
    fut
      .map_err(|e| println!("error cache set stream! {:?}", e))
      .and_then(move |_b| Ok(())),
  );

  let builder = &mut FlatBufferBuilder::new();
  let msg = msg::CacheSetReady::create(
    builder,
    &msg::CacheSetReadyArgs {
      id: stream_id,
      ..Default::default()
    },
  );
  ok_future(serialize_response(
    cmd_id,
    builder,
    msg::BaseArgs {
      msg: Some(msg.as_union_value()),
      msg_type: msg::Any::CacheSetReady,
      ..Default::default()
    },
  ))
}

pub fn op_cache_get(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_cache_get().unwrap();

  let stream_id = NEXT_EVENT_ID.fetch_add(1, Ordering::SeqCst) as u32;

  let key = msg.key().unwrap().to_string();

  let rt = ptr.to_runtime();

  let maybe_stream = match rt.cache_store.get(key) {
    Ok(s) => s,
    Err(e) => match e {
      cache_store::CacheError::NotFound => return odd_future("not found".to_string().into()),
      cache_store::CacheError::IoErr(ioe) => return odd_future(ioe.into()),
      cache_store::CacheError::Unknown => return odd_future("unknown error".to_string().into()),
      cache_store::CacheError::Failure(e) => return odd_future(e.into()),
    },
  };

  let got = maybe_stream.is_some();

  {
    // need to hijack the order here.
    let fut = future::lazy(move || {
      let builder = &mut FlatBufferBuilder::new();
      let msg = msg::CacheGetReady::create(
        builder,
        &msg::CacheGetReadyArgs {
          id: stream_id,
          stream: got,
          ..Default::default()
        },
      );
      ptr.send(
        fly_buf_from(
          serialize_response(
            cmd_id,
            builder,
            msg::BaseArgs {
              msg: Some(msg.as_union_value()),
              msg_type: msg::Any::CacheGetReady,
              ..Default::default()
            },
          ).unwrap(),
        ),
        None,
      );
      Ok(())
    });

    rt.spawn(fut);
  }

  // TODO: use send_body_stream somehow
  if let Some(stream) = maybe_stream {
    let fut = stream
      .map_err(|e| println!("error cache stream: {:?}", e))
      .for_each(move |bytes| {
        let builder = &mut FlatBufferBuilder::new();
        let chunk_msg = msg::StreamChunk::create(
          builder,
          &msg::StreamChunkArgs {
            id: stream_id,
            done: false,
            ..Default::default()
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
            ).unwrap(),
          ),
          Some(fly_buf {
            alloc_ptr: ptr::null_mut() as *mut u8,
            alloc_len: 0,
            data_ptr: (*bytes).as_ptr() as *mut u8,
            data_len: bytes.len(),
          }),
        );
        Ok(())
      }).and_then(move |_| {
        let builder = &mut FlatBufferBuilder::new();
        let chunk_msg = msg::StreamChunk::create(
          builder,
          &msg::StreamChunkArgs {
            id: stream_id,
            done: true,
            ..Default::default()
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
            ).unwrap(),
          ),
          None,
        );
        Ok(())
      });
    rt.spawn(fut);
  }

  ok_future(None)
}
