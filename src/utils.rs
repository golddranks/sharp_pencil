//! This module implements various utilities.

pub fn join_string(list: Vec<String>, seq: &str) -> String {
    list.iter().fold(String::new(), |a, b| if a.is_empty() { a } else { a + seq } + &b)
}