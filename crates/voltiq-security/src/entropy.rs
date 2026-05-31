//! Shannon entropy, used to gate the generic "looks like a high-entropy secret" rule.

use std::collections::HashMap;

/// Shannon entropy in bits per byte (0.0 ..= 8.0). Random base64/hex lands ~4.5–6;
/// natural-language identifiers land ~2.5–3.5.
pub fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut counts: HashMap<u8, u32> = HashMap::new();
    for b in s.bytes() {
        *counts.entry(b).or_insert(0) += 1;
    }
    let len = s.len() as f64;
    counts
        .values()
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_is_higher_than_words() {
        let random = shannon_entropy("Zk9x2QpL7mWv3RtY8nUa1BcD");
        let words = shannon_entropy("getUserByEmailAddress");
        assert!(
            random > words,
            "random {random} should exceed words {words}"
        );
    }
}
