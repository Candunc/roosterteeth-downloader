use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::string::String;

use ::toml;

const FILENAME: &'static str = "config.toml";

#[derive(Serialize, Deserialize)]
pub struct Config {
	conf_directory: String,
	pub media_directory: String
}

// Alright, this is convoluted as we _have_ to return a config file, even if it's the default
// Hence the weird 'hot potato' with path being passed to children.
impl Config {
	pub fn load(path: String) -> Result<Config, Box<Error>> {
		let file = File::open(format!("{}/{}",path,FILENAME));
		if file.is_ok() {
			Ok(Config::read_file(file.unwrap(), path)?)
		} else {
			Ok(Config::new(path)?)
		}
	}

	fn new(path: String) -> Result<Config, Box<Error>> {
		println!("Config file does not exist or is corrupt, default directory is {}", path);
		let conf = Config {
			conf_directory: path.clone(),
			media_directory: path
		};
		
		conf.write_file()?;
		Ok(conf)
	}

	fn read_file(mut file: File, path: String) -> Result<Config, Box<Error>> {
		let mut contents = String::new();
		file.read_to_string(&mut contents)?;
		let conf: Config = match toml::from_str(&contents) {
			Ok(f) => f,
			_ => Config::new(path)?
		};

		Ok(conf)
	}

	pub fn write_file(&self) -> Result<(), Box<Error>> {
		let mut file = File::create(format!("{}/{}",self.conf_directory,FILENAME))?;
		file.write_all(toml::to_string(&self)?.as_bytes())?;
		Ok(())
	}
}
