use futures::sync::mpsc;

use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::runtime::{JsBody, JsRuntime, Op};
use crate::utils::*;
use libfly::*;

use crate::get_next_stream_id;

use futures::{Future, Stream};

use crate::cache_store::*;
use crate::cache_store_notifier::*;

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

  let stream_id = get_next_stream_id();

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

  let tags = match msg.tags() {
    Some(raw_tags) => {
      let mut tags: Vec<String> = vec![];
      for i in 0..raw_tags.len() {
        tags.push(i.to_string());
      }
      Some(tags)
    }
    None => None,
  };

  let fut = rt.cache_store.set(
    key,
    Box::new(recver),
    CacheSetOptions {
      ttl,
      tags,
      meta: None,
    },
  );

  rt.spawn(
    fut
      .map_err(|e| error!("error cache set stream! {:?}", e))
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

  let stream_id = get_next_stream_id();

  let key = msg.key().unwrap().to_string();

  let rt = ptr.to_runtime();
  Box::new(
    rt.cache_store
      .get(key)
      .map_err(|e| format!("cache error: {:?}", e).into())
      .and_then(move |maybe_entry| {
        let builder = &mut FlatBufferBuilder::new();
        let msg = msg::CacheGetReady::create(
          builder,
          &msg::CacheGetReadyArgs {
            id: stream_id,
            stream: maybe_entry.is_some(),
            ..Default::default()
          },
        );
        if let Some(entry) = maybe_entry {
          send_body_stream(
            ptr,
            stream_id,
            JsBody::BoxedStream(Box::new(
              entry.stream.map_err(|e| format!("{:?}", e).into()),
            )),
          );
        }
        Ok(serialize_response(
          cmd_id,
          builder,
          msg::BaseArgs {
            msg: Some(msg.as_union_value()),
            msg_type: msg::Any::CacheGetReady,
            ..Default::default()
          },
        ))
      }),
  )
}

pub fn op_cache_notify_del(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_cache_notify_del().unwrap();

  let rt = ptr.to_runtime();
  let key = msg.key().unwrap().to_string();

  Box::new(
    rt.cache_store
      .notify(CacheOperation::Del, key)
      .map_err(|e| match e {
        CacheStoreNotifierError::Unknown => "cache notifier unknown error".to_string().into(),
        CacheStoreNotifierError::Failure(s) => s.into(),
        CacheStoreNotifierError::Unavailable => "cache notifications is not available".to_string().into(),
      })
      .and_then(|_| Ok(None)),
  )
}

pub fn op_cache_notify_purge_tag(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let msg = base.msg_as_cache_notify_purge_tag().unwrap();

  let rt = ptr.to_runtime();

  let tag = msg.tag().unwrap().to_string();

  Box::new(
    rt.cache_store
      .notify(CacheOperation::PurgeTag, tag)
      .map_err(|e| match e {
        CacheStoreNotifierError::Unknown => "cache notifier unknown error".to_string().into(),
        CacheStoreNotifierError::Failure(s) => s.into(),
        CacheStoreNotifierError::Unavailable => "cache notifications is not available".to_string().into(),
      })
      .and_then(|_| Ok(None)),
  )
}
