//! Deliverability DNS checks via DNS-over-HTTPS (Cloudflare DoH, JSON).
//! No extra crates: uses reqwest (already in workspace).

pub async fn doh_query(
    client: &reqwest::Client,
    name: &str,
    rtype: &str,
) -> Option<serde_json::Value> {
    let url = "https://cloudflare-dns.com/dns-query";
    let resp = client
        .get(url)
        .query(&[("name", name), ("type", rtype)])
        .header("Accept", "application/dns-json")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .ok()?;
    let v: serde_json::Value = resp.json().await.ok()?;
    Some(v)
}

pub fn extract_txt(json: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(arr) = json.get("Answer").and_then(|a| a.as_array()) {
        for a in arr {
            if let Some(data) = a.get("data").and_then(|d| d.as_str()) {
                out.push(
                    data.trim_start_matches('"')
                        .trim_end_matches('"')
                        .to_string(),
                );
            }
        }
    }
    out
}

pub fn extract_mx(json: &serde_json::Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(arr) = json.get("Answer").and_then(|a| a.as_array()) {
        for a in arr {
            if let Some(data) = a.get("data").and_then(|d| d.as_str()) {
                // "10 mail.example.com."
                out.push(data.trim_end_matches('.').to_string());
            }
        }
    }
    out
}

pub fn check_spf(txts: &[String]) -> bool {
    txts.iter().any(|t| t.starts_with("v=spf1"))
}

pub fn check_dmarc(txts: &[String]) -> bool {
    txts.iter().any(|t| t.starts_with("v=DMARC1"))
}

pub fn check_dkim(txts: &[String]) -> bool {
    txts.iter().any(|t| t.starts_with("v=DKIM1"))
}

pub fn check_sts(txts: &[String]) -> bool {
    txts.iter().any(|t| t.contains("v=STSv1"))
}

pub fn check_tls_rpt(txts: &[String]) -> bool {
    txts.iter().any(|t| t.contains("v=TLSRPTv1"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spf_detection() {
        assert!(check_spf(&["v=spf1 a -all".to_string()]));
        assert!(!check_spf(&["v=DMARC1; p=none".to_string()]));
    }
    #[test]
    fn dmarc_detection() {
        assert!(check_dmarc(&["v=DMARC1; p=quarantine".to_string()]));
    }
    #[test]
    fn dkim_detection() {
        assert!(check_dkim(&["v=DKIM1; k=rsa; p=abc".to_string()]));
    }
    #[test]
    fn sts_tlsrpt() {
        assert!(check_sts(&["v=STSv1".to_string()]));
        assert!(check_tls_rpt(&["v=TLSRPTv1;".to_string()]));
    }
}
