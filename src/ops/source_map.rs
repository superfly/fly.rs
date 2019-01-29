use crate::msg;
use flatbuffers::FlatBufferBuilder;

use crate::runtime::Runtime;
use crate::v8env::V8ENV_SOURCEMAP;
use libfly::*;

use crate::utils::*;

use sourcemap::SourceMap;
use std::sync::{mpsc, Mutex};

use futures::{sync::oneshot, Future};
use std::thread;

type SourceMapId = (u32, u32, String, String);

lazy_static! {
    static ref SM_CHAN: Mutex<mpsc::Sender<(Vec<SourceMapId>, oneshot::Sender<Vec<SourceMapId>>)>> = {
        let (sender, receiver) =
            mpsc::channel::<(Vec<SourceMapId>, oneshot::Sender<Vec<SourceMapId>>)>();
        thread::Builder::new()
            .name("sourcemapper".to_string())
            .spawn(move || {
                let sm = SourceMap::from_reader(*V8ENV_SOURCEMAP).unwrap();
                for tup in receiver.iter() {
                    let ch = tup.1;
                    let v = tup.0;
                    ch.send(
                        v.iter()
                            .map(|(line, col, name, filename)| {
                                if filename == "v8env/dist/v8env.js" {
                                    return match sm.lookup_token(*line, *col) {
                                        Some(t) => {
                                            let newline = t.get_src_line();
                                            let newcol = t.get_src_col();
                                            let newfilename = match t.get_source() {
                                                Some(s) => String::from(s),
                                                None => filename.clone(),
                                            };
                                            (newline, newcol, name.clone(), newfilename)
                                        }
                                        None => (*line, *col, name.clone(), filename.clone()),
                                    };
                                }
                                (*line, *col, name.clone(), filename.clone())
                            })
                            .collect(),
                    )
                    .unwrap();
                }
            })
            .unwrap();
        Mutex::new(sender)
    };
}

pub fn op_source_map(_rt: &mut Runtime, base: &msg::Base, _raw: fly_buf) -> Box<Op> {
    let cmd_id = base.cmd_id();
    let msg = base.msg_as_source_map().unwrap();

    let msg_frames = msg.frames().unwrap();
    let mut frames = Vec::with_capacity(msg_frames.len());

    for i in 0..msg_frames.len() {
        let f = msg_frames.get(i);

        debug!(
            "got frame: {:?} {:?} {:?} {:?}",
            f.name(),
            f.filename(),
            f.line(),
            f.col()
        );

        let name = match f.name() {
            Some(n) => n,
            None => "",
        };

        let filename = match f.filename() {
            Some(f) => f,
            None => "",
        };

        let line = f.line();
        let col = f.col();

        frames.insert(i, (line, col, String::from(name), String::from(filename)));
    }

    let (tx, rx) = oneshot::channel::<Vec<SourceMapId>>();
    if let Err(err) = SM_CHAN.lock().unwrap().send((frames, tx)) {
        return odd_future(format!("{}", err).into());
    }

    Box::new(rx.map_err(|e| format!("{}", e).into()).and_then(move |v| {
        let builder = &mut FlatBufferBuilder::new();
        let framed: Vec<_> = v
            .iter()
            .map(|(line, col, name, filename)| {
                let namefbb = builder.create_string(name.as_str());
                let filenamefbb = builder.create_string(filename.as_str());
                msg::Frame::create(
                    builder,
                    &msg::FrameArgs {
                        name: Some(namefbb),
                        filename: Some(filenamefbb),
                        line: *line,
                        col: *col,
                    },
                )
            })
            .collect();
        let ret_frames = builder.create_vector(&framed);

        let ret_msg = msg::SourceMapReady::create(
            builder,
            &msg::SourceMapReadyArgs {
                frames: Some(ret_frames),
                ..Default::default()
            },
        );
        Ok(serialize_response(
            cmd_id,
            builder,
            msg::BaseArgs {
                msg: Some(ret_msg.as_union_value()),
                msg_type: msg::Any::SourceMapReady,
                ..Default::default()
            },
        ))
    }))
}
