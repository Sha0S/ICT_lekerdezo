#![allow(clippy::assigning_clones)]

use ini::Ini;
use std::path::PathBuf;

#[derive(Default)]
pub struct Config {
    pub server: String,
    pub database: String,
    pub password: String,
    pub username: String,

    pub log_viewer: String,
}

impl Config {
    pub fn read(path: PathBuf) -> anyhow::Result<Config> {
        let mut c = Config {
            log_viewer: ".\\ict_lr.exe".to_owned(),
            ..Default::default()
        };

        if let Ok(config) = Ini::load_from_file(path.clone()) {
            if let Some(jvserver) = config.section(Some("JVSERVER")) {
                // mandatory fields:
                if let Some(server) = jvserver.get("SERVER") {
                    c.server = server.to_owned();
                }
                if let Some(password) = jvserver.get("PASSWORD") {
                    c.password = password.to_owned();
                }
                if let Some(username) = jvserver.get("USERNAME") {
                    c.username = username.to_owned();
                }
                if let Some(database) = jvserver.get("DATABASE") {
                    c.database = database.to_owned();
                }

                if c.server.is_empty()
                    || c.password.is_empty()
                    || c.username.is_empty()
                    || c.database.is_empty()
                {
                    return Err(anyhow::Error::msg(
                        "ER: Missing fields from configuration file!",
                    ));
                }
            } else {
                return Err(anyhow::Error::msg("ER: Could not find [JVSERVER] field!"));
            }

            if let Some(app) = config.section(Some("APP")) {
                if let Some(viewer) = app.get("VIEWER") {
                    c.log_viewer = viewer.to_owned();
                    println!("using {viewer}");
                }
            }
        } else {
            return Err(anyhow::Error::msg(format!(
                "ER: Could not read configuration file! [{}]",
                path.display()
            )));
        }

        Ok(c)
    }
}
