use super::TrafficProfile;

const LLM_MVP_PROFILE_JSON: &str = include_str!("../../profiles/llm-mvp.json");

pub fn llm_mvp_profile() -> TrafficProfile {
    serde_json::from_str(LLM_MVP_PROFILE_JSON)
        .expect("bundled DAM LLM MVP traffic profile JSON must be valid")
}
