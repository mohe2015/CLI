extern crate tree_magic;

use std::path::Path;

use clap::{Arg, ArgAction, Command};

use crate::module_manager::{ArgumentList, ArgumentResult};
use crate::{printer::{raise_error, print_dir_not_empty_warning}, helper::get_automatic_path};

#[derive(Clone)]
pub struct Options {
  pub input: String,
  pub output: String,
  pub tsonly: bool,
  pub generator_args: Vec<ArgumentResult>,
  pub render_args: Vec<ArgumentResult>,
}

pub fn parse_args(generator_args: ArgumentList, render_args: ArgumentList) -> Options {
  let mut options: Options = Options {
    input: String::new(),
    output: String::new(),
    tsonly: false,
    generator_args: Vec::new(),
    render_args: Vec::new(),
  };

  {
    // ensure that generator and render arguments are not overlapping
    let mut longs = Vec::new();
    let mut shorts = Vec::new();
    for arg in &generator_args {
      if longs.contains(&arg.long) || shorts.contains(&arg.short) {
        raise_error("Argument names are overlapping.");
      }
      longs.push(arg.long.clone());
      shorts.push(arg.short);
    }
    for arg in &render_args {
      if longs.contains(&arg.long) || shorts.contains(&arg.short) {
        raise_error("Argument names are overlapping.");
      }
      longs.push(arg.long.clone());
      shorts.push(arg.short);
    }
  }

  let mut command = Command::new("lecturecut")
    .about("LectureCut is a tool to remove silence from videos.

    It uses WebRTC's VAD to detect silence and ffmpeg to transcode the video.
    To speed up transcoding, a form of smart encoding is employed. This means
    that the video is split into segments and only the segments that need to be
    cut are transcoded. This results in a much faster transcoding process, but
    the output video will have a slightly lower quality than the input video.")
    .arg(Arg::new("input").short('i').long("input").help("The video file to process").required(true))
    .arg(Arg::new("output").short('o').long("output").help("The output file. If not specified, LectureCut will automatically generate a name."))
    .arg(Arg::new("tsonly").long("tsonly").help("Only output the timestamps of the cuts. This is useful for debugging purposes or if you want to use the cuts in another program.").action(ArgAction::SetTrue));

  command = command.next_help_heading("Generator Arguments");
  for arg in &generator_args {
    let mut arrg = Arg::new(arg.long.clone()).long(arg.long.clone()).help(arg.description.clone()).required(arg.required);
    if arg.short != '\0' {
      arrg = arrg.short(arg.short);
    }
    if arg.is_flag {
      arrg = arrg.action(ArgAction::SetTrue);
    }
    command = command.arg(arrg);
  }

  command = command.next_help_heading("Render Arguments");
  for arg in &render_args {
    let mut arrg = Arg::new(arg.long.clone()).long(arg.long.clone()).help(arg.description.clone()).required(arg.required);
    if arg.short != '\0' {
      arrg = arrg.short(arg.short);
    }
    if arg.is_flag {
      arrg = arrg.action(ArgAction::SetTrue);
    }
    command = command.arg(arrg);
  }

  // parse arguments
  let matches = command.get_matches();

  // unpack arguments
  options.input = matches.get_one::<String>("input").unwrap().to_string();
  if let Some(output) = matches.get_one::<String>("output") {
    options.output = output.to_string();
  }
  options.tsonly = matches.get_flag("tsonly");

  // unpack generator arguments
  for arg in &generator_args {
    match arg.is_flag {
      true => {
        if matches.get_flag(&arg.long) {
          options.generator_args.push(ArgumentResult {
            long: arg.long.clone(),
            value: "true".to_string(),
          });
        }
      },
      false => {
        if let Some(value) = matches.get_one::<String>(&arg.long) {
          options.generator_args.push(ArgumentResult {
            long: arg.long.clone(),
            value: value.to_string(),
          });
        }
      }
    }
  }

  // unpack render arguments
  for arg in &render_args {
    if let Some(value) = matches.get_one::<String>(&arg.long) {
      options.render_args.push(ArgumentResult {
        long: arg.long.clone(),
        value: value.to_string(),
      });
    }
  }

  // because windows is seemingly designed by a 5 year old
  // we need to replace trailing double quotes with a backslash
  // ( see https://bugs.python.org/msg364246 )
  if cfg!(windows) {
    options.input = options.input.replace('\"', "\\\"");
    options.output = options.output.replace('\"', "\\\"");
  }

  options
}


pub fn validate_args(options: Options) -> Options {
  let mut changed_options = options.clone();

  // input validation
  let input_path: &Path = Path::new(options.input.as_str());
  let input_is_file: bool = input_path.is_file();
  let input_is_dir: bool = input_path.is_dir();

  // check if input is a file or a directory (os.path.exists)
  if !input_path.exists() {
    raise_error("Input file or directory does not exist.");
  }
  if !input_is_file && !input_path.is_dir() {
    raise_error("Input needs to be a file or a directory.");
  }
  // check filetype using magicbytes if file
  if input_is_file {
    let filetype: String = tree_magic::from_filepath(input_path);
    // if not video, warn user
    if !filetype.starts_with("video") {
      println!("Warning: Input file is not a video file.");
    }
  }

  if cfg!(windows) {
    changed_options.input = options.input.replace('/', "\\");
  }

  // output validation
  if !options.output.is_empty() {
    // may not contain any illegal characters for paths
    // if is windows
    let mut illegal_chars: String = "".to_string();
    if cfg!(windows) {
      illegal_chars += r#"<>"|?*"#;
      for i in 0..32 {
        illegal_chars += &(i as u8 as char).to_string();
      }
    } else {
      illegal_chars = (0 as char).to_string();
    }
    for c in illegal_chars.chars() {
      if options.output.contains(c) {
        println!("Output path contains illegal characters. ({})", c);
        
        raise_error("Output path contains illegal characters.");
      }
    }
    let output_path: &Path = Path::new(options.output.as_str());
    if input_is_dir {
      // if input is directory, output must be directory
      if output_path.exists() {
        if !output_path.is_dir() {
          raise_error("Output path needs to be a directory.");
        }
        // if output directory exists, it must be empty
        if let Ok(dir) = output_path.read_dir() {
          if dir.count() > 0 {
            print_dir_not_empty_warning();
          }
        } else {
          raise_error("Could not read output directory.");
        }
      }
      else {
        // try to create output directory raise_error if it fails
        if std::fs::create_dir(output_path).is_err() {
          raise_error("Could not create output directory.");
        }
      }
    } else {
      // if input is file, output must be file
      if output_path.exists() {
        raise_error("Output file already exists.")
      }
    }
  }
  else if !input_is_dir {
    changed_options.output = get_automatic_path(options.input.as_str(), options.tsonly);
    if Path::new(changed_options.output.as_str()).exists() {
      raise_error("Output file already exists.")
    }
  }

  changed_options
}