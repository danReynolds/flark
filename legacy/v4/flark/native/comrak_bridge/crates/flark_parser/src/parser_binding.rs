use flark_engine::ParserProfileId;

/// Grammar partition bound into recursive-Green parser work.
pub const M11_GRAMMAR_REVISION: u32 = 9;

/// Parser profile and grammar partition for one recursive-Green parse.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct M11ParserBinding {
    syntax_profile: ParserProfileId,
    grammar_revision: u32,
}

impl M11ParserBinding {
    #[must_use]
    pub const fn new(syntax_profile: ParserProfileId, grammar_revision: u32) -> Self {
        Self {
            syntax_profile,
            grammar_revision,
        }
    }

    #[must_use]
    pub const fn current(syntax_profile: ParserProfileId) -> Self {
        Self::new(syntax_profile, M11_GRAMMAR_REVISION)
    }

    #[must_use]
    pub const fn syntax_profile(self) -> ParserProfileId {
        self.syntax_profile
    }

    #[must_use]
    pub const fn grammar_revision(self) -> u32 {
        self.grammar_revision
    }
}
