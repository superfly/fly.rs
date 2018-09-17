// extern crate js_value_derive;
// use js_value_derive;
// use super::js_value_derive;

use libfly::*;
use std::ffi::CString;

#[derive(Debug)]
pub struct Command {
  pub id: u32,
  pub msg: Message,
}

impl Command {
  pub fn new(id: u32, msg: Message) -> Self {
    Command { id, msg }
  }
}

impl JsValue for Command {
  fn prepare_js(&self) -> TempValue {
    TempValue::Object(TempObject::new(vec![
      TempKeyValue::new("id".to_string(), TempValue::Uint32(self.id)),
      TempKeyValue::new("msg".to_string(), self.msg.prepare_js()),
    ]))
  }
}

#[derive(Debug)]
pub enum Message {
  TimerStart(TimerStart),
  TimerReady(TimerReady),
  TimerClear(TimerClear),
}

impl Message {
  pub fn prepare_js(&self) -> TempValue {
    use self::Message::*;
    match self {
      TimerStart(t) => t.prepare_js(),
      TimerClear(t) => t.prepare_js(),
      TimerReady(t) => t.prepare_js(),
    }
  }
  // pub fn to_js(&self) -> Value {
  //   use self::Message::*;
  //   match self {
  //     TimerStart(t) => t.to_js(),
  //     TimerClear(t) => t.to_js(),
  //     TimerReady(t) => t.to_js(),
  //   }
  // }
}

#[derive(Debug)]
pub struct TempKeyValue {
  key: CString,
  val: TempValue,
  v8: Value,
}

impl TempKeyValue {
  pub fn new(name: String, mut v: TempValue) -> Self {
    let v8 = v.to_js();
    TempKeyValue {
      key: CString::new(name).unwrap(),
      val: v,
      v8: v8,
    }
  }
  pub fn to_js(&self) -> KeyValue {
    KeyValue {
      key: self.key.as_ptr(),
      val: &self.v8,
    }
  }
}

#[derive(Debug)]
pub struct TempObject {
  pairs: Vec<TempKeyValue>,
  v8: Vec<KeyValue>,
  // _keep: Vec<KeyValue>,
}

impl TempObject {
  pub fn new(pairs: Vec<TempKeyValue>) -> Self {
    let v8 = pairs.iter().map(|p| p.to_js()).collect();
    TempObject { pairs, v8: v8 }
  }
  pub fn to_js(&self) -> Value {
    Value::Object {
      len: self.pairs.len() as i32,
      pairs: self.v8.as_ptr(),
    }
  }
}

#[derive(Debug)]
pub struct TempArray(Vec<Value>);

impl TempArray {
  pub fn new(mut values: Vec<TempValue>) -> Self {
    TempArray(values.iter_mut().map(|v| v.to_js()).collect())
  }
  pub fn to_js(&self) -> Value {
    Value::Array {
      len: self.0.len(),
      values: self.0.as_ptr(),
    }
  }
}

#[derive(Debug)]
pub enum TempValue {
  None,
  Int32(i32),
  Uint32(u32),
  String(CString),
  Object(TempObject),
  Array(TempArray),
  ArrayBuffer(Vec<u8>),
  Uint8Array(Vec<u8>),
}

impl TempValue {
  pub fn to_js(&self) -> Value {
    use self::TempValue::*;
    match self {
      None => Value::None,
      Int32(i) => Value::Int32(*i),
      Uint32(i) => Value::Int32(*i as i32),
      String(s) => Value::String(s.as_ptr()),
      Object(o) => o.to_js(),
      Array(a) => a.to_js(),
      _ => unimplemented!(),
    }
  }
}

impl Default for TempValue {
  fn default() -> Self {
    TempValue::None
  }
}

#[derive(Debug)]
pub struct TimerStart {
  pub id: u32,
  pub delay: u32,
}

impl JsValue for TimerStart {
  fn prepare_js(&self) -> TempValue {
    TempValue::Object(TempObject::new(vec![
      TempKeyValue::new("id".to_string(), TempValue::Uint32(self.id)),
      TempKeyValue::new("delay".to_string(), TempValue::Uint32(self.delay)),
    ]))
  }
  // fn to_js(&self) -> Value {
  //   // self._v8 = TempValue::Object(TempObject::new(vec![
  //   //   TempKeyValue::new("id", TempValue::Uint32(self.id)),
  //   //   TempKeyValue::new("delay", TempValue::Uint32(self.delay)),
  //   // ]));
  //   self._v8.to_js()
  // }
}

#[derive(Debug)]
pub struct TimerClear {
  pub id: u32,
}
impl JsValue for TimerClear {
  fn prepare_js(&self) -> TempValue {
    TempValue::Object(TempObject::new(vec![TempKeyValue::new(
      "id".to_string(),
      TempValue::Uint32(self.id),
    )]))
  }
  // fn to_js(&self) -> Value {
  //   // self._v8 = TempValue::Object(TempObject::new(vec![TempKeyValue::new(
  //   //   "id",
  //   //   TempValue::Uint32(self.id),
  //   // )]));
  //   self._v8.to_js()
  // }
}

#[derive(Debug)]
pub struct TimerReady {
  pub id: u32,
}
impl TimerReady {
  pub fn new(id: u32) -> Self {
    TimerReady { id }
  }
}
impl JsValue for TimerReady {
  fn prepare_js(&self) -> TempValue {
    TempValue::Object(TempObject::new(vec![TempKeyValue::new(
      "id".to_string(),
      TempValue::Uint32(self.id),
    )]))
  }
  // fn to_js(&self) -> Value {
  //   // self._v8 = TempValue::Object(TempObject::new(vec![TempKeyValue::new(
  //   //   "id",
  //   //   TempValue::Uint32(self.id),
  //   // )]));
  //   self._v8.to_js()
  // }
}

pub trait JsValue {
  // fn to_js(&self) -> Value;
  fn prepare_js(&self) -> TempValue;
}
