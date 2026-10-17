//! Selector statics, so that "what a site's HTML looks like" is one screen.
//!
//! A selector is a literal, and a literal parses once: the macro wraps it in a
//! `LazyLock<Selector>` so a request path never pays for it twice, and a typo is
//! a panic at first use rather than a silent no-match.
//!
//! The expansion names `Selector` and `LazyLock`, so a module that uses it has
//! to have those two in scope.

macro_rules! selector {
    ($name:ident, $css:literal) => {
        static $name: LazyLock<Selector> =
            LazyLock::new(|| Selector::parse($css).expect("selector literals are valid"));
    };
}

pub(crate) use selector;
