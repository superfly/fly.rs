use crate::msg;
use crate::runtime::{JsRuntime, Op};
use crate::utils::*;
use flatbuffers::FlatBufferBuilder;
use futures::Future;
use libfly::*;

pub fn op_validate_challenge(ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
  let cmd_id = base.cmd_id();
  let msg = base.msg_as_acme_validate_challenge().unwrap();
  let hostname = msg.hostname().unwrap().to_string();
  let token = msg.token().unwrap().to_string();

  let rt = ptr.to_runtime();

  if rt.acme_store.is_none() {
    return Box::new(odd_future("No acme store configured".to_string().into()));
  }

  let acme_store = rt.acme_store.as_ref().unwrap();

  Box::new(
    acme_store
      .validate_challenge(hostname, token)
      .map_err(|e| format!("acme error: {:?}", e).into())
      .and_then(move |contents| {
        let builder = &mut FlatBufferBuilder::new();
        let valid = contents.is_some();
        let contents = builder.create_string(&contents.unwrap_or("".to_string()));
        let msg = msg::AcmeValidateChallengeReady::create(
          builder,
          &msg::AcmeValidateChallengeReadyArgs {
            valid: valid,
            contents: Some(contents),
            ..Default::default()
          },
        );
        Ok(serialize_response(
          cmd_id,
          builder,
          msg::BaseArgs {
            msg: Some(msg.as_union_value()),
            msg_type: msg::Any::AcmeValidateChallengeReady,
            ..Default::default()
          },
        ))
      }),
  )
}
