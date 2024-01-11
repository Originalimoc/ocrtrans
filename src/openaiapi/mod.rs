use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::translator::TranslateRequest;

pub(crate) fn new_translate_json(request: &TranslateRequest) -> Value {
	let content = if request.content.is_empty() {
		"これはテスト文です。"
	} else {
		&request.content
	};
	serde_json::json!({
		"mode": "instruct",
		"instruction_template": "Gorilla_Trans",
		"max_tokens": 200,
		"temperature": 0.66, //1.35, intel7bv3-2
		"top_p": 0.42, //0.3
		"top_k": 69, //42
		"repetition_penalty": 1,
		"stream": true,
		"messages": vec![
			TranslateSrcPayload {
				role: "user".to_string(),
				content: format!(
r#######"
Translate task: Here is some {} text quoted in ``` to translate to {}, which are ONLY BEFORE and NOT include backtick quote ```, here text:
```
{}
```

Requirement: Output result translated text directly with no additional info. IF there are additional info and sentence you want to say DO NOT reply. DO NOT add ANY additional info in front or in end of the translation. Eg: If it's JP to EN, when I say "Aishiteru", you ONLY reply "I love you."
"#######,
					request.src_lang, request.target_lang, content
				)
			},
		],
	})
}

#[derive(Debug, serde::Serialize)]
struct TranslateSrcPayload {
    role: String,
    content: String,
}

// Define a struct to deserialize the relevant parts of the JSON.
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Completion {
    pub choices: Vec<Choice>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Choice {
    index: usize,
    pub message: Message,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Message {
    pub content: String,
}
