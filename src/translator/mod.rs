mod openaiapi;
use openaiapi::{Completion, new_translate_json};
use std::io::Write;
use reqwest_eventsource::{Event, EventSource};
use reqwest::Client;
use anyhow::Result;
use serde_json::Value;
use futures::StreamExt;

pub(crate) async fn translate_openai(api_endpoint: &str, api_key: &Option<String>, request: &TranslateRequest, steaming_output_async_channel: Option<tokio::sync::mpsc::Sender<String>>, steaming_output_sync_channel: Option<std::sync::mpsc::Sender<String>>) -> Result<String> {
	let payload = new_translate_json(request);
	 let http_client = if let Some(api_key) = api_key {
		Client::new()
			.post(api_endpoint)
			.bearer_auth(api_key)
			.json(&payload)
	} else {
		Client::new()
			.post(api_endpoint)
			.json(&payload)
	};
	let mut es = EventSource::new(http_client)?;
	let mut concatenated_result = String::new();
	while let Some(event) = es.next().await {
		match event {
			Ok(Event::Open) => {
				// println!("\nStreaming Output:")
			},
			Ok(Event::Message(message)) => {
				let value: Value = serde_json::from_str(&message.data).unwrap();
				let completion: Completion = serde_json::from_value(value).unwrap();
				// Loop through the choices and concatenate the message contents
				for choice in completion.choices {
					let content = choice.message.content;
					if !content.replace(['\n', ' '], "").is_empty() {
						if let Some(ref channel) = steaming_output_sync_channel {
							if let Err(e) = channel.send(content.clone()) {
								eprintln!("steaming_output_sync_channel error: {}", e);
							}
						}
						if let Some(ref channel) = steaming_output_async_channel {
							if let Err(e) = channel.send(content.clone()).await {
								eprintln!("steaming_output_async_channel error: {}", e);
							}
						}
						let _ = std::io::stdout().flush();
						concatenated_result.push_str(&content);
					}
				}
			},
			Err(err) => {
				es.close();
				if let reqwest_eventsource::Error::StreamEnded = err {
					// println!("\n/Streaming Output Done.\n")
				} else {
					return Err(err)?;
				}
			}
		}
	}
	Ok(concatenated_result)
}

pub struct TranslateRequest {
	pub content: String,
	pub src_lang: String,
	pub target_lang: String,
}

impl TranslateRequest {
	pub fn new(content: &str, src_lang: &str, target_lang: &str) -> Self {
		Self {
			content: content.to_string(),
			src_lang: src_lang.to_string(),
			target_lang: target_lang.to_string(),
		}
	}
}
