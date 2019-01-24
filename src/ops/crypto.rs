use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::runtime::Runtime;
use libfly::*;

use crate::utils::*;

use rand::{thread_rng, Rng};

#[allow(unused_imports)]
use sha1::Digest as Sha1Digest; // puts trait in scope
use sha1::Sha1;

#[allow(unused_imports)]
use sha2::Digest; // puts trait in scope
use sha2::Sha256;

use futures::future;
use std::slice;

pub fn op_crypto_random_values(_rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_crypto_random_values().unwrap();

    let len = msg.len() as usize;
    let mut v = vec![0u8; len];
    let arr = v.as_mut_slice();

    thread_rng().fill(arr);

    let builder = &mut FlatBufferBuilder::new();
    let ret_buffer = builder.create_vector(arr);

    let crypto_rand = msg::CryptoRandomValuesReady::create(
        builder,
        &msg::CryptoRandomValuesReadyArgs {
            buffer: Some(ret_buffer),
            ..Default::default()
        },
    );

    ok_future(serialize_response(
        cmd_id,
        builder,
        msg::BaseArgs {
            msg: Some(crypto_rand.as_union_value()),
            msg_type: msg::Any::CryptoRandomValuesReady,
            ..Default::default()
        },
    ))
}

pub fn op_crypto_digest(_rt: &mut Runtime, base: &msg::Base, raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_crypto_digest().unwrap();

    let algo = msg.algo().unwrap().to_uppercase();
    let buffer = unsafe { slice::from_raw_parts(raw.data_ptr, raw.data_len) }.to_vec();

    Box::new(future::lazy(move || {
        let builder = &mut FlatBufferBuilder::new();
        let bytes_vec = match algo.as_str() {
            "SHA-256" => {
                let mut h = Sha256::default();
                h.input(buffer.as_slice());
                let res = h.result();
                builder.create_vector(res.as_slice())
            }
            "SHA-1" => {
                let mut h = Sha1::default();
                h.input(buffer.as_slice());
                let res = h.result();
                builder.create_vector(res.as_slice())
            }
            _ => unimplemented!(),
        };

        let crypto_ready = msg::CryptoDigestReady::create(
            builder,
            &msg::CryptoDigestReadyArgs {
                buffer: Some(bytes_vec),
                ..Default::default()
            },
        );
        Ok(serialize_response(
            cmd_id,
            builder,
            msg::BaseArgs {
                msg: Some(crypto_ready.as_union_value()),
                msg_type: msg::Any::CryptoDigestReady,
                ..Default::default()
            },
        ))
    }))
}
