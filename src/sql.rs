use std::error::Error;

use rusqlite::{Connection, NO_PARAMS, types::ToSql};

#[derive(Debug)]
pub struct VideoRow {
	pub uuid: String,
	pub show_title: String,
	pub season: i64,
	pub number: i64,
	pub title: String,
	pub slug: String,
	pub release: i64,
	pub m3u8: String
}

pub struct SubscriptionRow {
	pub title: String,
	pub from_date: i64
}

pub struct Sql {
	pub conn: Connection
}

impl Sql {
	pub fn new(path: &'static str) -> Result<Sql, Box<Error>> {
		let conn = Connection::open(path)?;

		// Initialize database if does it doesn't exist
		conn.execute("CREATE TABLE IF NOT EXISTS videos (uuid TEXT PRIMARY KEY, show_title TEXT NOT NULL, season INTEGER NOT NULL, number INTEGER NOT NULL, title TEXT NOT NULL, slug TEXT NOT NULL, release INTEGER NOT NULL, m3u8 TEXT, downloaded INTEGER DEFAULT 0)", NO_PARAMS)?;
		conn.execute("CREATE TABLE IF NOT EXISTS authorization (id INTEGER PRIMARY KEY CHECK (id = 0), access_token TEXT NOT NULL, expiry INTEGER NOT NULL)", NO_PARAMS)?;
		conn.execute("INSERT OR IGNORE INTO authorization (id, access_token, expiry) VALUES (0, '', 0)", NO_PARAMS)?;
		conn.execute("CREATE TABLE IF NOT EXISTS subscriptions (show_title TEXT PRIMARY KEY, from_date INTEGER NOT NULL)", NO_PARAMS)?;

		Ok(Sql {
			conn: conn
		})
	}

	fn map_video(&self, statement: &'static str, param: &[&(dyn ToSql + 'static)]) -> Result<Vec<VideoRow>, Box<Error>> {
		let mut stmt = self.conn.prepare_cached(statement)?;

		let iter = stmt.query_map(param, |row| VideoRow {
			uuid: row.get(0),
			show_title: row.get(1),
			season: row.get(2),
			number: row.get(3),
			title: row.get(4),
			slug: row.get(5),
			release: row.get(6),
			m3u8: row.get(7)
		})?;

		let mut output: Vec<VideoRow> = Vec::new();
		for i in iter {
			output.push(i?)
		}

		Ok(output)
	}

	pub fn select_for_download(&self) -> Result<Vec<VideoRow>, Box<Error>> {
		Ok(self.map_video("SELECT uuid, show_title, season, number, title, slug, release, m3u8 FROM videos WHERE m3u8 NOT NULL AND downloaded=0", NO_PARAMS)?)
	}

	/*
	pub fn select_videos(&self, uuid: &String) -> Result<Vec<VideoRow>, Box<Error>> {
		Ok(self.map_video("SELECT uuid, show_title, season, number, title, slug, release, m3u8 FROM videos WHERE uuid=?1", &[uuid])?)
	}*/

	pub fn select_videos_new_m3u8(&self) -> Result<Vec<VideoRow>, Box<Error>> {
		Ok(self.map_video("SELECT uuid, show_title, season, number, title, slug, release, m3u8 FROM videos WHERE m3u8='GET'", NO_PARAMS)?)
	}

	pub fn select_subscriptions(&self) -> Result<Vec<SubscriptionRow>, Box<Error>> {
		let mut stmt = self.conn.prepare_cached("SELECT show_title, from_date FROM subscriptions")?;

		let iter = stmt.query_map(NO_PARAMS, |row| SubscriptionRow {
			title: row.get(0),
			from_date: row.get(1)
		})?;

		let mut output: Vec<SubscriptionRow> = Vec::new();
		for i in iter {
			output.push(i?);
		}

		Ok(output)
	}

	pub fn update_m3u8(&self, uuid: &String, m3u8: &String) -> Result<(), Box<Error>> {
		let mut stmt = self.conn.prepare_cached("UPDATE videos SET m3u8=?1 WHERE uuid=?2")?;
		stmt.execute(&[m3u8, uuid])?;

		Ok(())
	}

	pub fn update_downloaded(&self, uuid: &String) -> Result<(), Box<Error>> {
		let mut stmt = self.conn.prepare_cached("UPDATE videos SET downloaded=1 WHERE uuid=?1")?;
		stmt.execute(&[uuid])?;

		Ok(())
	}

	pub fn insert_episode(&self, params: &[String; 7]) -> Result<(), Box<Error>> {
		let mut stmt = self.conn.prepare_cached("INSERT OR IGNORE INTO videos (uuid, show_title, season, number, title, slug, release) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)")?;
		stmt.execute(params)?;

		Ok(())
	}
}
