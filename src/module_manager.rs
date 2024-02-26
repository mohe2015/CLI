extern crate libloading;

use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::c_char;
use std::ffi::c_double;
use std::ffi::c_long;
use std::ffi::c_void;
use std::path::Path;

use crate::printer::raise_error;

use self::libloading::Symbol;

use self::libloading::Library;

#[repr(C)]
pub struct Cut {
  pub start: c_double,
  pub end: c_double,
}

#[repr(C)]
pub struct CutList {
  pub length: c_long,
  pub cuts: *const Cut,
}

#[repr(C)]
#[derive(Clone)]
pub struct GeneratorStats {
  pub len_pre_cut: c_double,
  pub len_post_cut: c_double,
}

#[repr(C)]
pub struct GeneratorResult {
  pub cuts: CutList,
  pub stats: GeneratorStats,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CArgument {
  pub short: c_char,
  pub long: *const c_char,
  pub description: *const c_char,
  pub required: bool,
  pub is_flag: bool,
}

pub struct Argument {
  pub short: char,
  pub long: String,
  pub description: String,
  pub required: bool,
  pub is_flag: bool,
}

impl From<CArgument> for Argument {
  fn from(c_arg: CArgument) -> Argument {
    let long = unsafe { CStr::from_ptr(c_arg.long) };
    let description = unsafe { CStr::from_ptr(c_arg.description) };
    Argument {
      short: c_arg.short as u8 as char,
      long: long.to_str().unwrap().to_string(),
      description: description.to_str().unwrap().to_string(),
      required: c_arg.required,
      is_flag: c_arg.is_flag,
    }
  }
}

#[repr(C)]
pub struct CArgumentList {
  pub length: c_long,
  pub arguments: *const CArgument,
}

pub type ArgumentList<'a> = Vec<Argument>;

impl From<CArgumentList> for ArgumentList<'static> {
  fn from(c_arg_list: CArgumentList) -> ArgumentList<'static> {
    let mut arguments: Vec<Argument> = Vec::new();
    for i in 0..c_arg_list.length {
      let c_arg = unsafe { *c_arg_list.arguments.offset(i as isize) };
      arguments.push(c_arg.into());
    }
    arguments
  }
}

#[derive(Clone)]
pub struct ArgumentResult {
  pub long: String,
  pub value: String,
}

#[repr(C)]
pub struct CArgumentResult {
  pub long: *const c_char,
  pub value: *const c_char,
}

#[repr(C)]
pub struct CArgumentResultList {
  pub length: c_long,
  pub results: *const CArgumentResult,
}

impl From<Vec<ArgumentResult>> for CArgumentResultList {
  fn from(arg_res_list: Vec<ArgumentResult>) -> CArgumentResultList {
    let length = arg_res_list.len() as c_long;
    let results: Vec<CArgumentResult> = arg_res_list.iter().map(|arg_res| {
      let long = CString::new(arg_res.long.as_str()).unwrap();
      let value = CString::new(arg_res.value.as_str()).unwrap();
      let result = CArgumentResult {
        long: long.as_ptr(),
        value: value.as_ptr(),
      };
      std::mem::forget(long);
      std::mem::forget(value);
      result
    }).collect();
    let result = CArgumentResultList {
      length,
      results: results.as_ptr(),
    };
    std::mem::forget(results);
    result
  }
}

type ErrorCallback = unsafe extern fn(*const c_char) -> ();
type ProgressCallback = unsafe extern fn(*const c_char, c_double) -> ();

type InitFunc<'a> = Symbol<'a, unsafe extern fn(ErrorCallback) -> ()>;
type VersionFunc<'a> = Symbol<'a, unsafe extern fn(ErrorCallback) -> *const c_char>;
type GetArgumentsFunc<'a> = Symbol<'a, unsafe extern fn(ErrorCallback) -> CArgumentList>;
type RenderFunc<'a> = Symbol<'a, unsafe extern fn(*const c_char, *const c_char, CutList, CArgumentResultList, ProgressCallback, ErrorCallback) -> c_void>;
type GenerateFunc<'a> = Symbol<'a, unsafe extern fn(*const c_char, CArgumentResultList, ProgressCallback, ErrorCallback) -> GeneratorResult>;

unsafe extern fn module_error_callback(message: *const c_char) {
  raise_error(&std::ffi::CStr::from_ptr(message).to_string_lossy());
}

pub fn load_render() -> Library {
  // load render so from modules/render.so (render.dll)
  let mut lib_path = "modules/librender.so";
  if cfg!(windows) {
    lib_path = "modules/render.dll";
  }
  if cfg!(target_os = "macos") {
    lib_path = "modules/librender.dylib";
  }
  let binding = std::env::current_exe().unwrap();
  let binding = binding.parent().unwrap().join(lib_path);
  lib_path = binding.to_str().unwrap();
  
  if !Path::new(lib_path).exists() {
    raise_error(format!("{} does not exist. Please compile the render module first.", lib_path).as_str());
  }
  
  let lib: Library = unsafe { Library::new(lib_path).unwrap() };

  let init: InitFunc = unsafe { lib.get(b"init").unwrap() };
  
  unsafe {
    init(module_error_callback);
  }
  
  lib
}

pub fn render_render(lib: &Library, input: &str, output: &str, cuts: CutList, args: CArgumentResultList, progress: ProgressCallback) {
  let render: RenderFunc = unsafe { lib.get(b"render").unwrap() };
  let input = CString::new(input).unwrap();
  let output = CString::new(output).unwrap();
  unsafe { render(input.as_ptr(), output.as_ptr(), cuts, args, progress, module_error_callback) };
}

pub fn load_generator() -> Library {
  // load generator so from modules/generator.so (generator.dll)
  
  let mut lib_path = "modules/libgenerator.so";
  if cfg!(windows) {
    lib_path = "modules/generator.dll";
  }
  if cfg!(target_os = "macos") {
    lib_path = "modules/libgenerator.dylib";
  }
  let binding = std::env::current_exe().unwrap();
  let binding = binding.parent().unwrap().join(lib_path);
  lib_path = binding.to_str().unwrap();

  if !Path::new(lib_path).exists() {
    raise_error(format!("{} does not exist. Please compile the generator module first.", lib_path).as_str());
  }
  
  let lib = unsafe { Library::new(lib_path).unwrap() };
  
  let init: InitFunc = unsafe { lib.get(b"init").unwrap() };

  unsafe {
    init(module_error_callback);
  }

  lib
}

pub fn generator_generate(lib: &Library, input: &str, args: CArgumentResultList, progress: ProgressCallback) -> GeneratorResult {
  let generate: GenerateFunc = unsafe { lib.get(b"generate").unwrap() };
  let input = CString::new(input).unwrap();
  unsafe { generate(input.as_ptr(), args, progress, module_error_callback) }
}

pub fn module_version(lib: &Library) -> String {
  let version: VersionFunc = unsafe { lib.get(b"version").unwrap() };
  let version = unsafe { version(module_error_callback) };
  let version = unsafe { CStr::from_ptr(version) };
  version.to_str().unwrap().to_string()
}

pub fn module_get_arguments(lib: &Library) -> ArgumentList {
  let get_arguments: GetArgumentsFunc = unsafe { lib.get(b"get_arguments").unwrap() };
  unsafe { get_arguments(module_error_callback).into() }
}