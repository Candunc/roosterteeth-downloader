use std::collections::HashMap;
use std::error::Error;
use std::process::{Command, Stdio};
use std::string::String;
use std::time::SystemTime;

use chrono::DateTime;
use directories::ProjectDirs;
use reqwest::Client;
use rusqlite::NO_PARAMS;
use serde_json::Value;

use crate::config::Config;
use crate::sql::Sql;

const DATABASE_PATH: &'static str = "/rtdownloader.sqlite";
const API_DOMAIN: &'static str = "https://svod-be.roosterteeth.com";
const API_EPISODES: &'static str = "https://svod-be.roosterteeth.com/api/v1/episodes?per_page=100&order=desc&page=1";

const LOGIN_URL: &'static str = "https://auth.roosterteeth.com/oauth/token";
const USERNAME: &'static str = "candunc";
const PASSWORD: &'static str = "Piplup6!";

#[derive(Serialize, Deserialize)]
struct Output {
	data: Vec<Value>,
}

#[derive(Serialize, Deserialize)]
struct Oauth {
	access_token: String,
	token_type: String,
	// i64 is used as sqlite uses exclusively signed numbers.
	expires_in: i64,
	created_at: i64
}

impl Oauth {
	fn expiry(&self) -> String {
		(self.created_at+self.expires_in).to_string()
	}
}

struct Video {
	uuid: String,
	show_title: String,
	season: String,
	number: String,
	title: String,
	slug: String,
	release: String
}

impl Video {
	fn new(raw: &Value) -> Self {
		let release = DateTime::parse_from_rfc3339(raw["attributes"]["sponsor_golive_at"].as_str().unwrap()).unwrap();
		Video {
			uuid:		raw["uuid"].as_str().unwrap().into(),
			show_title:	raw["attributes"]["show_slug"].as_str().unwrap().into(),
			season:		raw["attributes"]["season_number"].as_u64().unwrap().to_string(),
			number:		raw["attributes"]["number"].as_u64().unwrap().to_string(),
			title:		raw["attributes"]["title"].as_str().unwrap().into(),
			slug:		raw["attributes"]["slug"].as_str().unwrap().into(),
			release:	release.timestamp().to_string()
		}
	}

	fn to_sql(self) -> [String; 7] {
		[self.uuid, self.show_title, self.season, self.number, self.title, self.slug, self.release]
	}
}

pub struct Framework {
	config: Config,
	client: Client,
	sql: Sql,
	time: u64,
	new_metadata: bool
}

impl Framework {
	pub fn new() -> Result<Framework, Box<Error>> {
		let project = ProjectDirs::from("com", "Duncan Bristow", "rt-downloader").expect("Cannot find base directory");
		let path = project.data_dir();
		let dir = String::from(path.to_str().ok_or("Invalid path!")?);
		std::fs::create_dir_all(path)?;

		Ok(Framework {
			config: Config::load(dir.clone())?,
			client: Client::new(),
			sql: Sql::new(&format!("{}/{}",dir,DATABASE_PATH))?,
			time: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs(),
			new_metadata: false
		})
	}

	pub fn get_metadata(&mut self) -> Result<(), Box<Error>> {
		// Short circuit condition so we don't update multiple times in one session
		if !self.new_metadata {
			let body: Output = self.client.get(API_EPISODES).send()?.json()?;

			for i in &body.data {
				self.sql.insert_episode(&Video::new(&i).to_sql())?;
			}

			self.new_metadata = true;
		}
		Ok(())
	}

	fn get_m3u8(&self) -> Result<(), Box<Error>> {
		let access_token = self.login()?;

		let rows = self.sql.select_videos_new_m3u8()?;
		let mut contents: Output;
		let mut m3u8: String;

		for video in rows {
			contents = self.client.get(&format!("{}/api/v1/episodes/{}/videos",API_DOMAIN,video.slug)).header("authorization", format!("Bearer {};", access_token)).send()?.json()?;
			m3u8 = contents.data[0]["attributes"]["url"].as_str().unwrap().into();
			self.sql.update_m3u8(&video.uuid, &m3u8)?;
		}

		Ok(())
	}

	pub fn add_show(& self, show: &String, get_all: bool) -> Result<(), Box<Error>> {
		let mut stmt = self.sql.conn.prepare_cached("INSERT OR IGNORE INTO subscriptions (show_title, from_date) VALUES (?1, ?2)")?;

		if get_all {
			stmt.execute(&[show, "0"])?;
			self.get_show(show)?;
			self.sql.conn.execute("UPDATE videos SET m3u8='GET' WHERE m3u8 IS NULL AND show_title=?1", &[show])?;
		} else {
			let time = self.time.to_string();
			stmt.execute(&[show, &time])?;
			self.sql.conn.execute("UPDATE videos SET m3u8='GET' WHERE m3u8 IS NULL AND show_title=?1 AND release > ?2", &[show, &time])?;
		}

		Ok(())
	}

	// Needs to be mutable for self.get_m3u8()
	pub fn download_new(&mut self) -> Result<(), Box<Error>> {
		// Ensure our metadata is up to date.
		self.get_metadata()?;

		// Prepare to download everything that meets our criteria
		let mut stmt = self.sql.conn.prepare("UPDATE videos SET m3u8='GET' WHERE m3u8 IS NULL AND downloaded=0 AND show_title=?1 AND release >?2")?;

		let subscriptions = self.sql.select_subscriptions()?;
		for show in subscriptions {
			stmt.execute(&[&show.title, &show.from_date.to_string()])?;
		}

		// Actually get the m3u8 files for everything we want to download
		self.get_m3u8()?;

		let queue = self.sql.select_for_download()?;
		for episode in queue {
			// Todo: pipe stdout to current terminal
			std::fs::create_dir_all(format!("{}/{}", self.config.media_directory, episode.show_title))?;
			Command::new("ffmpeg").stdout(Stdio::piped()).args(&["-i", &episode.m3u8, "-vcodec", "copy", "-c:a", "copy", &format!("{}/{}/S{}E{}.mp4", self.config.media_directory, episode.show_title, episode.season, episode.number)])
				.spawn().expect("error!");
			self.sql.update_downloaded(&episode.uuid)?;
		}

		Ok(())
	}

	pub fn clean_database(&self) -> Result<(), Box<Error>> {
		// The idea of this function is to remove any listings in 'videos' that do not
		// have a show_title of a subscribed show. This is a byproduct of the 'wasteful'
		// update_metadata where we just shove everything into the database.
		self.sql.conn.execute("DELETE FROM videos WHERE show_title NOT IN (SELECT show_title FROM subscriptions)", NO_PARAMS)?;

		Ok(())
	}

	fn get_show(&self, show: &String) -> Result<(), Box<Error>> {
		// This should iterate through all past seasons of the show in the case that add_show()->get_all is true

		let seasons: Output = self.client.get(&format!("{}/api/v1/shows/{}/seasons?order=asc",API_DOMAIN,show)).send()?.json()?;
		for season in &seasons.data {
			let episodes: Output = self.client.get(&format!("{}{}",API_DOMAIN,season["links"]["episodes"].as_str().unwrap())).send()?.json()?;
			for i in &episodes.data {
				self.sql.insert_episode(&Video::new(&i).to_sql())?;
			}
		}

		Ok(())
	}

	fn login(&self) -> Result<String, Box<Error>> {
		let mut stmt = self.sql.conn.prepare("SELECT access_token, expiry FROM authorization")?;
		let mut rows = stmt.query(NO_PARAMS)?;
		let row = rows.next().ok_or("unreachable")??;

		let access_token: String = row.get(0);
		let expiry: i64 = row.get(1);

		if (expiry as u64) > self.time {
			Ok(access_token)
		} else {
			let mut map = HashMap::new();
			map.insert("client_id", "4338d2b4bdc8db1239360f28e72f0d9ddb1fd01e7a38fbb07b4b1f4ba4564cc5");
			map.insert("grant_type", "password");
			map.insert("password", PASSWORD);
			map.insert("scope", "user public");
			map.insert("username", USERNAME);

			let response: Oauth = self.client.post(LOGIN_URL).json(&map).send()?.json()?;

			self.sql.conn.execute("UPDATE authorization SET access_token=?1, expiry=?2 WHERE id=0", &[&response.access_token, &response.expiry()])?;

			Ok(response.access_token)
		}
	}
}
