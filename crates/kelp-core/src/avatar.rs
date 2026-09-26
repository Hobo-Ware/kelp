pub fn initials(name: &str) -> String {
    let mut words = name
        .split_whitespace()
        .filter(|w| w.chars().next().is_some_and(char::is_alphanumeric));
    let first = words.next().and_then(|w| w.chars().next());
    let last = words.next_back().and_then(|w| w.chars().next());
    match (first, last) {
        (Some(a), Some(b)) => format!("{a}{b}").to_uppercase(),
        (Some(a), None) => a.to_uppercase().to_string(),
        _ => "?".into(),
    }
}

pub fn color_index(email: &str, colors: usize) -> usize {
    let hash = email
        .trim()
        .to_lowercase()
        .bytes()
        .fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ b as u64).wrapping_mul(0x100000001b3)
        });
    (hash % colors as u64) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_use_first_and_last_word() {
        assert_eq!(initials("Vlad Jerca"), "VJ");
        assert_eq!(initials("Mara Elena Ionescu"), "MI");
        assert_eq!(initials("dependabot[bot]"), "D");
        assert_eq!(initials("  "), "?");
    }

    #[test]
    fn same_email_always_gets_the_same_color() {
        assert_eq!(
            color_index("Vlad@Trakt.tv ", 8),
            color_index("vlad@trakt.tv", 8)
        );
    }
}
