use super::*;
use libc::c_char;
use std::collections::HashMap;
use std::ffi::{CStr, CString};

#[repr(C)]
#[derive(Debug)]
pub struct Message {
  pub id: u32,
  pub sync: bool,
  pub kind: MessageKind,
  pub value: Value,
}

use std::slice;

impl Message {
  pub fn payload(&self) -> Option<MessagePayload> {
    let pl: HashMap<&str, Value> = match self.value {
      Value::Object { len, pairs } => unsafe {
        slice::from_raw_parts(pairs, len as usize)
          .iter()
          .map(|kv| (CStr::from_ptr(kv.key).to_str().unwrap(), *kv.val))
          .collect()
      },
      _ => unimplemented!(),
    };
    match self.kind {
      MessageKind::TimerStart => {
        let id = match pl.get("id")? {
          Value::Int32(i) => (*i) as u32,
          _ => panic!("wrong value type"),
        };
        let delay = match pl.get("delay")? {
          Value::Int32(i) => (*i) as u32,
          _ => panic!("wrong value type"),
        };
        Some(MessagePayload::TimerStart(TimerStart { id, delay }))
      }
      _ => None,
    }
  }
}

#[repr(C)]
#[derive(Debug, PartialEq)]
pub enum MessageKind {
  TimerStart,
  TimerReady,
}

#[derive(Debug)]
pub enum MessagePayload {
  TimerStart(TimerStart),
  // TimerReady(TimerReady),
  // TimerClear(TimerClear),
}

#[derive(Default, Debug)]
pub struct TimerStart {
  pub id: u32,
  pub delay: u32,
}
