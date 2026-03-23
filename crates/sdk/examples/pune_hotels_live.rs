use std::{collections::BTreeMap, env, sync::Arc, time::Duration};

use gateway::{GatewayServer, GatewayTurnRunner, RuntimeGatewayTurnRunner};
use provider::OpenAIProvider;
use runtime::{AgentRuntime, RuntimeLimits};
use sdk::{ClientConfig, OxydraClient};
use serde_json::Value;
use types::{ModelCatalog, ModelId, ProviderId, ProviderSelection};

fn openrouter_compatible_catalog(model: &str) -> Result<ModelCatalog, Box<dyn std::error::Error>> {
    let mut snapshot: Value = serde_json::from_str(ModelCatalog::pinned_snapshot_json())?;
    if let Some((provider_prefix, base_model)) = model.split_once('/') {
        let alias_descriptor = snapshot
            .get(provider_prefix)
            .and_then(|p| p.get("models"))
            .and_then(|m| m.get(base_model))
            .cloned();
        if let Some(openai_models) = snapshot
            .get_mut("openai")
            .and_then(|p| p.get_mut("models"))
            .and_then(Value::as_object_mut)
            && !openai_models.contains_key(model)
            && let Some(alias_descriptor) = alias_descriptor
        {
            openai_models.insert(model.to_owned(), alias_descriptor);
        }
    }

    let snapshot_str = serde_json::to_string(&snapshot)?;
    let base_catalog = ModelCatalog::from_snapshot_str(&snapshot_str)?;
    let overrides = serde_json::from_str(ModelCatalog::pinned_overrides_json())?;
    Ok(base_catalog.with_caps_overrides(overrides))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("OPENROUTER_API_KEY")
        .or_else(|_| env::var("OPENAI_API_KEY"))
        .map_err(|_| "set OPENROUTER_API_KEY (or OPENAI_API_KEY) before running this example")?;

    let model = env::var("OPENROUTER_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_owned());
    let model_id = ModelId::from(model.as_str());
    let provider_id = ProviderId::from("openai");

    let catalog = openrouter_compatible_catalog(&model)?;
    catalog.validate(&provider_id, &model_id)?;

    let llm = OpenAIProvider::new(
        provider_id.clone(),
        provider_id.clone(),
        api_key,
        "https://openrouter.ai/api".to_owned(),
        BTreeMap::new(),
        catalog,
    );

    let tools_bootstrap = tools::bootstrap_runtime_tools(None, None, None).await;
    let selection = ProviderSelection {
        provider: provider_id.clone(),
        model: model_id.clone(),
    };

    let runtime = Arc::new(
        AgentRuntime::new(
            Box::new(llm),
            tools_bootstrap.registry,
            RuntimeLimits {
                turn_timeout: Duration::from_secs(120),
                max_turns: 10,
                max_cost: None,
                ..RuntimeLimits::default()
            },
        )
        .with_primary_selection(selection.clone()),
    );

    let turn_runner: Arc<dyn GatewayTurnRunner> = Arc::new(RuntimeGatewayTurnRunner::new(
        runtime,
        selection,
        BTreeMap::new(),
    ));

    let gateway = Arc::new(GatewayServer::new(Arc::clone(&turn_runner)));
    let client = OxydraClient::builder()
        .config(ClientConfig::new("hotel_hunter").with_agent_name("default"))
        .gateway(gateway)
        .turn_runner(turn_runner)
        .build()?;

    let prompt = r#"
You are a travel research agent.

Task: find hotels in Pune for check-in on 30 March 2026 with budget INR 5,000-10,000 per night.

Execution requirements:
- You must use web_search at least 3 times with different queries (by city, by area, by booking portal pages).
- You must use web_fetch on top candidates to validate title/price/rating text.
- If exact 30-Mar-2026 availability is not published, provide best current listings in that price range and mark notes as "availability to be rechecked for 2026-03-30".

Return strict JSON only with this schema:
{
  "city": "Pune",
  "check_in_date": "2026-03-30",
  "budget_inr": {"min": 5000, "max": 10000},
  "hotels": [
    {
      "name": "",
      "area": "",
      "approx_price_inr": 0,
      "rating": "",
      "key_amenities": [""],
      "booking_url": "",
      "source": "",
      "notes": ""
    }
  ],
  "disclaimer": "Prices and availability can change; verify before booking"
}

Include at least 8 hotels if possible.
Prefer reliable sources (hotel brand pages, booking sites, major travel portals).
Return raw JSON only. Do not wrap in markdown code fences.
"#;

    let result = client.one_shot(prompt, None).await?;

    println!("Stop reason: {:?}", result.stop_reason);
    if let Some(usage) = result.usage {
        println!(
            "Usage tokens: prompt={:?} completion={:?} total={:?}",
            usage.prompt_tokens, usage.completion_tokens, usage.total_tokens
        );
    }
    println!("\n{}", result.response);

    Ok(())
}
