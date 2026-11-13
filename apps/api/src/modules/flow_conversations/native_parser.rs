//! Native provider transcript whitelist; raw provider metadata never reaches archive.

use serde_json::{Value, json};

/// Normalize terminal verified provider details; unavailable or malformed snapshots stay retryable.
pub fn normalize(value: &Value, provider: &str, agent: &str) -> Option<Value> {
    if value["conversation_id"] != provider
        || value["agent_id"] != agent
        || !matches!(value["status"].as_str(), Some("done" | "failed"))
    {
        return None;
    }
    let transcript = value["transcript"].as_array()?;
    let text_only = match value["metadata"]["text_only"] {
        Value::Bool(value) => value,
        Value::Null => false,
        _ => return None,
    };
    let mut turns = Vec::new();
    for (index, row) in transcript.iter().enumerate() {
        let Some(role @ ("user" | "agent")) = row["role"].as_str() else {
            continue;
        };
        if !row["contextual_update_info"].is_null()
            || !row["platform_event"].is_null()
            || row["message"].is_null()
        {
            continue;
        }
        let raw = row["message"].as_str()?;
        if raw.chars().all(super::redaction::js_whitespace) {
            continue;
        }
        let text = super::redaction::archive_text(raw);
        if text.len() > 16384 {
            return None;
        }
        let key = row["id"]
            .as_str()
            .filter(|s| {
                (1..=200).contains(&s.len())
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            })
            .map(str::to_owned)
            .unwrap_or_else(|| format!("turn:{index}"));
        turns.push(json!({"key":key,"role":role,"text":text,"modality":if row["source_medium"]=="text"||text_only{"text"}else{"voice"},"interrupted":row["interrupted"]==true}));
        if turns.len() > 1000 {
            return None;
        }
    }
    let summary = value["analysis"]["transcript_summary"]
        .as_str()
        .map(super::redaction::archive_text)
        .filter(|s| !s.is_empty());
    if summary.as_ref().is_some_and(|s| s.len() > 16384) {
        return None;
    }
    let result = json!({"provider_conversation_id":provider,"agent_id":agent,"turns":turns,"summary":summary});
    (serde_json::to_vec(&result).ok()?.len() <= 2 * 1024 * 1024).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn whitelist_matches_office_keys_roles_redaction_and_source_order() {
        let input = json!({"conversation_id":"conv_test","agent_id":"agent_test","status":"done","metadata":{"text_only":false},"transcript":[{"id":"m1","role":"user","message":"Use blue","source_medium":"text"},{"role":"agent","message":"private scene","contextual_update_info":{}},{"role":"agent","message":null,"tool_calls":[{"token":"never-store"}]},{"role":"user","message":"Confirm action 123456"},{"id":"m3","role":"agent","message":"Use red","interrupted":true}],"analysis":{"transcript_summary":"Confirm batch 654321","token":"never-store"}});
        let result = normalize(&input, "conv_test", "agent_test").unwrap();
        assert_eq!(result["turns"].as_array().unwrap().len(), 3);
        assert_eq!(result["turns"][1]["key"], "turn:3");
        assert_eq!(result["turns"][1]["text"], "[approval redacted]");
        assert_eq!(result["turns"][2]["role"], "agent");
        assert_eq!(result["turns"][2]["interrupted"], true);
        assert!(!result.to_string().contains("never-store"));
        assert_eq!(result["summary"], "[approval redacted]");
    }
    #[test]
    fn incomplete_wrong_identity_and_oversize_never_claim_final() {
        let input = json!({"conversation_id":"conv_test","agent_id":"agent_test","status":"processing","transcript":[]});
        assert!(normalize(&input, "conv_test", "agent_test").is_none());
        let mut input = input;
        input["status"] = json!("done");
        assert!(normalize(&input, "conv_other", "agent_test").is_none());
        input["transcript"] = json!([{"role":"user","message":"x".repeat(16385)}]);
        assert!(normalize(&input, "conv_test", "agent_test").is_none());
    }
}
