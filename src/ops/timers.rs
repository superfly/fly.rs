use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::runtime::{JsRuntime, Runtime};
use libfly::*;

use crate::utils::*;

use futures::{sync::oneshot, Future};
use std::time::{Duration, Instant};

use tokio::timer::Delay;

pub fn op_timer_start(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    debug!("op_timer_start");
    let msg = base.msg_as_timer_start().unwrap();
    let cmd_id = base.cmd_id();
    let timer_id = msg.id();
    let delay = msg.delay();

    let timers = &rt.timers;

    let ptr = rt.ptr;

    let fut = {
        let (delay_task, cancel_delay) = set_timeout(
            move || {
                remove_timer(ptr, timer_id);
            },
            delay,
        );

        timers.lock().unwrap().insert(timer_id, cancel_delay);
        delay_task
    };
    // }
    Box::new(fut.then(move |result| {
        let builder = &mut FlatBufferBuilder::new();
        let msg = msg::TimerReady::create(
            builder,
            &msg::TimerReadyArgs {
                id: timer_id,
                canceled: result.is_err(),
                ..Default::default()
            },
        );
        Ok(serialize_response(
            cmd_id,
            builder,
            msg::BaseArgs {
                msg: Some(msg.as_union_value()),
                msg_type: msg::Any::TimerReady,
                ..Default::default()
            },
        ))
    }))
}

fn remove_timer(ptr: JsRuntime, timer_id: u32) {
    let rt = ptr.to_runtime();
    rt.timers.lock().unwrap().remove(&timer_id);
}

pub fn op_timer_clear(rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let msg = base.msg_as_timer_clear().unwrap();
    debug!("op_timer_clear");
    remove_timer(rt.ptr, msg.id());
    ok_future(None)
}

fn set_timeout<F>(cb: F, delay: u32) -> (impl Future<Item = (), Error = ()>, oneshot::Sender<()>)
where
    F: FnOnce() -> (),
{
    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    let when = Instant::now() + Duration::from_millis(delay.into());
    let delay_task = Delay::new(when)
        .map_err(|e| panic!("timer failed; err={:?}", e))
        .and_then(|_| {
            cb();
            Ok(())
        })
        .select(cancel_rx)
        .map(|_| ())
        .map_err(|_| ());

    (delay_task, cancel_tx)
}
