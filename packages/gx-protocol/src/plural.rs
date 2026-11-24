//! English count/noun and count/verb agreement helpers shared across Rust crates.

use std::fmt::Display;

/// Integer count types accepted by [`plural`] and [`plural_word`].
pub trait PluralCount: Copy + Display {
    fn is_one(self) -> bool;
}

macro_rules! impl_plural_count {
    ($($t:ty),+ $(,)?) => {
        $(
            impl PluralCount for $t {
                #[inline]
                fn is_one(self) -> bool {
                    self == 1
                }
            }
        )+
    };
}

impl_plural_count!(u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize);

/// Formats `count` with the singular (`count == 1`) or plural English phrase.
///
/// Examples: `plural(1, "needs you", "need you")` -> `"1 needs you"`,
/// `plural(2, "needs you", "need you")` -> `"2 need you"`,
/// `plural(1, "agent", "agents")` -> `"1 agent"`.
#[inline]
pub fn plural<N: PluralCount>(count: N, one: &str, many: &str) -> String {
    format!("{count} {}", if count.is_one() { one } else { many })
}

/// Picks the singular (`count == 1`) or plural English word or phrase without repeating `count`.
#[inline]
pub fn plural_word<N: PluralCount>(count: N, one: &'static str, many: &'static str) -> &'static str {
    if count.is_one() { one } else { many }
}
