/// True if `name` is safe as a file name and as an argument: letters, digits,
/// dots, dashes and underscores, starting with a letter or a digit.
pub(crate) fn valid_name(name: &str) -> bool {
    name.len() <= 100
        && name.starts_with(|first: char| first.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_names() {
        assert!(valid_name("com.example.app_1"));
        assert!(valid_name("pixel-6"));
        assert!(!valid_name(""));
        assert!(!valid_name("-rf"));
        assert!(!valid_name("../etc"));
        assert!(!valid_name("a b"));
        assert!(!valid_name("a/b"));
    }
}
