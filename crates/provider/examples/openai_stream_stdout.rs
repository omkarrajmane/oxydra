use std::collections::BTreeMap;
use std::io::{self, Write};

use provider::OpenAIProvider;
use types::{
    Context, Message, MessageRole, ModelCatalog, ModelId, Provider as _, ProviderId, StreamItem,
    UsageUpdate,
};

const DEFAULT_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const DEFAULT_OPENROUTER_MODEL: &str = "google/gemini-2.0-flash-001";
const INITIAL_BUDGET_MICROUSD: u64 = 10_000;
const DEFAULT_RUNTIME_COST_PER_M_TOKENS_MICROUSD: u64 = 10_000_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (policy_mode, prompt) = parse_args();
    let prompt = prompt.unwrap_or_else(|| {
        "Say hello and name one benefit of Rust async in one sentence.".to_owned()
    });

    let api_key = std::env::var("OPENAI_API_KEY").expect("set OPENAI_API_KEY to run this example");
    let mut catalog =
        ModelCatalog::from_pinned_snapshot().expect("pinned model catalog should parse");
    let (base_url, model) = if policy_mode {
        let configured_base_url = std::env::var("OPENAI_BASE_URL")
            .unwrap_or_else(|_| DEFAULT_OPENROUTER_BASE_URL.to_owned());
        (
            normalize_openrouter_base_url(&configured_base_url),
            std::env::var("OPENAI_MODEL").unwrap_or_else(|_| DEFAULT_OPENROUTER_MODEL.to_owned()),
        )
    } else {
        (String::new(), "gpt-4o-mini".to_owned())
    };

    if policy_mode {
        ensure_openai_catalog_model(&mut catalog, &model);
    }

    let provider = OpenAIProvider::new(
        ProviderId::from("openai"),
        ProviderId::from("openai"),
        api_key,
        base_url,
        BTreeMap::new(),
        catalog,
    );

    let context = Context {
        provider: ProviderId::from("openai"),
        model: ModelId::from(model),
        tools: vec![],
        messages: vec![Message {
            role: MessageRole::User,
            content: Some(prompt),
            tool_calls: vec![],
            tool_call_id: None,
            attachments: Vec::new(),
        }],
    };

    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            if policy_mode {
                run_policy_mode(&provider, &context).await?;
            } else {
                run_default_stream_mode(&provider, &context).await?;
            }
            Ok::<(), Box<dyn std::error::Error>>(())
        })?;

    Ok(())
}

fn parse_args() -> (bool, Option<String>) {
    let mut policy_mode = false;
    let mut prompt_parts = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg == "--policy" {
            policy_mode = true;
        } else {
            prompt_parts.push(arg);
        }
    }
    let prompt = if prompt_parts.is_empty() {
        None
    } else {
        Some(prompt_parts.join(" "))
    };
    (policy_mode, prompt)
}

async fn run_default_stream_mode(
    provider: &OpenAIProvider,
    context: &Context,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = provider.stream(context, 64).await?;
    while let Some(item) = stream.recv().await {
        match item? {
            StreamItem::Text(text) => {
                print!("{text}");
                io::stdout().flush()?;
            }
            StreamItem::ReasoningDelta(reasoning) => {
                eprintln!("\n[reasoning_delta] {reasoning}");
            }
            StreamItem::ToolCallDelta(delta) => {
                eprintln!("\n[tool_call_delta] {}", serde_json::to_string(&delta)?);
            }
            StreamItem::UsageUpdate(usage) => {
                eprintln!("\n[usage_update] {}", serde_json::to_string(&usage)?);
            }
            StreamItem::ConnectionLost(message) => {
                eprintln!("\n[connection_lost] {message}");
            }
            StreamItem::FinishReason(reason) => {
                eprintln!("\n[finish_reason] {reason}");
            }
            StreamItem::Progress(_) => {}
            StreamItem::Media(_) => {}
            StreamItem::PolicyEvent(event) => {
                eprintln!("\n[policy_event] {:?}", event);
            }
        }
    }
    println!();
    Ok(())
}

async fn run_policy_mode(
    provider: &OpenAIProvider,
    context: &Context,
) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("=== Policy Enforcement Mode ===");
    eprintln!("Budget: $0.01 ({INITIAL_BUDGET_MICROUSD} micro-USD)");
    eprintln!("Max turns: 2\n");

    let mut stream = provider.stream(context, 64).await?;
    let mut text = String::new();
    let mut finish_reason = String::from("unknown");
    let mut turn_cost_microusd = 0u64;

    while let Some(item) = stream.recv().await {
        match item? {
            StreamItem::Text(delta) => {
                text.push_str(&delta);
            }
            StreamItem::UsageUpdate(usage) => {
                turn_cost_microusd = estimate_turn_cost_microusd(&usage);
            }
            StreamItem::FinishReason(reason) => {
                finish_reason = reason;
            }
            StreamItem::ConnectionLost(message) => {
                eprintln!("\n[connection_lost] {message}");
            }
            StreamItem::ReasoningDelta(reasoning) => {
                eprintln!("\n[reasoning_delta] {reasoning}");
            }
            StreamItem::ToolCallDelta(delta) => {
                eprintln!("\n[tool_call_delta] {}", serde_json::to_string(&delta)?);
            }
            StreamItem::PolicyEvent(event) => {
                eprintln!("\n[policy_event] {:?}", event);
            }
            StreamItem::Progress(_) | StreamItem::Media(_) => {}
        }
    }

    let remaining_budget = INITIAL_BUDGET_MICROUSD.saturating_sub(turn_cost_microusd);
    println!("[Turn 1]");
    println!(
        "Response: {}",
        if text.is_empty() {
            "<empty response>"
        } else {
            text.as_str()
        }
    );
    println!(
        "[Budget] Cost: {turn_cost_microusd} micro-USD, Remaining: {remaining_budget} micro-USD"
    );
    println!("[Stop reason: {finish_reason}]");

    Ok(())
}

fn estimate_turn_cost_microusd(usage: &UsageUpdate) -> u64 {
    let total_tokens = usage.total_tokens.or_else(|| {
        let prompt = usage.prompt_tokens.unwrap_or(0);
        let completion = usage.completion_tokens.unwrap_or(0);
        let aggregated = prompt.saturating_add(completion);
        (aggregated > 0).then_some(aggregated)
    });
    total_tokens
        .map(|tokens| tokens * DEFAULT_RUNTIME_COST_PER_M_TOKENS_MICROUSD / 1_000_000)
        .unwrap_or(0)
}

fn ensure_openai_catalog_model(catalog: &mut ModelCatalog, model: &str) {
    let has_model = catalog
        .providers
        .get("openai")
        .is_some_and(|provider| provider.models.contains_key(model));
    if has_model {
        return;
    }

    let model_without_prefix = model.rsplit('/').next().unwrap_or(model);
    let model_without_revision = model_without_prefix.trim_end_matches("-001");
    let candidate_ids = [model, model_without_prefix, model_without_revision];

    let source_model = candidate_ids.iter().find_map(|candidate| {
        catalog
            .providers
            .get("openai")
            .and_then(|provider| provider.models.get(*candidate))
            .cloned()
            .or_else(|| {
                catalog
                    .providers
                    .get("google")
                    .and_then(|provider| provider.models.get(*candidate))
                    .cloned()
            })
    });

    if let Some(mut descriptor) = source_model
        && let Some(openai_provider) = catalog.providers.get_mut("openai")
    {
        descriptor.id = model.to_owned();
        descriptor.name = model.to_owned();
        openai_provider.models.insert(model.to_owned(), descriptor);
    }
}

fn normalize_openrouter_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if let Some(without_v1) = trimmed.strip_suffix("/v1") {
        without_v1.to_owned()
    } else {
        trimmed.to_owned()
    }
}
