//! DMARC aggregate/failure report XML parsing (audit M-13).
//! Rejects DTD (DOCTYPE/ENTITY) before parsing so no entity expansion / XXE
//! is possible, then extracts the report identity fields used for dedup and
//! display. `quick-xml` never resolves external entities.

use anyhow::{Context, Result};

#[derive(Debug, Default, Clone)]
pub struct DmarcReportSummary {
    pub org_name: Option<String>,
    pub report_id: Option<String>,
    pub date_begin: Option<String>,
    pub date_end: Option<String>,
    pub policy_domain: Option<String>,
}

/// Parse and validate a DMARC report document.
/// Returns the root element name and the extracted summary fields.
pub fn parse_dmarc_report(xml: &str) -> Result<(String, DmarcReportSummary)> {
    let lower = xml.to_lowercase();
    if lower.contains("<!doctype") || lower.contains("<!entity") {
        anyhow::bail!("DTD not allowed in DMARC reports");
    }
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut root: Option<String> = None;
    let mut summary = DmarcReportSummary::default();
    let mut path: Vec<String> = Vec::new();
    loop {
        let event = match reader.read_event() {
            Ok(e) => e,
            Err(e) => return Err(anyhow::anyhow!(e)).context("xml parse"),
        };
        match event {
            quick_xml::events::Event::Start(e) => {
                let name = e.name().as_ref().to_string();
                if root.is_none() {
                    root = Some(name.clone());
                }
                path.push(name);
            }
            quick_xml::events::Event::End(_) => {
                path.pop();
            }
            quick_xml::events::Event::Empty(e) => {
                let name = e.name().as_ref().to_string();
                if root.is_none() {
                    root = Some(name);
                }
            }
            quick_xml::events::Event::Text(t) => {
                let text = t
                    .xml_content(quick_xml::XmlVersion::Implicit1_0)
                    .trim()
                    .to_string();
                if text.is_empty() {
                    continue;
                }
                let parent = path.last().map(|s| s.as_str()).unwrap_or("");
                match parent {
                    "org_name" => summary.org_name = Some(text),
                    "report_id" => summary.report_id = Some(text),
                    "begin" => summary.date_begin = Some(text),
                    "end" => summary.date_end = Some(text),
                    "domain" if summary.policy_domain.is_none() => {
                        summary.policy_domain = Some(text);
                    }
                    _ => {}
                }
            }
            quick_xml::events::Event::Decl(_)
            | quick_xml::events::Event::Comment(_)
            | quick_xml::events::Event::CData(_)
            | quick_xml::events::Event::PI(_)
            | quick_xml::events::Event::GeneralRef(_) => {}
            quick_xml::events::Event::DocType(_) => {
                anyhow::bail!("DOCTYPE not allowed in DMARC reports");
            }
            quick_xml::events::Event::Eof => break,
        }
    }
    let root = root.context("no root element")?;
    if root != "feedback" {
        anyhow::bail!("not a DMARC report (root={root})");
    }
    Ok((root, summary))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feedback>
  <report_metadata>
    <org_name>Example Reporter</org_name>
    <report_id>abc123</report_id>
    <date_range><begin>1700000000</begin><end>1700086400</end></date_range>
  </report_metadata>
  <policy><domain>example.com</domain><p>none</p></policy>
  <record><row><source_ip>203.0.113.1</source_ip></row></record>
</feedback>"#;

    #[test]
    fn parses_sample() {
        let (root, s) = parse_dmarc_report(SAMPLE).unwrap();
        assert_eq!(root, "feedback");
        assert_eq!(s.org_name.as_deref(), Some("Example Reporter"));
        assert_eq!(s.report_id.as_deref(), Some("abc123"));
        assert_eq!(s.date_begin.as_deref(), Some("1700000000"));
        assert_eq!(s.policy_domain.as_deref(), Some("example.com"));
    }

    #[test]
    fn rejects_dtd() {
        let xxe =
            "<!DOCTYPE foo [<!ENTITY xxe SYSTEM 'file:///etc/passwd'>]><feedback>&xxe;</feedback>";
        assert!(parse_dmarc_report(xxe).is_err());
    }

    #[test]
    fn rejects_non_feedback_root() {
        assert!(parse_dmarc_report("<root/>").is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_dmarc_report("not xml at all").is_err());
    }
}
