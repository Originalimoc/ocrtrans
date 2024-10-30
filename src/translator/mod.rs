use openai::chat::{ChatCompletionMessage, ChatCompletionMessageRole, ChatCompletionDelta};
use std::io::Write;
use anyhow::Result;

pub(crate) async fn translate_openai(request: &TranslateRequest, steaming_output_async_channel: Option<tokio::sync::mpsc::Sender<String>>, steaming_output_sync_channel: Option<std::sync::mpsc::Sender<String>>) -> Result<String> {
	let mut messages = vec![ChatCompletionMessage {
        role: ChatCompletionMessageRole::System,
        content: Some("You are a multilingual translator, but mainly focused on video games or visual novels in Japanese.".to_string()),
        name: None,
        function_call: None,
    }];

	messages.push(ChatCompletionMessage {
		role: ChatCompletionMessageRole::User,
		content: Some(format!("Translate ```\n{}\n``` to {}, reply translation only", request.content, request.target_lang)),
		name: None,
		function_call: None,
	});

	let mut translation_result_stream = ChatCompletionDelta::builder("gpt-4o", messages.clone())
		.create_stream()
		.await
		.unwrap();

	let mut concatenated_result = String::new();

	while let Some(delta) = translation_result_stream.recv().await {
		let choice = &delta.choices[0];
		// if let Some(role) = &choice.delta.role {
		// 	print!("{:#?}: ", role);
		// }
		if let Some(content) = &choice.delta.content {
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
				concatenated_result.push_str(content);
			}
		}
		// if let Some(_) = &choice.finish_reason {
		// 	print!("\n");
		// }
	}

	concatenated_result.push('\n');
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
