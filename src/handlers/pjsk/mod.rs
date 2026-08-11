pub mod asset_binary;
pub mod asset_source;
pub mod assets;
pub mod b30;
pub mod card;
pub mod event;
pub mod music;
pub mod profile;

use once_cell::sync::Lazy;
use std::collections::HashMap;

pub static VALID_SERVERS: Lazy<std::collections::HashSet<String>> = Lazy::new(|| {
    let mut s = std::collections::HashSet::new();
    s.insert("jp".to_string());
    s.insert("cn".to_string());
    s.insert("en".to_string());
    s.insert("tw".to_string());
    s.insert("kr".to_string());
    s
});

pub static SERVER_NAMES: Lazy<HashMap<String, String>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("jp".to_string(), "日服".to_string());
    m.insert("cn".to_string(), "国服".to_string());
    m.insert("en".to_string(), "美服".to_string());
    m.insert("tw".to_string(), "台服".to_string());
    m.insert("kr".to_string(), "韩服".to_string());
    m
});
