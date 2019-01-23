use crate::msg;
use crate::runtime::Runtime;
use crate::utils::*;
use flatbuffers::FlatBufferBuilder;
use futures::Future;
use libfly::*;

pub fn op_get_challenge(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_acme_get_challenge().unwrap();
    let hostname = msg.hostname().unwrap().to_string();
    let token = msg.token().unwrap().to_string();

    let acme_store = rt.acme_store.as_ref();

    if acme_store.is_none() {
        return Box::new(odd_future("No acme store configured".to_string().into()));
    }

    let acme_store = acme_store.unwrap();

    Box::new(
        acme_store
            .get_challenge(hostname, token)
            .map_err(|e| format!("acme error: {:?}", e).into())
            .and_then(move |contents| {
                let builder = &mut FlatBufferBuilder::new();
                let contents = builder.create_string(&contents.unwrap_or("".to_string()));
                let msg = msg::AcmeGetChallengeReady::create(
                    builder,
                    &msg::AcmeGetChallengeReadyArgs {
                        contents: Some(contents),
                        ..Default::default()
                    },
                );
                Ok(serialize_response(
                    cmd_id,
                    builder,
                    msg::BaseArgs {
                        msg: Some(msg.as_union_value()),
                        msg_type: msg::Any::AcmeGetChallengeReady,
                        ..Default::default()
                    },
                ))
            }),
    )
}
