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

    // 6. General Syntactic Fallback
    let rej_first = rej.lines().find(|l| !l.trim().is_empty()).unwrap_or("candidate logic").trim();
    let acc_first = acc.lines().find(|l| !l.trim().is_empty()).unwrap_or("clean resolution").trim();

    let mistake = format!("Rejected draft logic{}: `{}`", ctx, truncate_line(rej_first, 60));
    let fix = format!("Committed resolution: `{}`", truncate_line(acc_first, 60));
    (mistake, fix)
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
    if s.len() > max {
        &s[..max]
    } else {
        s
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
}
