use crate::correlator::CorrelatedHunkPair;
use crate::store::rule::EpisodicRule;

pub fn derive_path_glob(file_path: &str) -> String {
    let normalized = file_path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() <= 1 {
        normalized
    } else {
        let parent = parts[..parts.len() - 1].join("/");
        format!("{}/**", parent)
    }
}

pub fn distill_rule(pair: &CorrelatedHunkPair) -> Option<EpisodicRule> {
    let rej_adds = pair.rejected_hunk.added_content();
    let acc_adds = pair.accepted_hunk.added_content();

    let rej_text = rej_adds.join("\n");
    let acc_text = acc_adds.join("\n");

    if rej_text.trim().is_empty() && acc_text.trim().is_empty() {
        return None;
    }

    let path_glob = derive_path_glob(&pair.file_path);
    let (mistake, fix) = extract_semantics(&pair.rejected_hunk.heading, &rej_text, &acc_text);

    let hunk_rejected = format_snippet(&rej_adds);
    let hunk_accepted = format_snippet(&acc_adds);

    Some(EpisodicRule::new(
        path_glob,
        mistake,
        fix,
        hunk_rejected,
        hunk_accepted,
    ))
}

pub fn extract_semantics(heading: &str, rej: &str, acc: &str) -> (String, String) {
    let ctx = if !heading.is_empty() {
        format!(" in `{}`", heading.split('(').next().unwrap_or(heading).trim())
    } else {
        String::new()
    };

    // 1. Unwrap / Panic
    if (rej.contains(".unwrap()") || rej.contains(".expect(") || rej.contains("panic!("))
        && (acc.contains('?') || acc.contains("map_err") || acc.contains("match ") || acc.contains("if let"))
    {
        return (
            format!("Used unwrap()/panic! on Result/Option{}", ctx),
            "Propagate error via `?` or `map_err`".to_string(),
        );
    }

    // 2. Float currency / Precision
    if (rej.contains("as f64") || rej.contains("as f32") || rej.contains("parseFloat") || rej.contains("* 0.") || rej.contains("0.05"))
        && (acc.contains("BigInt") || acc.contains("Decimal") || acc.contains("saturating_mul") || acc.contains("checked_mul") || acc.contains("checked_div") || acc.contains("bps") || acc.contains("basis"))
    {
        return (
            format!("Used floating-point math for currency/exact values{}", ctx),
            "Use Decimal, BigInt, or integer basis point arithmetic".to_string(),
        );
    }

    // 3. Brittle Test Mocks
    if (rej.contains("mock(") || rej.contains("jest.fn(") || rej.contains("sinon") || rej.contains("Mock::"))
        && (acc.contains("testcontainers") || acc.contains("fixture") || acc.contains("tempdir") || acc.contains("setup_"))
    {
        return (
            format!("Used brittle mocks in test suite{}", ctx),
            "Use isolated fixtures or testcontainers".to_string(),
        );
    }

    // 4. Hardcoded URLs / Ports
    if (rej.contains("http://localhost") || rej.contains("127.0.0.1") || rej.contains("8080") || rej.contains("3000"))
        && (acc.contains("env::var") || acc.contains("process.env") || acc.contains("config.") || acc.contains("Config::"))
    {
        return (
            format!("Hardcoded host or port{}", ctx),
            "Load endpoint and configuration from environment".to_string(),
        );
    }

    // 5. Mutable globals
    if (rej.contains("static mut") || rej.contains("global.") || rej.contains("window."))
        && (acc.contains("Arc<Mutex<") || acc.contains("RwLock<") || acc.contains("Atomic") || acc.contains("thread_local!"))
    {
        return (
            format!("Directly mutated global or static state{}", ctx),
            "Use thread-safe primitives (Arc<Mutex> or Atomic)".to_string(),
        );
    }

    // 6. SQL string interpolation -> Parameterized query
    if (rej.contains("format!(\"SELECT") || rej.contains("format!(\"INSERT") || rej.contains("format!(\"UPDATE") || rej.contains("f\"SELECT "))
        && (acc.contains("$1") || acc.contains("?") || acc.contains(".bind(") || acc.contains("query_as"))
    {
        return (
            format!("Constructed SQL query via string interpolation{}", ctx),
            "Use parameterized queries with bound placeholders ($1 / ?)".to_string(),
        );
    }

    // 7. Blocking I/O or sleep in async context
    if (rej.contains("std::thread::sleep") || rej.contains("std::fs::"))
        && (acc.contains("tokio::time::sleep") || acc.contains("tokio::fs::"))
    {
        return (
            format!("Used blocking std I/O or thread::sleep inside async path{}", ctx),
            "Use non-blocking tokio::fs / tokio::time::sleep equivalents".to_string(),
        );
    }

    // 8. UTF-8 byte slicing -> char-boundary safe iteration
    if (rej.contains("[..") && !rej.contains("chars()"))
        && (acc.contains(".chars().take(") || acc.contains(".char_indices()"))
    {
        return (
            format!("Sliced string by raw byte index{}", ctx),
            "Use .chars().take() or .char_indices() to respect UTF-8 boundaries".to_string(),
        );
    }

    // 9. Optional LLM / Ollama semantic distillation when configured
    if let Some(pair) = try_llm_distill(&ctx, rej, acc) {
        return pair;
    }

    // 10. General Syntactic Fallback
    let rej_first = rej.lines().find(|l| !l.trim().is_empty()).unwrap_or("candidate logic").trim();
    let acc_first = acc.lines().find(|l| !l.trim().is_empty()).unwrap_or("clean resolution").trim();

    let mistake = format!("Rejected draft logic{}: `{}`", ctx, truncate_line(rej_first, 60));
    let fix = format!("Committed resolution: `{}`", truncate_line(acc_first, 60));
    (mistake, fix)
}

fn try_llm_distill(ctx: &str, rej: &str, acc: &str) -> Option<(String, String)> {
    let (url, api_key, model, is_anthropic) = if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        (
            "https://api.anthropic.com/v1/messages".to_string(),
            Some(key),
            std::env::var("DIFFMEM_MODEL").unwrap_or_else(|_| "claude-3-5-haiku-20241022".to_string()),
            true,
        )
    } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        (
            "https://api.openai.com/v1/chat/completions".to_string(),
            Some(key),
            std::env::var("DIFFMEM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string()),
            false,
        )
    } else if let Ok(custom_url) = std::env::var("DIFFMEM_LLM_URL") {
        (
            custom_url,
            std::env::var("DIFFMEM_API_KEY").ok(),
            std::env::var("DIFFMEM_MODEL").unwrap_or_else(|_| "default".to_string()),
            false,
        )
    } else {
        return None;
    };

    let prompt = format!(
        "Compare the rejected code hunk against the accepted fix{ctx}.\n\
         Rejected:\n{rej}\n\nAccepted:\n{acc}\n\n\
         Output strictly JSON: {{\"mistake\":\"<concise 1-line mistake>\",\"fix\":\"<concise 1-line rule>\"}}"
    );

    let payload = if is_anthropic {
        serde_json::json!({
            "model": model,
            "max_tokens": 200,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.0
        })
    } else {
        serde_json::json!({
            "model": model,
            "messages": [
                {"role": "system", "content": "Output strictly JSON with keys 'mistake' and 'fix'."},
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.0
        })
    };

    let mut req = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(5))
        .set("Content-Type", "application/json");
    if let Some(k) = &api_key {
        if is_anthropic {
            req = req.set("x-api-key", k).set("anthropic-version", "2023-06-01");
        } else {
            req = req.set("Authorization", &format!("Bearer {k}"));
        }
    }

    let resp = req.send_json(payload).ok()?;
    let json: serde_json::Value = resp.into_json().ok()?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .or_else(|| json["content"][0]["text"].as_str())?;
    let clean = content
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: serde_json::Value = serde_json::from_str(clean).ok()?;
    let mistake = parsed.get("mistake")?.as_str()?.trim().to_string();
    let fix = parsed.get("fix")?.as_str()?.trim().to_string();
    if mistake.is_empty() || fix.is_empty() {
        return None;
    }
    Some((mistake, fix))
}

pub fn format_snippet(lines: &[&str]) -> String {
    lines
        .iter()
        .take(5)
        .map(|l| {
            let trimmed = l.trim();
            if trimmed.starts_with('+') || trimmed.starts_with('-') {
                trimmed.to_string()
            } else {
                format!("+ {}", trimmed)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_line(s: &str, max: usize) -> &str {
    if s.chars().count() <= max {
        return s;
    }
    match s.char_indices().nth(max) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_path_glob() {
        assert_eq!(derive_path_glob("Cargo.toml"), "Cargo.toml");
        assert_eq!(derive_path_glob("src/main.rs"), "src/**");
        assert_eq!(derive_path_glob("src/payments/calc.rs"), "src/payments/**");
        assert_eq!(derive_path_glob("src\\auth\\token.rs"), "src/auth/**");
    }

    #[test]
    fn test_extract_semantics_unwrap_panic() {
        let (mistake, fix) = extract_semantics(
            "pub fn parse()",
            "let x = val.unwrap();",
            "let x = val?;"
        );
        assert!(mistake.contains("unwrap()"));
        assert!(fix.contains("?"));
    }

    #[test]
    fn test_extract_semantics_float_currency() {
        let (mistake, fix) = extract_semantics(
            "pub fn calculate_fee()",
            "let fee = (amount as f64 * 0.05) as u64;",
            "let fee = amount.checked_mul(500)? / 10000;"
        );
        assert!(mistake.contains("floating-point"));
        assert!(fix.contains("basis point"));
    }

    #[test]
    fn test_extract_semantics_hardcoded_url() {
        let (mistake, fix) = extract_semantics(
            "pub fn connect()",
            "let url = \"http://localhost:8080\";",
            "let url = env::var(\"API_URL\").unwrap();"
        );
        assert!(mistake.contains("Hardcoded host or port"));
        assert!(fix.contains("environment"));
    }

    #[test]
    fn test_extract_semantics_brittle_mocks() {
        let (mistake, fix) = extract_semantics(
            "fn test_database()",
            "let m = mock(\"query\");",
            "let container = testcontainers::clients::Cli::default();"
        );
        assert!(mistake.contains("brittle mocks"));
        assert!(fix.contains("testcontainers"));
    }

    #[test]
    fn test_extract_semantics_mutable_globals() {
        let (mistake, fix) = extract_semantics(
            "pub fn set_state()",
            "static mut COUNTER: u32 = 0;",
            "static COUNTER: AtomicU32 = AtomicU32::new(0);"
        );
        assert!(mistake.contains("mutated global"));
        assert!(fix.contains("thread-safe"));
    }

    #[test]
    fn test_extract_semantics_sql_interpolation_and_blocking_async() {
        let (m1, f1) = extract_semantics(
            "pub async fn find_user()",
            "let q = format!(\"SELECT * FROM users WHERE id = {}\", id);",
            "let q = sqlx::query_as(\"SELECT * FROM users WHERE id = $1\").bind(id);"
        );
        assert!(m1.contains("SQL query via string interpolation"));
        assert!(f1.contains("parameterized"));

        let (m2, f2) = extract_semantics(
            "pub async fn poll()",
            "std::thread::sleep(Duration::from_secs(1));",
            "tokio::time::sleep(Duration::from_secs(1)).await;"
        );
        assert!(m2.contains("blocking std I/O"));
        assert!(f2.contains("tokio"));
    }
}
