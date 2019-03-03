use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::runtime::Runtime;
use crate::errors::permission_denied;
use libfly::*;

use crate::utils::*;

use crate::module_resolver::RefererInfo;

use futures::future;

pub fn op_load_module(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_load_module().unwrap();
    let specifier_url = msg.specifier_url().unwrap().to_string();

    if !rt.dev_tools {
        return odd_future(permission_denied());
    }

    let referer_info = match msg.referer_origin_url() {
        Some(v) => Some(RefererInfo {
            origin_url: v.to_string(),
            is_wasm: Some(false),
            source_code: None,
            indentifier_hash: None,
        }),
        None => None,
    };

    let module = match rt
        .module_resolver_manager
        .resolve_module(specifier_url, referer_info)
    {
        Ok(m) => m,
        Err(e) => return odd_future(e.into()),
    };

    Box::new(future::lazy(move || {
        let builder = &mut FlatBufferBuilder::new();
        let origin_url = builder.create_string(&module.origin_url);
        let source_code = builder.create_string(&module.loaded_source.source);

        let msg = msg::LoadModuleResp::create(
            builder,
            &msg::LoadModuleRespArgs {
                origin_url: Some(origin_url),
                source_code: Some(source_code),
            },
        );
        Ok(serialize_response(
            cmd_id,
            builder,
            msg::BaseArgs {
                msg: Some(msg.as_union_value()),
                msg_type: msg::Any::LoadModuleResp,
                ..Default::default()
            },
        ))
    }))
}
