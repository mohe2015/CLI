extern crate libloading;
extern crate console;
extern crate indicatif;
extern crate clap;

mod argmunents;
mod printer;
mod helper;
mod module_manager;

extern crate once_cell;
use once_cell::sync::Lazy;

use std::fs::File;
use std::io::Write;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use argmunents::{parse_args, validate_args, Options};
use indicatif::ProgressBar;
use indicatif::ProgressStyle;
use module_manager::GeneratorStats;
use printer::print_stats;
use self::indicatif::MultiProgress;
use self::console::style;
use crate::module_manager::generator_generate;
use crate::module_manager::render_render;

use self::libloading::Library;
use module_manager::{load_render, load_generator, module_version};
use printer::{greetings, print_non_mp4_warning};

use std::ffi::c_char;
use std::ffi::c_double;

struct ProgressWrapper {
  pub progress: Option<MultiProgress>,
  pub pbars: Lazy<HashMap<String, ProgressBar>>,
}

static PROG_WRAPPER: Mutex<ProgressWrapper> = Mutex::new(ProgressWrapper {
  progress: Option::None,
  pbars: Lazy::new(HashMap::new),
});



fn run(options: &Options, generator: &Library, render: &Library) -> GeneratorStats  {
  if let Ok(locked_prog) = PROG_WRAPPER.lock() {
    if let Some(prog) = locked_prog.progress.as_ref() {
      if let Err(e) = prog.println(format!(" Input: {}", style(&options.input).yellow())) {
        eprintln!("Error: {}", e);
      }
      if let Err(e) = prog.println(format!("Output: {}", style(&options.output).yellow())) {
        eprintln!("Error: {}", e);
      }
      if let Err(e) = prog.println("") {
        eprintln!("Error: {}", e);
      }
    }
    
    drop(locked_prog);
  }

  if tree_magic::from_filepath(Path::new(options.input.as_str())) != "video/mp4" {
    print_non_mp4_warning();
  }

  unsafe extern fn progress_callback(name: *const c_char, value: c_double) {
    // decode name
    if let Ok(name) = std::ffi::CStr::from_ptr(name).to_str() {
    // lock progress.mutex
      if let Ok(mut locked_prog) = PROG_WRAPPER.lock() {

        // find progress bar
        if let Some(pb) = locked_prog.pbars.get(name) {
          pb.set_position((value * 1000.) as u64);
        }
        // if not found, create new progress bar
        else if let Some(prog) = locked_prog.progress.as_ref() {
          let pb = prog.add(ProgressBar::new(1000));
          if let Ok(style) = ProgressStyle::with_template("{spinner:.green} {msg} {bar:40.green/magenta} {percent:>3} % • {elapsed_precise:.yellow} • {eta_precise:.cyan}") {
            pb.set_style(
              style
              .progress_chars("━╸━")
            );
          }
          pb.set_position((value * 1000.0) as u64);
          let padded_name = format!("{: >16}", name);
          pb.set_message(padded_name);
          locked_prog.pbars.insert(name.to_string(), pb);
        }
        
        drop(locked_prog);
      }
    }
  }

  if options.tsonly {
    let gen = generator_generate(generator, options.input.as_str(), options.generator_args.clone().into(), progress_callback);
    let file = File::create(options.output.as_str());
    if let Ok(mut file) = file {
      for i in 0..gen.cuts.length {
        unsafe {
          if let Some(cut) = gen.cuts.cuts.offset(i as isize).as_ref() {
            if let Err(e) = file.write_all(format!("{},{}\n", cut.start, cut.end).as_bytes()) {
              eprintln!("Error: {}", e);
            }
          }
        }
      }
      if let Err(e) = file.flush() {
        eprintln!("Error: {}", e);
      }
    } else {
      panic!("Failed to create output file");
    }
    return gen.stats;
  }

  let gen = generator_generate(generator, options.input.as_str(), options.generator_args.clone().into(), progress_callback);
  
  render_render(render, options.input.as_str(), options.output.as_str(), gen.cuts, options.render_args.clone().into(), progress_callback);

  if let Ok(mut locked_prog) = PROG_WRAPPER.lock() {
    for (_, pb) in locked_prog.pbars.iter() {
      pb.finish_and_clear();
      if let Some(prog) = locked_prog.progress.as_ref() {
        prog.remove(pb);
      }
    }
    locked_prog.pbars.clear();
    drop(locked_prog);
  }
  gen.stats
}

fn process_files_in_dir(options: Options, generator: Library, render: Library) {
  if let Ok(files) = Path::new(&options.input).read_dir() {
    // map files to paths
    let files: Vec<_> = files.filter_map(|f| if let Ok(f) = f {Some(f.path())} else {None}).collect();
    let files: Vec<_> = files.into_iter().filter(|f| f.is_file()).collect();
    let files: Vec<_> = files.into_iter().filter(|f| tree_magic::from_filepath(f).starts_with("video")).collect();

    for file in files {
      if let Some(file_path) = file.to_str() {
        let output_path = if !options.output.is_empty() {
          let tmp = if let Some(tmp) = Path::new(&options.output).join(if let Some(filename) = file.file_name() {
            filename
          } else {
            panic!("Failed to get filename");
          }
        ).to_str() {
            tmp
          } else {
            "\\"
          }.to_string();
          tmp
        } else {
          let tmp = helper::get_automatic_path(if let Some(filename) = file.to_str() {
            filename
          } else {
            panic!("Failed to get filename");
          }, options.tsonly);
          tmp
        };

        let options = Options {
          input: file_path.to_string(),
          output: output_path.to_string(),
          ..options.clone()
        };

        run(&options, &generator, &render);
      }
    }
  }
}

fn process_single_file(options: Options, generator: Library, render: Library) {
  // start timer
  let start = std::time::Instant::now();
  let stats = run(&options, &generator, &render);
  // stop timer
  let end = std::time::Instant::now();
  print_stats([(options.input, options.output, stats)].to_vec(), end - start);
}

fn main() {
  // initialize progress bar
  let progress = MultiProgress::new();
  if let Ok(mut locked_prog) = PROG_WRAPPER.lock() {
    locked_prog.progress = Some(progress);
    // unlock progress bar
    drop(locked_prog);
  }

  let render = load_render();
  let render_version = module_version(&render);
  let generator = load_generator();
  let generator_version = module_version(&generator);

  greetings(render_version.as_str(), generator_version.as_str());

  let generator_args = module_manager::module_get_arguments(&generator);
  let render_args = module_manager::module_get_arguments(&render);

  let mut options = parse_args(generator_args, render_args);
  options = validate_args(options);

  if Path::new(options.input.as_str()).is_dir() {
    process_files_in_dir(options, generator, render);
  }
  else {
    process_single_file(options, generator, render);
  }
}