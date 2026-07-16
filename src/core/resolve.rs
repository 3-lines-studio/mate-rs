use crate::agent;
use crate::config::{Config, ModelConfig, ProviderConfig};
use crate::provider::{Client, ModelProfile, ProviderPreferences};
use crate::tools::{lookup, Registry};
use std::collections::HashMap;

pub fn resolve_client(
    model_id: &str,
    models: &[ModelConfig],
    providers: &[ProviderConfig],
    verbose: bool,
) -> Result<(Client, String), String> {
    let m = models
        .iter()
        .find(|m| m.id == model_id)
        .ok_or_else(|| format!("model {:?} not found", model_id))?;

    let p = providers
        .iter()
        .find(|p| p.id == m.provider)
        .ok_or_else(|| {
            format!(
                "provider {:?} not found for model {:?}",
                m.provider, model_id
            )
        })?;

    let profile = ModelProfile {
        context_window: m.context_window,
        max_output_tokens: m.max_output_tokens,
        thinking_type: m.thinking_type.clone(),
        reasoning_effort: m.reasoning_effort.clone(),
        reasoning_max_tokens: m.reasoning_max_tokens,
        open_router: is_open_router(p),
        input_price: m.input_price,
        cached_input_price: m.cached_input_price,
        output_price: m.output_price,
        fallback_models: m.fallback_models.clone(),
        route: m.route.clone(),
        provider_prefs: if m.provider_sort.is_empty() {
            None
        } else {
            Some(ProviderPreferences {
                order: Vec::new(),
                allow_fallbacks: None,
                require_parameters: None,
                data_collection: String::new(),
                only: Vec::new(),
                ignore: Vec::new(),
                quantizations: Vec::new(),
                sort: m.provider_sort.clone(),
            })
        },
        prompt_cache: m.prompt_cache,
        prompt_cache_ttl: m.prompt_cache_ttl.clone(),
    };

    let mut client = Client::new(&p.base_url, &m.name, &p.api_key, profile);
    client.set_debug(verbose);
    let extra_headers = provider_headers(p);
    if !extra_headers.is_empty() {
        client.set_extra_headers(extra_headers);
    }
    Ok((client, m.name.clone()))
}

pub fn is_open_router(p: &ProviderConfig) -> bool {
    let s = format!("{} {}", p.id.to_lowercase(), p.base_url.to_lowercase());
    s.contains("openrouter")
}

pub fn provider_headers(p: &ProviderConfig) -> HashMap<String, String> {
    let mut h = HashMap::new();
    if !p.referer.is_empty() {
        h.insert("HTTP-Referer".to_string(), p.referer.clone());
    }
    if !p.app_title.is_empty() {
        h.insert("X-OpenRouter-Title".to_string(), p.app_title.clone());
    }
    if !p.categories.is_empty() {
        h.insert("X-OpenRouter-Categories".to_string(), p.categories.clone());
    }
    h
}

pub fn resolve_subagents(
    cfg: &Config,
    system_md: &str,
    global_md: &str,
    local_md: &str,
    verbose: bool,
) -> HashMap<String, agent::SubagentDef> {
    let mut defs = HashMap::new();
    for sc in &cfg.subagents {
        let (client, model_name) =
            match resolve_client(&sc.model, &cfg.models, &cfg.providers, verbose) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!(
                        "subagent model not found subagent={} model={}: {}",
                        sc.id,
                        sc.model,
                        e
                    );
                    continue;
                }
            };

        let mut reg = Registry::new();
        for name in &sc.tools {
            if let Some(t) = lookup(name) {
                let _ = reg.register(t);
            }
        }

        let prompt = agent::build_system_prompt(system_md, global_md, local_md, &sc.prompt);
        defs.insert(
            sc.id.clone(),
            agent::SubagentDef {
                id: sc.id.clone(),
                description: sc.description.clone(),
                client: std::sync::Arc::new(client),
                registry: std::sync::Arc::new(reg),
                system_prompt: prompt,
                model_name,
            },
        );
    }
    defs
}
