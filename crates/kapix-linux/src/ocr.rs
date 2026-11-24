//! Offline OCR via the `tesseract` CLI (zero C linking — keeps the binary pure Rust).

use std::io::Write;
use std::process::{Command, Stdio};

use kapix_core::Image;

use crate::{Error, Result};

/// Map a Kapix `ocr.language` config string (BCP-47 tag, `"auto"`, or native Tesseract code)
/// to an optional Tesseract `-l` argument.
pub fn map_tesseract_lang(lang: &str) -> Option<String> {
    let trimmed = lang.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        return None;
    }
    // Preserve multi-language Tesseract specs like `eng+jpn` or 3-letter codes directly.
    if trimmed.contains('+') || trimmed.contains('_') {
        return Some(trimmed.to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    let mapped = match lower.as_str() {
        "zh-cn" | "zh-hans" | "zh-sg" => "chi_sim",
        "zh-tw" | "zh-hant" | "zh-hk" => "chi_tra",
        _ => {
            let primary = lower.split('-').next().unwrap_or(&lower);
            match primary {
                "en" => "eng",
                "id" => "ind",
                "ja" => "jpn",
                "ko" => "kor",
                "zh" => "chi_sim",
                "fr" => "fra",
                "de" => "deu",
                "es" => "spa",
                "pt" => "por",
                "it" => "ita",
                "nl" => "nld",
                "ru" => "rus",
                "ar" => "ara",
                "hi" => "hin",
                "vi" => "vie",
                "th" => "tha",
                "pl" => "pol",
                "tr" => "tur",
                "uk" => "ukr",
                _ => trimmed,
            }
        }
    };
    Some(mapped.to_string())
}

/// Recognize text lines in `img` using `tesseract stdin stdout`.
pub fn recognize(img: &Image, lang: &str) -> Result<Vec<String>> {
    if img.width == 0 || img.height == 0 {
        return Ok(Vec::new());
    }
    let png = img.encode_png().map_err(|e| Error(e.to_string()))?;

    let mut cmd = Command::new("tesseract");
    cmd.arg("stdin")
        .arg("stdout")
        .arg("--psm")
        .arg("3")
        .arg("quiet");
    if let Some(code) = map_tesseract_lang(lang) {
        cmd.arg("-l").arg(code);
    }

    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error(
                    "tesseract not found — install `tesseract` and a language pack (e.g. `tesseract-data-eng`)"
                        .into(),
                )
            } else {
                Error(format!("failed to run tesseract: {e}"))
            }
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&png);
    }

    let out = child
        .wait_with_output()
        .map_err(|e| Error(format!("waiting for tesseract failed: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(Error(if stderr.is_empty() {
            format!("tesseract exited with {}", out.status)
        } else {
            format!("tesseract failed: {stderr}")
        }));
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_bcp47_and_tesseract_codes() {
        assert_eq!(map_tesseract_lang("auto"), None);
        assert_eq!(map_tesseract_lang(""), None);
        assert_eq!(map_tesseract_lang("en-US").as_deref(), Some("eng"));
        assert_eq!(map_tesseract_lang("ja").as_deref(), Some("jpn"));
        assert_eq!(map_tesseract_lang("zh-CN").as_deref(), Some("chi_sim"));
        assert_eq!(map_tesseract_lang("zh-TW").as_deref(), Some("chi_tra"));
        assert_eq!(map_tesseract_lang("eng+jpn").as_deref(), Some("eng+jpn"));
        assert_eq!(map_tesseract_lang("ind").as_deref(), Some("ind"));
    }
}
