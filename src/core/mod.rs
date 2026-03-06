use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub timestamp: i64,      // We'll treat this as MS now
    pub confidence: f64,
    pub associations: Vec<String>,
    pub access_count: u32,
    pub last_access: i64,    // MS here too
}


#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CoActivation {
    pub tag_a: String,
    pub tag_b: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Synapse {
    pub tag_a: String,
    pub tag_b: String,
    pub weight_log: f64, // log(w)
    pub last_seen: i64,
}
