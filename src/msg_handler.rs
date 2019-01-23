use crate::msg;
use crate::runtime::Runtime;
use libfly::*;

use crate::ops;
use crate::utils::*;

lazy_static! {
    pub static ref DEFAULT_MESSAGE_HANDLER: DefaultMessageHandler = DefaultMessageHandler {};
}

pub trait MessageHandler {
    fn handle_msg(&self, rt: &mut Runtime, base: &msg::Base, raw_buf: fly_buf) -> Box<Op>;
}

pub struct DefaultMessageHandler {}

impl MessageHandler for DefaultMessageHandler {
    fn handle_msg(&self, rt: &mut Runtime, base: &msg::Base, raw_buf: fly_buf) -> Box<Op> {
        let msg_type = base.msg_type();
        debug!("MSG TYPE: {:?}", msg_type);
        let handler: Handler = match msg_type {
            msg::Any::TimerStart => ops::timers::op_timer_start,
            msg::Any::TimerClear => ops::timers::op_timer_clear,
            msg::Any::HttpRequest => ops::fetch::op_fetch,
            msg::Any::HttpResponse => ops::fetch::op_http_response,
            msg::Any::StreamChunk => ops::streams::op_stream_chunk,
            msg::Any::CacheGet => ops::cache::op_cache_get,
            msg::Any::CacheSet => ops::cache::op_cache_set,
            msg::Any::CacheDel => ops::cache::op_cache_del,
            msg::Any::CacheNotifyDel => ops::cache::op_cache_notify_del,
            msg::Any::CacheNotifyPurgeTag => ops::cache::op_cache_notify_purge_tag,
            msg::Any::CacheExpire => ops::cache::op_cache_expire,
            msg::Any::CacheSetMeta => ops::cache::op_cache_set_meta,
            msg::Any::CachePurgeTag => ops::cache::op_cache_purge_tag,
            msg::Any::CryptoDigest => ops::crypto::op_crypto_digest,
            msg::Any::CryptoRandomValues => ops::crypto::op_crypto_random_values,
            msg::Any::SourceMap => ops::source_map::op_source_map,
            msg::Any::DataPut => ops::data::op_data_put,
            msg::Any::DataGet => ops::data::op_data_get,
            msg::Any::DataDel => ops::data::op_data_del,
            msg::Any::DataIncr => ops::data::op_data_incr,
            msg::Any::DataDropCollection => ops::data::op_data_drop_coll,
            msg::Any::DnsQuery => ops::dns::op_dns_query,
            msg::Any::DnsResponse => ops::dns::op_dns_response,
            msg::Any::AddEventListener => ops::events::op_add_event_ln,
            msg::Any::LoadModule => ops::modules::op_load_module,
            msg::Any::ImageApplyTransforms => ops::image::op_image_transform,
            msg::Any::AcmeGetChallenge => ops::acme::op_get_challenge,
            _ => unimplemented!(),
        };

        handler(rt, base, raw_buf)
    }
}
