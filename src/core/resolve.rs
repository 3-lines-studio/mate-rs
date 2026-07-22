use crate::agent;
use crate::config::{Config, ModelConfig, ProviderConfig};
use crate::provider::{Client, ModelProfile};
use crate::tools::Registry;
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
        reasoning_effort: m.reasoning_effort.clone(),
        open_router: p.open_router,
        input_price: m.input_price,
        cached_input_price: m.cached_input_price,
        output_price: m.output_price,
        prompt_cache: m.prompt_cache,
    };

    let mut client = Client::new(&p.base_url, &m.name, &p.api_key, profile);
    client.set_debug(verbose);
    Ok((client, m.name.clone()))
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

        let std = Registry::standard();
        let reg = if sc.tools.iter().any(|t| t == "*") {
            std
        } else {
            let mut reg = Registry::new();
            for name in &sc.tools {
                if let Some(t) = std.get(name) {
                    let _ = reg.register(t.clone());
                }
            }
            reg
        };

        let prompt = agent::build_system_prompt(
            system_md,
            global_md,
            local_md,
            &sc.prompt,
            !sc.tools.is_empty(),
        );
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
