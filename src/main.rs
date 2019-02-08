#[macro_use]
extern crate serde_derive;

use std::env;

mod config;
mod framework;
mod sql;
use framework::Framework;

fn main() -> Result<(), Box<std::error::Error>>{
	let mut framework = Framework::new()?;

	let mut args: Vec<String> = Vec::new();

	for arg in env::args() {
		args.push(arg);
	}

	for i in 1..args.len() {
		if args[i] == "--add" {
			framework.add_show(&args[i+1], true)?;
			framework.download_new()?;
		} else if args[i] == "--cron" {
			framework.download_new()?;
		} else if args[i] == "--clean" {
			framework.clean_database()?;
		}
	}

	Ok(())
}
