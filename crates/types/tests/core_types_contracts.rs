use std::{
    collections::BTreeMap,
    env, fs,
    time::{SystemTime, UNIX_EPOCH},
};

use types::{
    CapsOverrideEntry, CapsOverrides, CatalogProvider, MemoryError, ModelCatalog, ModelDescriptor,
    ModelId, ModelLimits, ProviderError, ProviderId, RuntimeError, ToolError, UnknownModelCaps,
    derive_caps, derive_input_caps,
};

#[test]
fn runtime_error_composes_provider_and_tool_errors() {
    let provider_error = ProviderError::UnknownModel {
        provider: ProviderId::from("openai"),
        model: ModelId::from("does-not-exist"),
    };
    let runtime_from_provider: RuntimeError = provider_error.into();
    assert!(matches!(runtime_from_provider, RuntimeError::Provider(_)));

    let tool_error = ToolError::InvalidArguments {
        tool: "file_read".to_owned(),
        message: "path is required".to_owned(),
    };
    let runtime_from_tool: RuntimeError = tool_error.into();
    assert!(matches!(runtime_from_tool, RuntimeError::Tool(_)));

    let memory_error = MemoryError::NotFound {
        session_id: "session-1".to_owned(),
    };
    let runtime_from_memory: RuntimeError = memory_error.into();
    assert!(matches!(runtime_from_memory, RuntimeError::Memory(_)));
}

#[test]
fn model_catalog_validates_provider_and_model_id() {
    let provider = ProviderId::from("openai");
    let known_model = ModelId::from("gpt-4o-mini");
    let catalog = test_catalog_with_model(
        "openai",
        "gpt-4o-mini",
        "GPT-4o mini",
        ModelLimits {
            context: 128_000,
            output: 16_384,
        },
    );

    let descriptor = catalog
        .validate(&provider, &known_model)
        .expect("known model must validate");
    assert_eq!(descriptor.id, known_model.0);

    let unknown = catalog.validate(&provider, &ModelId::from("unknown-model"));
    assert!(matches!(
        unknown,
        Err(ProviderError::UnknownModel {
            provider: _,
            model: _
        })
    ));
}

#[test]
fn pinned_catalog_snapshot_parses_and_validates_known_model() {
    let catalog = ModelCatalog::from_pinned_snapshot().expect("pinned snapshot must parse");
    assert!(
        !catalog.providers.is_empty(),
        "pinned catalog must not be empty"
    );
    assert!(catalog.all_models().all(|(provider_id, model_id, _)| {
        !provider_id.trim().is_empty() && !model_id.trim().is_empty()
    }));

    let openai_provider = ProviderId::from("openai");
    let known_model_id = catalog
        .providers
        .get("openai")
        .and_then(|p| p.models.keys().next())
        .map(|id| ModelId::from(id.as_str()))
        .expect("pinned snapshot should include at least one openai model");
    let descriptor = catalog
        .validate(&openai_provider, &known_model_id)
        .expect("known pinned model should validate");
    assert_eq!(descriptor.id, known_model_id.0);

    let anthropic_provider = ProviderId::from("anthropic");
    let anthropic_model_id = catalog
        .providers
        .get("anthropic")
        .and_then(|p| p.models.keys().next())
        .map(|id| ModelId::from(id.as_str()))
        .expect("pinned snapshot should include at least one anthropic model");
    let anthropic_descriptor = catalog
        .validate(&anthropic_provider, &anthropic_model_id)
        .expect("known anthropic pinned model should validate");
    assert_eq!(anthropic_descriptor.id, anthropic_model_id.0);
}

#[test]
fn pinned_catalog_snapshot_rejects_missing_required_fields() {
    // CatalogProvider requires `id` and `name`; a model requires `id` and `name`.
    // This JSON has a provider entry missing the `name` field on a model.
    let invalid_snapshot = r#"
    {
      "openai": {
        "id": "openai",
        "name": "OpenAI",
        "models": {
          "missing-name": {
            "id": "missing-name"
          }
        }
      }
    }
    "#;

    let parse_result = ModelCatalog::from_snapshot_str(invalid_snapshot);
    assert!(matches!(parse_result, Err(ProviderError::Serialization(_))));
}

#[test]
fn model_catalog_regenerator_writes_canonical_sorted_snapshot() {
    // Provider keys and model keys are BTreeMap-ordered, so output is deterministic.
    let unsorted_snapshot = r#"
    {
      "zebra": {
        "id": "zebra",
        "name": "Zebra Provider",
        "models": {
          "z-model": { "id": "z-model", "name": "Z Model" },
          "a-model": { "id": "a-model", "name": "A Model" }
        }
      },
      "alpha": {
        "id": "alpha",
        "name": "Alpha Provider",
        "models": {
          "b-model": { "id": "b-model", "name": "B Model" }
        }
      }
    }
    "#;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let output_path = env::temp_dir().join(format!("oxydra-model-catalog-{timestamp}.json"));

    ModelCatalog::regenerate_snapshot(unsorted_snapshot, &output_path)
        .expect("regeneration should write canonical snapshot");

    let regenerated = fs::read_to_string(&output_path).expect("snapshot should be readable");
    fs::remove_file(&output_path).expect("temp snapshot should be removable");

    let catalog =
        ModelCatalog::from_snapshot_str(&regenerated).expect("regenerated snapshot should parse");

    // BTreeMap ordering: "alpha" before "zebra"
    let provider_ids: Vec<&str> = catalog.providers.keys().map(|k| k.as_str()).collect();
    assert_eq!(provider_ids, vec!["alpha", "zebra"]);

    // Within "zebra": "a-model" before "z-model"
    let zebra_model_ids: Vec<&str> = catalog.providers["zebra"]
        .models
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert_eq!(zebra_model_ids, vec!["a-model", "z-model"]);
}

#[test]
fn models_dev_json_snippet_deserializes() {
    // A representative subset of models.dev JSON should parse into the new structs.
    let snippet = r#"
    {
      "openai": {
        "id": "openai",
        "name": "OpenAI",
        "env": ["OPENAI_API_KEY"],
        "api": "https://api.openai.com/v1",
        "doc": "https://platform.openai.com/docs",
        "models": {
          "gpt-4o": {
            "id": "gpt-4o",
            "name": "GPT-4o",
            "family": "gpt-4o",
            "attachment": true,
            "reasoning": false,
            "tool_call": true,
            "structured_output": true,
            "temperature": true,
            "knowledge": "2023-10",
            "release_date": "2024-05-13",
            "modalities": { "input": ["text", "image"], "output": ["text"] },
            "open_weights": false,
            "cost": { "input": 2.5, "output": 10.0, "cache_read": 1.25 },
            "limit": { "context": 128000, "output": 16384 }
          },
          "o3-mini": {
            "id": "o3-mini",
            "name": "OpenAI o3-mini",
            "family": "o3",
            "reasoning": true,
            "tool_call": true,
            "interleaved": { "field": "reasoning_content" },
            "structured_output": true,
            "modalities": { "input": ["text"], "output": ["text"] },
            "cost": { "input": 1.1, "output": 4.4 },
            "limit": { "context": 200000, "output": 100000 }
          }
        }
      }
    }
    "#;

    let catalog = ModelCatalog::from_snapshot_str(snippet).expect("snippet should parse");
    assert_eq!(catalog.providers.len(), 1);

    let openai = &catalog.providers["openai"];
    assert_eq!(openai.id, "openai");
    assert_eq!(openai.name, "OpenAI");
    assert_eq!(openai.env, vec!["OPENAI_API_KEY"]);
    assert_eq!(openai.api.as_deref(), Some("https://api.openai.com/v1"));
    assert_eq!(openai.models.len(), 2);

    let gpt4o = &openai.models["gpt-4o"];
    assert_eq!(gpt4o.id, "gpt-4o");
    assert_eq!(gpt4o.name, "GPT-4o");
    assert_eq!(gpt4o.family.as_deref(), Some("gpt-4o"));
    assert!(gpt4o.attachment);
    assert!(!gpt4o.reasoning);
    assert!(gpt4o.tool_call);
    assert!(gpt4o.structured_output);
    assert!(gpt4o.temperature);
    assert_eq!(gpt4o.knowledge.as_deref(), Some("2023-10"));
    assert_eq!(gpt4o.modalities.input, vec!["text", "image"]);
    assert_eq!(gpt4o.modalities.output, vec!["text"]);
    assert!(!gpt4o.open_weights);
    assert!((gpt4o.cost.input - 2.5).abs() < f64::EPSILON);
    assert!((gpt4o.cost.output - 10.0).abs() < f64::EPSILON);
    assert!((gpt4o.cost.cache_read.unwrap() - 1.25).abs() < f64::EPSILON);
    assert_eq!(gpt4o.limit.context, 128_000);
    assert_eq!(gpt4o.limit.output, 16_384);

    let o3_mini = &openai.models["o3-mini"];
    assert!(o3_mini.reasoning);
    assert!(o3_mini.interleaved.is_some());
    assert_eq!(
        o3_mini.interleaved.as_ref().unwrap().field,
        "reasoning_content"
    );

    // Verify to_provider_caps derivation
    let caps = gpt4o.to_provider_caps();
    assert!(caps.supports_streaming); // default true
    assert!(caps.supports_tools);
    assert!(caps.supports_json_mode);
    assert!(!caps.supports_reasoning_traces);
    assert_eq!(caps.max_context_tokens, Some(128_000));
    assert_eq!(caps.max_output_tokens, Some(16_384));

    let o3_caps = o3_mini.to_provider_caps();
    assert!(o3_caps.supports_reasoning_traces);
}

#[test]
fn models_dev_interleaved_boolean_deserializes() {
    let snippet = r#"
    {
      "openai": {
        "id": "openai",
        "name": "OpenAI",
        "env": ["OPENAI_API_KEY"],
        "models": {
          "o3-mini": {
            "id": "o3-mini",
            "name": "OpenAI o3-mini",
            "reasoning": true,
            "interleaved": true,
            "limit": { "context": 200000, "output": 100000 }
          },
          "o3-mini-false": {
            "id": "o3-mini-false",
            "name": "OpenAI o3-mini false",
            "reasoning": true,
            "interleaved": false,
            "limit": { "context": 200000, "output": 100000 }
          }
        }
      }
    }
    "#;

    let catalog = ModelCatalog::from_snapshot_str(snippet).expect("snippet should parse");
    let openai = &catalog.providers["openai"];

    let o3_mini = &openai.models["o3-mini"];
    assert!(o3_mini.interleaved.is_some());
    assert_eq!(
        o3_mini.interleaved.as_ref().unwrap().field,
        "reasoning_content"
    );

    let o3_mini_false = &openai.models["o3-mini-false"];
    assert!(o3_mini_false.interleaved.is_none());
}

#[test]
fn all_models_iterator_yields_all_entries() {
    let catalog = ModelCatalog::from_pinned_snapshot().expect("pinned snapshot must parse");

    let expected_count: usize = catalog.providers.values().map(|p| p.models.len()).sum();
    let actual_count = catalog.all_models().count();
    assert_eq!(actual_count, expected_count);
    assert!(
        actual_count > 0,
        "catalog should contain at least one model"
    );

    // Every yielded triple should be consistent with the underlying maps
    for (provider_id, model_id, descriptor) in catalog.all_models() {
        assert_eq!(descriptor.id, model_id);
        assert!(catalog.providers.contains_key(provider_id));
        assert!(catalog.providers[provider_id].models.contains_key(model_id));
    }
}

#[test]
fn to_provider_caps_maps_fields_correctly() {
    let descriptor = ModelDescriptor {
        id: "test-model".to_owned(),
        name: "Test Model".to_owned(),
        family: None,
        attachment: false,
        reasoning: false,
        tool_call: true,
        interleaved: None,
        structured_output: true,
        temperature: true,
        knowledge: None,
        release_date: None,
        last_updated: None,
        modalities: Default::default(),
        open_weights: false,
        cost: Default::default(),
        limit: ModelLimits {
            context: 100_000,
            output: 8192,
        },
    };

    let caps = descriptor.to_provider_caps();
    assert!(caps.supports_streaming);
    assert!(caps.supports_tools);
    assert!(caps.supports_json_mode);
    assert!(!caps.supports_reasoning_traces);
    assert_eq!(caps.max_input_tokens, Some(100_000));
    assert_eq!(caps.max_output_tokens, Some(8192));
    assert_eq!(caps.max_context_tokens, Some(100_000));

    // With reasoning + interleaved
    let reasoning_descriptor = ModelDescriptor {
        reasoning: true,
        interleaved: Some(types::InterleavedSpec {
            field: "thinking".to_owned(),
        }),
        ..descriptor
    };
    let reasoning_caps = reasoning_descriptor.to_provider_caps();
    assert!(reasoning_caps.supports_reasoning_traces);
}

#[test]
fn to_input_caps_maps_attachment_modalities() {
    let descriptor = ModelDescriptor {
        id: "multi-modal".to_owned(),
        name: "Multi Modal".to_owned(),
        family: None,
        attachment: true,
        reasoning: false,
        tool_call: false,
        interleaved: None,
        structured_output: false,
        temperature: true,
        knowledge: None,
        release_date: None,
        last_updated: None,
        modalities: types::Modalities {
            input: vec![
                "text".to_owned(),
                "image".to_owned(),
                "audio".to_owned(),
                "pdf".to_owned(),
                "unknown".to_owned(),
            ],
            output: vec!["text".to_owned()],
        },
        open_weights: false,
        cost: Default::default(),
        limit: ModelLimits {
            context: 32_000,
            output: 4096,
        },
    };

    let caps = descriptor.to_input_caps();
    assert!(caps.supports_attachments);
    assert!(caps.accepts_modality(types::InputModality::Image));
    assert!(caps.accepts_modality(types::InputModality::Audio));
    assert!(caps.accepts_modality(types::InputModality::Pdf));
    assert!(!caps.accepts_modality(types::InputModality::Video));
}

#[test]
fn derive_input_caps_overlay_applies_and_model_override_wins() {
    let descriptor = ModelDescriptor {
        id: "special-model".to_owned(),
        name: "Special".to_owned(),
        family: None,
        attachment: false,
        reasoning: false,
        tool_call: false,
        interleaved: None,
        structured_output: false,
        temperature: true,
        knowledge: None,
        release_date: None,
        last_updated: None,
        modalities: Default::default(),
        open_weights: false,
        cost: Default::default(),
        limit: Default::default(),
    };

    let mut overrides = CapsOverrides::default();
    overrides.provider_defaults.insert(
        "custom".to_owned(),
        CapsOverrideEntry {
            attachment: Some(true),
            input_modalities: Some(vec!["image".to_owned()]),
            ..Default::default()
        },
    );
    overrides.overrides.insert(
        "custom/special-model".to_owned(),
        CapsOverrideEntry {
            input_modalities: Some(vec!["audio".to_owned(), "video".to_owned()]),
            ..Default::default()
        },
    );

    let caps = derive_input_caps("custom", &descriptor, &overrides);
    assert!(caps.supports_attachments);
    assert!(!caps.accepts_modality(types::InputModality::Image));
    assert!(caps.accepts_modality(types::InputModality::Audio));
    assert!(caps.accepts_modality(types::InputModality::Video));
}

#[test]
fn unknown_model_defaults_support_attachment_overrides() {
    let descriptor = ModelDescriptor::default_for_unknown(
        "custom-model",
        &UnknownModelCaps {
            attachment: Some(true),
            input_modalities: Some(vec!["image".to_owned(), "document".to_owned()]),
            reasoning: None,
            max_input_tokens: None,
            max_output_tokens: None,
            max_context_tokens: None,
        },
    );

    let caps = descriptor.to_input_caps();
    assert!(caps.supports_attachments);
    assert!(caps.accepts_modality(types::InputModality::Image));
    assert!(caps.accepts_modality(types::InputModality::Document));
}

#[test]
fn unknown_image_model_defaults_disable_tools_and_infer_image_modality() {
    let descriptor = ModelDescriptor::default_for_unknown(
        "gemini-3.1-flash-image-preview",
        &UnknownModelCaps::default(),
    );

    assert!(
        !descriptor.tool_call,
        "unknown image models should default to no tool calling"
    );
    assert!(descriptor.attachment);
    let caps = descriptor.to_input_caps();
    assert!(caps.supports_attachments);
    assert!(caps.accepts_modality(types::InputModality::Image));
}

#[test]
fn catalog_input_caps_uses_overrides() {
    let mut catalog = ModelCatalog::from_snapshot_str(
        r#"
    {
      "openai": {
        "id": "openai",
        "name": "OpenAI",
        "models": {
          "gpt-test": {
            "id": "gpt-test",
            "name": "GPT Test",
            "attachment": false,
            "modalities": { "input": [], "output": ["text"] },
            "limit": { "context": 128000, "output": 8192 }
          }
        }
      }
    }
    "#,
    )
    .expect("catalog should parse");

    catalog.caps_overrides.provider_defaults.insert(
        "openai".to_owned(),
        CapsOverrideEntry {
            attachment: Some(true),
            input_modalities: Some(vec!["image".to_owned()]),
            ..Default::default()
        },
    );

    let caps = catalog
        .input_caps(&ProviderId::from("openai"), &ModelId::from("gpt-test"))
        .expect("input caps should resolve");
    assert!(caps.supports_attachments);
    assert!(caps.accepts_modality(types::InputModality::Image));
}

// ---------------------------------------------------------------------------
// Step 2 verification: ProviderCaps derivation + Oxydra overlay
// ---------------------------------------------------------------------------

#[test]
fn caps_derived_from_model_descriptor() {
    let descriptor = ModelDescriptor {
        id: "test-model".to_owned(),
        name: "Test Model".to_owned(),
        family: None,
        attachment: false,
        reasoning: false,
        tool_call: true,
        interleaved: None,
        structured_output: false,
        temperature: true,
        knowledge: None,
        release_date: None,
        last_updated: None,
        modalities: Default::default(),
        open_weights: false,
        cost: Default::default(),
        limit: ModelLimits {
            context: 64_000,
            output: 4096,
        },
    };

    let overrides = CapsOverrides::default();
    let caps = derive_caps("test-provider", &descriptor, &overrides);

    assert!(caps.supports_tools, "tool_call=true → supports_tools=true");
    assert!(
        !caps.supports_json_mode,
        "structured_output=false → supports_json_mode=false"
    );
    assert!(
        !caps.supports_reasoning_traces,
        "reasoning=false → supports_reasoning_traces=false"
    );
    assert_eq!(caps.max_context_tokens, Some(64_000));
    assert_eq!(caps.max_output_tokens, Some(4096));
}

#[test]
fn caps_overlay_applies() {
    let descriptor = ModelDescriptor {
        id: "claude-3-5-sonnet-latest".to_owned(),
        name: "Claude 3.5 Sonnet".to_owned(),
        family: None,
        attachment: false,
        reasoning: false,
        tool_call: true,
        interleaved: None,
        structured_output: false,
        temperature: true,
        knowledge: None,
        release_date: None,
        last_updated: None,
        modalities: Default::default(),
        open_weights: false,
        cost: Default::default(),
        limit: ModelLimits {
            context: 200_000,
            output: 8192,
        },
    };

    // Baseline: to_provider_caps defaults streaming to true
    let baseline = descriptor.to_provider_caps();
    assert!(baseline.supports_streaming);

    // Model-specific overlay sets streaming to false
    let mut overrides = CapsOverrides::default();
    overrides.overrides.insert(
        "anthropic/claude-3-5-sonnet-latest".to_owned(),
        CapsOverrideEntry {
            supports_streaming: Some(false),
            ..Default::default()
        },
    );

    let caps = derive_caps("anthropic", &descriptor, &overrides);
    assert!(
        !caps.supports_streaming,
        "model-specific overlay should override baseline"
    );
    assert!(
        caps.supports_tools,
        "non-overridden field should keep baseline value"
    );
}

#[test]
fn caps_default_by_provider() {
    let descriptor = ModelDescriptor {
        id: "some-model".to_owned(),
        name: "Some Model".to_owned(),
        family: None,
        attachment: false,
        reasoning: false,
        tool_call: false,
        interleaved: None,
        structured_output: false,
        temperature: true,
        knowledge: None,
        release_date: None,
        last_updated: None,
        modalities: Default::default(),
        open_weights: false,
        cost: Default::default(),
        limit: ModelLimits {
            context: 100_000,
            output: 4096,
        },
    };

    // Provider default sets supports_tools to true
    let mut overrides = CapsOverrides::default();
    overrides.provider_defaults.insert(
        "my-provider".to_owned(),
        CapsOverrideEntry {
            supports_tools: Some(true),
            ..Default::default()
        },
    );

    let caps = derive_caps("my-provider", &descriptor, &overrides);
    assert!(
        caps.supports_tools,
        "provider default should override baseline when no model override exists"
    );
}

#[test]
fn caps_model_override_takes_precedence_over_provider_default() {
    let descriptor = ModelDescriptor {
        id: "special-model".to_owned(),
        name: "Special Model".to_owned(),
        family: None,
        attachment: false,
        reasoning: false,
        tool_call: false,
        interleaved: None,
        structured_output: false,
        temperature: true,
        knowledge: None,
        release_date: None,
        last_updated: None,
        modalities: Default::default(),
        open_weights: false,
        cost: Default::default(),
        limit: ModelLimits {
            context: 100_000,
            output: 4096,
        },
    };

    let mut overrides = CapsOverrides::default();
    // Provider default: streaming = false
    overrides.provider_defaults.insert(
        "my-provider".to_owned(),
        CapsOverrideEntry {
            supports_streaming: Some(false),
            ..Default::default()
        },
    );
    // Model-specific: streaming = true
    overrides.overrides.insert(
        "my-provider/special-model".to_owned(),
        CapsOverrideEntry {
            supports_streaming: Some(true),
            ..Default::default()
        },
    );

    let caps = derive_caps("my-provider", &descriptor, &overrides);
    assert!(
        caps.supports_streaming,
        "model-specific override should take precedence over provider default"
    );
}

#[test]
fn pinned_caps_overrides_parses() {
    // The pinned overrides file bundled via include_str! should parse.
    let catalog = ModelCatalog::from_pinned_snapshot().expect("pinned snapshot must parse");
    // Provider defaults should be loaded
    assert!(
        !catalog.caps_overrides.provider_defaults.is_empty(),
        "pinned overrides should have provider defaults"
    );
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn test_catalog_with_model(
    provider_id: &str,
    model_id: &str,
    model_name: &str,
    limit: ModelLimits,
) -> ModelCatalog {
    let mut models = BTreeMap::new();
    models.insert(
        model_id.to_owned(),
        ModelDescriptor {
            id: model_id.to_owned(),
            name: model_name.to_owned(),
            family: None,
            attachment: false,
            reasoning: false,
            tool_call: true,
            interleaved: None,
            structured_output: false,
            temperature: true,
            knowledge: None,
            release_date: None,
            last_updated: None,
            modalities: Default::default(),
            open_weights: false,
            cost: Default::default(),
            limit,
        },
    );

    let mut providers = BTreeMap::new();
    providers.insert(
        provider_id.to_owned(),
        CatalogProvider {
            id: provider_id.to_owned(),
            name: provider_id.to_owned(),
            env: vec![],
            api: None,
            doc: None,
            models,
        },
    );

    ModelCatalog::new(providers)
}
