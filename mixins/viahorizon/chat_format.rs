// SPDX-License-Identifier: GPL-3.0-or-later
fn inject(
    bound: &horizon_protocol::packets::play::ChatTypeBound,
    content: &serde_json::Value,
    decoration: crate::game::remote::Decoration,
) -> ::core::ops::ControlFlow<horizon_client::text::Text> {
    use horizon_client::scoreboard::{comp_text, tag_to_json};
    use horizon_world::nbt::Tag;
    let narration = matches!(decoration, crate::game::remote::Decoration::Narration);
    let Some((sender, decoration)) = horizon_protocol::vfp_chat::direct_decoration(bound, narration) else {
        return ::core::ops::ControlFlow::Continue(());
    };
    let key = match decoration.get("translation_key") {
        Some(Tag::String(key)) => key.to_string_lossy(),
        _ => return ::core::ops::ControlFlow::Continue(()),
    };
    let parameters = match decoration.get("parameters") {
        Some(Tag::List(parameters)) => parameters,
        _ => return ::core::ops::ControlFlow::Continue(()),
    };
    let with: Vec<serde_json::Value> = parameters
        .iter()
        .map(|parameter| match parameter {
            Tag::String(name) => match name.to_string_lossy().as_str() {
                "sender" => tag_to_json(sender),
                "target" => bound
                    .target
                    .as_ref()
                    .map_or_else(|| serde_json::Value::from(""), tag_to_json),
                _ => content.clone(),
            },
            _ => serde_json::Value::from(""),
        })
        .collect();
    let mut component = decoration
        .get("style")
        .map_or_else(|| serde_json::json!({}), tag_to_json);
    component["translate"] = key.into();
    component["with"] = with.into();
    ::core::ops::ControlFlow::Break(comp_text(&component))
}
