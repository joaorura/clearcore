#![forbid(unsafe_code)]

const K: [u32; 64] = [
    0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4,
    0xab1c_5ed5, 0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe,
    0x9bdc_06a7, 0xc19b_f174, 0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f,
    0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da, 0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7,
    0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967, 0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc,
    0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85, 0xa2bf_e8a1, 0xa81a_664b,
    0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070, 0x19a4_c116,
    0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
    0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7,
    0xc671_78f2,
];

const fn rotr(x: u32, n: u32) -> u32 {
    (x >> n) | (x << (32 - n))
}

const fn s0(x: u32) -> u32 {
    rotr(x, 7) ^ rotr(x, 18) ^ (x >> 3)
}

const fn s1(x: u32) -> u32 {
    rotr(x, 17) ^ rotr(x, 19) ^ (x >> 10)
}

const fn big_s0(x: u32) -> u32 {
    rotr(x, 2) ^ rotr(x, 13) ^ rotr(x, 22)
}

const fn big_s1(x: u32) -> u32 {
    rotr(x, 6) ^ rotr(x, 11) ^ rotr(x, 25)
}

const fn ch(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (!x & z)
}

const fn maj(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (x & z) ^ (y & z)
}

#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn sha256_digest(input: &[u8]) -> [u8; 32] {
    let mut state = [
        0x6a09_e667_u32,
        0xbb67_ae85_u32,
        0x3c6e_f372_u32,
        0xa54f_f53a_u32,
        0x510e_527f_u32,
        0x9b05_688c_u32,
        0x1f83_d9ab_u32,
        0x5be0_cd19_u32,
    ];

    let bit_len = (input.len() as u64) * 8;
    let mut padded = Vec::with_capacity(input.len() + 64);
    padded.extend_from_slice(input);
    padded.push(0x80);

    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0_u32; 64];
        for (i, slot) in w.iter_mut().enumerate().take(16) {
            let start = i * 4;
            *slot = u32::from_be_bytes([
                chunk[start],
                chunk[start + 1],
                chunk[start + 2],
                chunk[start + 3],
            ]);
        }
        for i in 16..64 {
            w[i] = w[i - 16]
                .wrapping_add(s0(w[i - 15]))
                .wrapping_add(w[i - 7])
                .wrapping_add(s1(w[i - 2]));
        }

        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];

        for i in 0..64 {
            let t1 = h
                .wrapping_add(big_s1(e))
                .wrapping_add(ch(e, f, g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t2 = big_s0(a).wrapping_add(maj(a, b, c));
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }

    let mut out = [0_u8; 32];
    for (i, word) in state.iter().enumerate() {
        let bytes = word.to_be_bytes();
        out[i * 4..(i + 1) * 4].copy_from_slice(&bytes);
    }
    out
}

#[must_use]
pub fn sha256_hex(input: &[u8]) -> String {
    use std::fmt::Write;
    let digest = sha256_digest(input);
    let mut hex = String::with_capacity(64);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Computes a salted per-installation device ID hash.
/// Raw device names, hardware serials, and endpoint paths are NEVER leaked.
#[must_use]
pub fn hash_device_id(raw_device_id: &str, install_salt: &str) -> String {
    let payload = format!("salt:{install_salt}:dev:{raw_device_id}");
    sha256_hex(payload.as_bytes())
}

/// Sanitizes diagnostic text by redacting:
/// - User home directory paths
/// - Quoted string snippets (which may contain session titles or speech transcripts)
/// - Common conversational or meeting keywords
#[must_use]
pub fn sanitize_text(text: &str) -> String {
    let mut result = text.to_string();

    // 1. Redact quoted strings (potential speech transcripts, document names, meeting names)
    // Replaces '...' or "..." with [REDACTED_CONTENT]
    result = redact_quotes(&result);

    // 2. Redact user paths
    result = redact_user_paths(&result);

    // 3. Redact meeting / conversational sensitive keywords
    let sensitive_keywords = [
        "meeting",
        "conference",
        "sync",
        "huddle",
        "transcript",
        "confidential",
        "secret",
        "board",
    ];

    for kw in &sensitive_keywords {
        let lower = result.to_lowercase();
        if let Some(pos) = lower.find(kw) {
            let end = pos + kw.len();
            result.replace_range(pos..end, "[REDACTED]");
        }
    }

    result
}

fn redact_quotes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\'' || chars[i] == '"' {
            let quote_char = chars[i];
            let start = i;
            i += 1;
            let mut found_end = false;
            while i < chars.len() {
                if chars[i] == quote_char {
                    found_end = true;
                    i += 1;
                    break;
                }
                i += 1;
            }
            if found_end && i - start > 2 {
                out.push_str("[REDACTED_CONTENT]");
            } else {
                for c in &chars[start..i] {
                    out.push(*c);
                }
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }

    out
}

fn redact_user_paths(input: &str) -> String {
    let mut res = input.to_string();
    // Unix: /home/<user>/
    if let Some(home_idx) = res.find("/home/") {
        let rest = &res[home_idx + 6..];
        if let Some(slash_idx) = rest.find('/') {
            let target = format!("/home/{}/", &rest[..slash_idx]);
            res = res.replace(&target, "/home/[REDACTED_USER]/");
        }
    }
    // Windows: C:\Users\<user>\
    if let Some(users_idx) = res.find(r"Users\") {
        let rest = &res[users_idx + 6..];
        if let Some(slash_idx) = rest.find('\\') {
            let target = format!(r"Users\{}\", &rest[..slash_idx]);
            res = res.replace(&target, r"Users\[REDACTED_USER]\");
        }
    }
    res
}

/// Sanitizes a list of crash or state transition causes.
#[must_use]
pub fn sanitize_causes(causes: &[String]) -> Vec<String> {
    causes.iter().map(|c| sanitize_text(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_empty_matches_known_hash() {
        let hex = sha256_hex(b"");
        assert_eq!(
            hex,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn salted_hash_is_deterministic_and_unique() {
        let h1 = hash_device_id("mic-1", "salt-a");
        let h2 = hash_device_id("mic-1", "salt-a");
        let h3 = hash_device_id("mic-1", "salt-b");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn sanitize_text_strips_quotes_and_keywords() {
        let dirty = "Error in 'Quarterly Sync': failed to open /home/alice/recordings.pcm";
        let cleaned = sanitize_text(dirty);
        assert!(!cleaned.contains("Quarterly Sync"));
        assert!(!cleaned.contains("alice"));
        assert!(cleaned.contains("[REDACTED_CONTENT]"));
        assert!(cleaned.contains("[REDACTED_USER]"));
    }
}
