#![forbid(unsafe_code)]

/// The delimiter immediately before a token.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Separator {
    Start,
    Dot,
    Space,
    Dash,
    Underscore,
    Plus,
    Bracket,
    Other,
}

/// A token borrows the original input; offsets are UTF-8 byte offsets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
    pub before: Separator,
    pub bracketed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReleaseToken<'a> {
    pub text: &'a str,
    pub start: usize,
    pub end: usize,
    pub before: Separator,
    pub bracketed: bool,
    pub word: Option<crate::vocabulary::Word>,
}

impl ReleaseToken<'_> {
    pub(crate) fn is_technical(self) -> bool {
        self.word.is_some_and(crate::vocabulary::Word::boundary)
    }
}

impl<'a> From<Token<'a>> for ReleaseToken<'a> {
    fn from(token: Token<'a>) -> Self {
        Self {
            text: token.text,
            start: token.start,
            end: token.end,
            before: token.before,
            bracketed: token.bracketed,
            word: crate::vocabulary::classify(token.text),
        }
    }
}

const _: () = assert!(size_of::<ReleaseToken<'_>>() == size_of::<Token<'_>>());

pub(crate) fn release_tokens(input: &str) -> Vec<ReleaseToken<'_>> {
    tokenize_with(input, ReleaseToken::from)
}

/// Split a release name into Unicode-aware word tokens that borrow the input.
/// Punctuation remains represented by each token's preceding separator and span.
pub fn tokenize(input: &str) -> Vec<Token<'_>> {
    tokenize_with(input, |token| token)
}

fn tokenize_with<'a, T>(input: &'a str, mut make: impl FnMut(Token<'a>) -> T) -> Vec<T> {
    let mut tokens = Vec::with_capacity((input.len() / 6).min(32));
    let mut token_start = None;
    let mut before = Separator::Start;
    let mut bracket_depth = 0usize;

    for (offset, ch) in input.char_indices() {
        if ch.is_alphanumeric() {
            token_start.get_or_insert(offset);
            continue;
        }

        if let Some(start) = token_start.take() {
            tokens.push(make(Token {
                text: &input[start..offset],
                start,
                end: offset,
                before,
                bracketed: bracket_depth > 0,
            }));
        }

        before = match ch {
            '.' => Separator::Dot,
            ' ' | '\t' | '\n' | '\r' => Separator::Space,
            '-' | '‐' | '‑' | '–' | '—' => Separator::Dash,
            '_' => Separator::Underscore,
            '+' => Separator::Plus,
            '[' | '(' | '{' => {
                bracket_depth = bracket_depth.saturating_add(1);
                Separator::Bracket
            }
            ']' | ')' | '}' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                Separator::Bracket
            }
            _ => Separator::Other,
        };
    }

    if let Some(start) = token_start {
        tokens.push(make(Token {
            text: &input[start..],
            start,
            end: input.len(),
            before,
            bracketed: bracket_depth > 0,
        }));
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_unicode_byte_spans_and_delimiters() {
        let input = "[字幕组] Made.in.Abyss.S02E03.mkv";
        let tokens = tokenize(input);
        assert_eq!(tokens[0].text, "字幕组");
        assert_eq!(&input[tokens[0].start..tokens[0].end], "字幕组");
        assert!(tokens[0].bracketed);
        assert_eq!(tokens[1].text, "Made");
        assert_eq!(tokens[1].before, Separator::Space);
        assert_eq!(tokens[3].before, Separator::Dot);
    }

    #[test]
    fn tokenizes_cjk_and_ascii_markers() {
        let tokens = tokenize("沉默的荣耀.2025.S01E02");
        assert_eq!(tokens[0].text, "沉默的荣耀");
        assert_eq!(tokens[1].text, "2025");
    }
}
