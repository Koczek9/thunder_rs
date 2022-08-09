/*
 * Copyright 2022 Comcast Cable Communications Management, LLC
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 * SPDX-License-Identifier: Apache-2.0
 */
use std::ffi::CStr;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::mpsc::Sender;

type SendToFunction = unsafe extern "C" fn (u32, *const c_char, u32);

pub trait Plugin {
  fn on_message(&mut self, json: String, ctx: RequestContext);
  fn on_client_connect(&mut self, channel: u32);
  fn on_client_disconnect(&mut self, channel: u32);
}

pub struct Message {
  pub channel: u32,
  pub data: String
}

#[derive(Clone)]
pub struct RequestContext {
  pub channel: u32,
  pub auth_token: String,
  pub responder: Sender<Message>
}

impl RequestContext {
  pub fn send(&self, json: String) {
    let m = Message {
      channel: self.channel,
      data: json
    };
    let _result = self.responder.send(m);
    // TODO: check result and report any problems
  }
}

pub struct ServiceMetadata {
  pub name: &'static str,
  pub version: (u32, u32, u32),
  pub create: fn () -> Box<dyn Plugin>
}

#[macro_export]
macro_rules! export_plugin {
  ($name:expr, $version:expr,  $create:expr) => {
    #[no_mangle]
    pub static thunder_service_metadata : $crate::ServiceMetadata =
      $crate::ServiceMetadata {
        name: $name,
        version: $version,
        create: $create
      };
  };
}

//===============================================================================
// Internal code only below here
//===============================================================================

#[repr(C)]
pub struct CRequestContext {
  channel: u32,
  auth_token: *const c_char
}

fn cstr_to_string(s : *const c_char) -> String {
  if s.is_null() {
    String::new()
  }
  else {
    let c_str: &CStr = unsafe{ CStr::from_ptr(s) };
    let slice: &str = c_str.to_str().unwrap();
    let t: String = slice.to_owned();
    t
  }
}

pub struct CPlugin {
  pub name: String,
  pub plugin: Box<dyn Plugin>,
  sender: std::sync::mpsc::Sender<Message>
}

impl CPlugin {
  fn on_incoming_message(&mut self, json_req: *const c_char, ctx: CRequestContext) {
    let req = cstr_to_string(json_req);
    let req_ctx = RequestContext {
      channel: ctx.channel,
      auth_token: cstr_to_string(ctx.auth_token),
      responder: self.sender.clone()
    };
    println!("dispatch from thunder");
    self.plugin.on_message(req, req_ctx);
  }
  fn on_client_connect(&mut self, channel: u32) {
    self.plugin.on_client_connect(channel);
  }
  fn on_client_disconnect(&mut self, channel: u32) {
    self.plugin.on_client_disconnect(channel);
  }
}

#[no_mangle]
pub extern fn wpe_rust_plugin_create(_name: *const c_char, send_func: SendToFunction,
  plugin_ctx: u32, meta_data: *mut ServiceMetadata) -> *mut CPlugin
{
  assert!(!meta_data.is_null());

  let service_metadata = unsafe{ &*meta_data };
  let plugin: Box<dyn Plugin> = (service_metadata.create)();
  let name: String = service_metadata.name.to_string();

  let (tx, rx) = std::sync::mpsc::channel::<Message>();

  let c_plugin: Box<CPlugin> = Box::new(CPlugin {
    name: name,
    plugin: plugin,
    sender: tx
  });

  std::thread::spawn(move || {
    while let Ok(m) = rx.recv() {
      let c_str = CString::new(m.data).unwrap();
      unsafe {
        send_func(m.channel, c_str.as_ptr(), plugin_ctx);
      }
    }
  });

  Box::into_raw(c_plugin)
}

#[no_mangle]
pub extern fn wpe_rust_plugin_destroy(ptr: *mut CPlugin) {
  assert!(!ptr.is_null());

  unsafe {
    drop(Box::from_raw(ptr));
  }
}

#[no_mangle]
pub extern fn wpe_rust_plugin_init(_ptr: *mut CPlugin, _json: *const c_char) {
  // assert!(!ptr.is_null());

  // XXX: Create + Init doesn't seem to fit the Rust style. wpe_rust_plugin_create 
  // is probably enough. Consider getting rid of this function

  // let plugin = unsafe{ &mut *ptr };
  // println!("{}.init", plugin.name);
}

#[no_mangle]
pub extern fn wpe_rust_plugin_invoke(ptr: *mut CPlugin, json_req: *const c_char, req_ctx: CRequestContext) {
  assert!(!ptr.is_null());
  assert!(!json_req.is_null());

  let plugin = unsafe{ &mut *ptr };
  let uncaught_error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    plugin.on_incoming_message(json_req, req_ctx);
  }));

  match uncaught_error {
    Err(cause) => {
      println!("Error calling on_incoming_message");
      println!("{:?}", cause);
    }
    Ok(_) => { }
  }
}

#[no_mangle]
pub extern fn wpe_rust_plugin_on_client_connect(ptr: *mut CPlugin, channel: u32) {
  assert!(!ptr.is_null());

  let plugin = unsafe{ &mut *ptr };
  let uncaught_error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    plugin.on_client_connect(channel);
  }));

  match uncaught_error {
    Err(cause) => {
      println!("Error calling on_client_connect");
      println!("{:?}", cause);
    }
    Ok(_) => { }
  }
}

#[no_mangle]
pub extern fn wpe_rust_plugin_on_client_disconnect(ptr: *mut CPlugin, channel: u32) {
  assert!(!ptr.is_null());

  let plugin = unsafe{ &mut *ptr };
  let uncaught_error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
    plugin.on_client_disconnect(channel);
  }));

  match uncaught_error {
    Err(cause) => {
      println!("Error calling on_client_disconnect");
      println!("{:?}", cause);
    }
    Ok(_) => { }
  }
}
