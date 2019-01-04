use crate::acme_store::AcmeStore;
use crate::msg;
use crate::redis_acme::RedisAcmeStore;
use crate::runtime::{JsRuntime, Op};
use crate::settings::{AcmeStoreConfig, SETTINGS};
use crate::utils::*;
use flatbuffers::FlatBufferBuilder;
use futures::Future;
use libfly::*;

lazy_static! {
    static ref ACME_STORE: Option<Box<AcmeStore + 'static + Send + Sync>> =
        match &SETTINGS.read().unwrap().acme_store {
            Some(ref store) => match store {
                AcmeStoreConfig::Redis(conf) => Some(Box::new(RedisAcmeStore::new(&conf))),
            },
            None => None,
        };
}

pub fn op_validate_challenge(_ptr: JsRuntime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_acme_validate_challenge().unwrap();
    let hostname = msg.hostname().unwrap().to_string();
    let token = msg.token().unwrap().to_string();

    if ACME_STORE.is_none() {
        return Box::new(odd_future("No acme store configured".to_string().into()));
    }

    let acme_store = ACME_STORE.as_ref().unwrap();

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
