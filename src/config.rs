use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct Config {
  pub port: Option<u16>,
  pub bind: Option<String>,
  pub apps: Option<HashMap<String, App>>,
}

#[derive(Debug, Deserialize)]
pub struct App {
  pub filename: String,
  pub scale: Option<u32>,
}
