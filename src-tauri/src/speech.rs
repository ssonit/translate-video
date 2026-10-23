pub(crate) fn clean_for_speech(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0u32;
    for ch in text.chars() {
        match ch {
            '(' | '（' | '[' => depth += 1,
            ')' | '）' | ']' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            '&' => out.push_str(" và "),
            '®' | '™' | '©' | '*' | '#' | '_' | '~' | '`' | '"' | '“' | '”' | '\n' | '\r'
            | '\t' => out.push(' '),
            _ => out.push(ch),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn is_speakable(text: &str) -> bool {
    text.chars().filter(|ch| ch.is_alphanumeric()).count() >= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleaning_drops_remarks_and_symbols() {
        assert_eq!(
            clean_for_speech("Xin chào (cười) & bạn™"),
            "Xin chào và bạn"
        );
        assert_eq!(clean_for_speech("  a\nb\t*c*  "), "a b c");
        assert_eq!(clean_for_speech("[nhạc] Hết."), "Hết.");
    }

    #[test]
    fn one_letter_or_punctuation_is_not_spoken() {
        assert!(!is_speakable(""));
        assert!(!is_speakable("..."));
        assert!(!is_speakable("a."));
        assert!(!is_speakable("Ừ."));
        assert!(is_speakable("Dạ."));
        assert!(is_speakable("12"));
    }
}
