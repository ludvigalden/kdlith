//! Fail-closed resource limits, checked before any parser buffer grows.

/// Resource limits for one parse. A breach fails the parse closed with a
/// limit diagnostic instead of growing a buffer or a stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Limits {
    pub(crate) max_source_bytes: usize,
    pub(crate) max_token_bytes: usize,
    pub(crate) max_string_lines: usize,
    pub(crate) max_depth: usize,
    pub(crate) max_comment_depth: usize,
}

impl Limits {
    /// The default profile: 4 GiB sources (offsets fit `u32`), 8 MiB
    /// tokens, one million lines per multi-line string, 512 levels of
    /// children blocks, 128 levels of nested block comments.
    pub const DEFAULT: Self = Self {
        max_source_bytes: u32::MAX as usize,
        max_token_bytes: 8 * 1024 * 1024,
        max_string_lines: 1_000_000,
        max_depth: 512,
        max_comment_depth: 128,
    };

    /// Cap the source length in bytes; spans are `u32`, so the cap never
    /// exceeds the default.
    pub const fn with_max_source_bytes(mut self, max: usize) -> Self {
        let ceiling = Self::DEFAULT.max_source_bytes;
        self.max_source_bytes = if max < ceiling { max } else { ceiling };
        self
    }

    /// Cap the raw length in bytes of one string, identifier, or number;
    /// this also bounds its decoded text.
    pub const fn with_max_token_bytes(mut self, max: usize) -> Self {
        self.max_token_bytes = max;
        self
    }

    /// Cap the number of lines in one multi-line string.
    pub const fn with_max_string_lines(mut self, max: usize) -> Self {
        self.max_string_lines = max;
        self
    }

    /// Cap the nesting depth of children blocks, slashdashed ones included.
    pub const fn with_max_depth(mut self, max: usize) -> Self {
        self.max_depth = max;
        self
    }

    /// Cap the nesting depth of block comments.
    pub const fn with_max_comment_depth(mut self, max: usize) -> Self {
        self.max_comment_depth = max;
        self
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self::DEFAULT
    }
}
