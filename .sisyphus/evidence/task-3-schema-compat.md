# Task 3: Schema Compatibility Analysis

## Summary

**VERDICT: SAFE to filter schemas() return value**

All callers of `ToolRegistry::schemas()` and consumers of `context.tools` handle filtered subsets correctly. No caller assumes the full tool set is present.

---

## Callers of `ToolRegistry::schemas()`

### 1. `crates/runtime/src/lib.rs:372`
```rust
if context.tools.is_empty() {
    context.tools = self.tool_registry.schemas();
}
```
**Analysis**: Populates `context.tools` if empty. If `schemas()` returns a filtered subset, only those tools will be available to the LLM. This is the intended behavior for selective tool exposure.

**Compatibility**: ✅ SAFE - This is the primary injection point where filtering would be applied.

### 2. `crates/tools/src/tests.rs:657`
```rust
let exposed = registry
    .schemas()
    .into_iter()
    .map(|schema| schema.name)
    .collect::<BTreeSet<_>>();
let expected = BTreeSet::from([...]);
assert_eq!(exposed, expected);
```
**Analysis**: Test `bootstrap_registry_exposes_runtime_tool_surface_only` validates that bootstrapped registry exposes expected runtime tools. Test asserts exact set equality.

**Compatibility**: ⚠️ NEEDS ADJUSTMENT - This test asserts exact tool set. If filtering is implemented, this test would need to either:
- Test against the filtered registry (if filtering happens at registry level)
- Or test the unfiltered registry separately

### 3. `crates/tools/src/tests.rs:1196`
```rust
let schemas = registry.schemas();
let names: Vec<String> = schemas.iter().map(|s| s.name.clone()).collect();
assert!(names.contains(&MEMORY_SEARCH_TOOL_NAME.to_owned()));
// ... more assertions
```
**Analysis**: Test validates that memory tools are registered. Uses `contains()` checks, not exact count.

**Compatibility**: ✅ SAFE - Uses `contains()` assertions, so filtered subsets that still include memory tools would pass.

---

## Consumers of `context.tools`

### 1. Provider Layer - All Providers

#### `crates/provider/src/openai.rs:289-294`
```rust
let tools = context
    .tools
    .iter()
    .map(OpenAIRequestToolDefinition::from)
    .collect::<Vec<_>>();
let tool_choice = (!tools.is_empty()).then_some("auto".to_owned());
```
**Analysis**: Maps all tools in context to OpenAI format. Uses `!tools.is_empty()` to determine if tool calling should be enabled.

**Compatibility**: ✅ SAFE - Works with any subset. Empty tools = no tool calling.

#### `crates/provider/src/anthropic.rs:424-435`
```rust
let tools = context
    .tools
    .iter()
    .map(AnthropicRequestToolDefinition::from)
    .collect::<Vec<_>>();
let tool_choice = (!tools.is_empty()).then_some(AnthropicToolChoice::auto());
```
**Analysis**: Same pattern as OpenAI - maps tools and checks `!tools.is_empty()`.

**Compatibility**: ✅ SAFE - Works with any subset.

#### `crates/provider/src/gemini.rs:428-439`
```rust
let tools = if context.tools.is_empty() {
    None
} else {
    let function_declarations: Vec<GeminiFunctionDeclaration> = context
        .tools
        .iter()
        .map(GeminiFunctionDeclaration::from)
        .collect();
    Some(vec![GeminiToolDeclaration { function_declarations }])
};
```
**Analysis**: Checks `is_empty()` first, then maps all tools to Gemini format.

**Compatibility**: ✅ SAFE - Works with any subset.

#### `crates/provider/src/responses.rs:521-525`
```rust
let tools = context
    .tools
    .iter()
    .map(ResponsesToolDeclaration::from)
    .collect::<Vec<_>>();
```
**Analysis**: Maps all tools to Responses API format.

**Compatibility**: ✅ SAFE - Works with any subset.

### 2. Runtime Layer

#### `crates/runtime/src/lib.rs:371-373`
```rust
if context.tools.is_empty() {
    context.tools = self.tool_registry.schemas();
}
```
**Analysis**: Only populates tools if context has none. This is the injection point.

**Compatibility**: ✅ SAFE - This is where filtering would be applied.

#### `crates/runtime/src/provider_response.rs:12-23`
```rust
let request_context = if caps.supports_tools || context.tools.is_empty() {
    context.clone()
} else {
    tracing::debug!(...);
    let mut request_context = context.clone();
    request_context.tools.clear();
    request_context
};
```
**Analysis**: If provider doesn't support tools but context has tools, clears tools for this request only. This is provider-level filtering.

**Compatibility**: ✅ SAFE - Already handles filtered/subset tool scenarios.

#### `crates/runtime/src/budget.rs:424-426`
```rust
let tool_chars = serde_json::to_string(&context.tools)
    .map(|value| value.len())
    .unwrap_or(usize::MAX);
```
**Analysis**: Serializes tools to estimate token budget consumption.

**Compatibility**: ✅ SAFE - Smaller tool set = smaller budget estimate. Correct behavior.

#### `crates/runtime/src/budget.rs:538-540`
```rust
fn estimate_tool_schema_tokens(&self, context: &Context) -> Result<u64, RuntimeError> {
    let serialized =
        serde_json::to_string(&context.tools).map_err(|_| RuntimeError::BudgetExceeded)?;
    Self::estimate_text_tokens(serialized.as_str())
}
```
**Analysis**: Estimates token count for tool schemas in context.

**Compatibility**: ✅ SAFE - Smaller tool set = lower token estimate. Correct behavior.

### 3. Test Layer

#### `crates/runtime/src/tests.rs:1298`
```rust
.withf(|context| context.tools.iter().any(|tool| tool.name == "file_read"))
```
**Analysis**: Mock expectation checks that `file_read` tool is present in context.

**Compatibility**: ✅ SAFE - Uses `any()`, not count check. Filtered subset containing `file_read` passes.

#### `crates/runtime/src/tests.rs:1315`
```rust
assert!(context.tools.iter().any(|tool| tool.name == "file_read"));
```
**Analysis**: Post-call assertion that `file_read` is in context tools.

**Compatibility**: ✅ SAFE - Uses `any()`, not count check.

#### `crates/runtime/src/tests.rs:1327`
```rust
.withf(|context| context.tools.is_empty())
```
**Analysis**: Mock expectation for non-tool-capable model - expects empty tools.

**Compatibility**: ✅ SAFE - Tests empty tools scenario, not full set.

#### `crates/runtime/src/tests.rs:1341`
```rust
assert!(context.tools.iter().any(|tool| tool.name == "file_read"));
```
**Analysis**: Post-call assertion that `file_read` is still in context (even though provider didn't receive it).

**Compatibility**: ✅ SAFE - Uses `any()`, not count check.

---

## Key Findings

### Safe to Filter: YES

All consumers handle variable-sized tool sets correctly:

1. **Providers**: All four providers (OpenAI, Anthropic, Gemini, Responses) iterate over `context.tools` and convert to provider-specific format. They only check `is_empty()` to determine if tool calling should be enabled.

2. **Runtime**: The runtime populates `context.tools` from `schemas()` only if empty. Budget calculations correctly estimate based on actual tool set size.

3. **Tests**: Most tests use `any()` checks for specific tools, not hard-coded counts. One test (`bootstrap_registry_exposes_runtime_tool_surface_only`) asserts exact set equality and would need adjustment.

### Required Adjustments

| Location | Issue | Adjustment |
|----------|-------|------------|
| `crates/tools/src/tests.rs:654-676` | Asserts exact tool set equality | Test should either use a filtered registry or check for tool presence with `contains()` instead of exact set equality |

### No Assumptions About Full Tool Set

- No code assumes all tools are present
- No hard-coded tool counts
- No provider requires specific tools
- Tool dispatch (`execute_with_policy_and_context`) looks up tools by name from registry, not from context.tools

---

## Conclusion

**Schema filtering is safe to implement.** The only adjustment needed is updating one test that asserts exact tool set equality. All production code handles filtered subsets correctly.

The architecture already supports this:
- `context.tools` is the filtered view sent to providers
- `tool_registry` remains the source of truth for execution
- Budget calculations adapt to actual tool set size
- Provider capability checks already handle tool presence/absence
