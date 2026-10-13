use serde::Serialize;
use std::io::{self, Write};

#[derive(Serialize, Clone, Debug)]
pub struct ProgressEvent {
    pub percent: u32,
    pub stage: String,
    pub message: String,
    pub status: String,
}

pub struct Reporter {
    json_mode: bool,
}

impl Reporter {
    pub fn new(json_mode: bool) -> Self {
        Self { json_mode }
    }

    pub fn report(&self, percent: u32, stage: &str, message: &str, status: &str) {
        if self.json_mode {
            let event = ProgressEvent {
                percent,
                stage: stage.to_string(),
                message: message.to_string(),
                status: status.to_string(),
            };
            if let Ok(json) = serde_json::to_string(&event) {
                println!("{}", json);
                let _ = io::stdout().flush();
            }
        } else {
            let color = match status {
                "error" => "\x1b[31m",
                "success" => "\x1b[32m",
                _ => "\x1b[36m",
            };
            let reset = "\x1b[0m";
            println!(
                "{}[{:3}%] [{}] {}{}",
                color, percent, stage, message, reset
            );
            let _ = io::stdout().flush();
        }
    }
}
