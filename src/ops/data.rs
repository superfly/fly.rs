use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::runtime::Runtime;
use crate::utils::*;
use libfly::*;

use futures::Future;

pub fn op_data_put(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let msg = base.msg_as_data_put().unwrap();
    let coll = msg.collection().unwrap().to_string();
    let key = msg.key().unwrap().to_string();
    let value = msg.json().unwrap().to_string();

    Box::new(
        rt.data_store
            .put(coll, key, value)
            .map_err(|e| format!("{:?}", e).into())
            .and_then(move |_| Ok(None)),
    )
}

pub fn op_data_get(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_data_get().unwrap();
    let coll = msg.collection().unwrap().to_string();
    let key = msg.key().unwrap().to_string();

    Box::new(
        rt.data_store
            .get(coll, key)
            .map_err(|e| format!("error in data store get: {:?}", e).into())
            .and_then(move |s| match s {
                None => Ok(None),
                Some(s) => {
                    let builder = &mut FlatBufferBuilder::new();
                    let json = builder.create_string(&s);
                    let msg = msg::DataGetReady::create(
                        builder,
                        &msg::DataGetReadyArgs {
                            json: Some(json),
                            ..Default::default()
                        },
                    );
                    Ok(serialize_response(
                        cmd_id,
                        builder,
                        msg::BaseArgs {
                            msg: Some(msg.as_union_value()),
                            msg_type: msg::Any::DataGetReady,
                            ..Default::default()
                        },
                    ))
                }
            }),
    )
}

pub fn op_data_del(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let msg = base.msg_as_data_del().unwrap();
    let coll = msg.collection().unwrap().to_string();
    let key = msg.key().unwrap().to_string();

    Box::new(
        rt.data_store
            .del(coll, key)
            .map_err(|e| format!("{:?}", e).into())
            .and_then(move |_| Ok(None)),
    )
}

pub fn op_data_drop_coll(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let msg = base.msg_as_data_drop_collection().unwrap();
    let coll = msg.collection().unwrap().to_string();

    Box::new(
        rt.data_store
            .drop_coll(coll)
            .map_err(|e| format!("{:?}", e).into())
            .and_then(move |_| Ok(None)),
    )
}

pub fn op_data_incr(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let msg = base.msg_as_data_incr().unwrap();
    let coll = msg.collection().unwrap().to_string();
    let key = msg.key().unwrap().to_string();
    let field = msg.field().unwrap().to_string();
    let amount = msg.amount();

    Box::new(
        rt.data_store
            .incr(coll, key, field, amount)
            .map_err(|e| format!("{:?}", e).into())
            .and_then(move |_| Ok(None)),
    )
}
