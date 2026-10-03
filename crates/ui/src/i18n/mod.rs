//! The text busy's windows show, in the UI language (`busy_core::Lang`).
//!
//! Every language is one `Strings` value, compiled in: the English one is assembled from each section's `EN`
//! (`common.rs`, …, where `section!` declares each string with its English text), the others are in `lang/`.
//! A missing translation is a build error, not an English string at run time. `t()` is the current language's table;
//! the app sets the language at startup (`init`) and when the config changes (`select`).
//!
//! A string with values in it is a pattern with numbered slots, `{0}`, `{1}`, so a translation can reorder
//! them; `fill` puts the values in. A string that depends on a count is a `Plural`, chosen by
//! `plural`.

/// Declares a section of `Strings`: the struct, its English text as `EN`, and `texts`, every string of it
/// with its field's name, which the tests walk to check all languages alike. A field is a `&'static str` or a
/// `Plural`.
macro_rules! section {
    ($(#[$m:meta])* $name:ident { $($(#[$fm:meta])* $field:ident: $ty:ty = $en:expr,)* }) => {
        $(#[$m])*
        pub struct $name {
            $($(#[$fm])* pub $field: $ty,)*
        }

        pub(super) const EN: $name = $name { $($field: $en,)* };

        impl $name {
            #[cfg(test)]
            pub(super) fn texts(&self) -> Vec<super::Entry> {
                let mut v = Vec::new();
                $(super::Text::collect(&self.$field, stringify!($field), &mut v);)*
                v
            }
        }
    };
}

mod a11y;
mod cell;
mod choices;
mod common;
mod dialogs;
mod flyout;
mod menu;
mod settings;
mod setup;
mod time;

pub use a11y::A11y;
pub use cell::Cell;
pub use choices::Choices;
pub use common::Common;
pub use dialogs::Dialogs;
pub use flyout::Flyout;
pub use menu::Menu;
pub use settings::Settings;
pub use setup::Setup;
pub use time::Time;

use busy_core::Lang;
use std::fmt::{Display, Write};
use std::sync::atomic::{AtomicU8, Ordering};

/// One language's text, by the part of the UI that shows it.
pub struct Strings {
    pub common: Common,
    pub time: Time,
    pub cell: Cell,
    pub flyout: Flyout,
    pub menu: Menu,
    pub settings: Settings,
    pub choices: Choices,
    pub setup: Setup,
    pub a11y: A11y,
    pub dialogs: Dialogs,
}

/// The forms of a string that depends on a count `n`, in CLDR's plural categories as far as busy's languages
/// use them for whole numbers: `one` (English and German 1; French 0 and 1; Russian 1, 21, 31…), `few`
/// (Russian and Polish 2–4, 22–24…) and `other` (the rest). A language without `few` repeats `other` there;
/// Japanese, Korean and Chinese have one form, repeated in all three. Each form is a `fill` pattern, `{0}`
/// being the count.
pub struct Plural {
    pub one: &'static str,
    pub few: &'static str,
    pub other: &'static str,
}

impl Strings {
    /// Every string in declaration order.
    #[cfg(test)]
    fn texts(&self) -> Vec<Entry> {
        let mut v = self.common.texts();
        v.extend(self.time.texts());
        v.extend(self.cell.texts());
        v.extend(self.flyout.texts());
        v.extend(self.menu.texts());
        v.extend(self.settings.texts());
        v.extend(self.choices.texts());
        v.extend(self.setup.texts());
        v.extend(self.a11y.texts());
        v.extend(self.dialogs.texts());
        v
    }
}

/// One string of a section, for tests: (field name, text, whether it is a form of a `Plural`).
#[cfg(test)]
type Entry = (&'static str, &'static str, bool);

/// A string or the forms of a `Plural`, for `texts`.
#[cfg(test)]
trait Text {
    fn collect(&self, name: &'static str, out: &mut Vec<Entry>);
}

#[cfg(test)]
impl Text for &'static str {
    fn collect(&self, name: &'static str, out: &mut Vec<Entry>) {
        out.push((name, self, false));
    }
}

#[cfg(test)]
impl Text for Plural {
    fn collect(&self, name: &'static str, out: &mut Vec<Entry>) {
        out.extend([self.one, self.few, self.other].map(|s| (name, s, true)));
    }
}

static EN: Strings = Strings {
    common: common::EN,
    time: time::EN,
    cell: cell::EN,
    flyout: flyout::EN,
    menu: menu::EN,
    settings: settings::EN,
    choices: choices::EN,
    setup: setup::EN,
    a11y: a11y::EN,
    dialogs: dialogs::EN,
};

/// The language the system asks for (`init`), and the one shown, as `Lang::index`.
static SYSTEM: AtomicU8 = AtomicU8::new(0);
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// Sets the language `select(None)` stands for, the first of Windows' display languages busy has
/// (`busy_win::ui_languages`, read once at startup), and selects `choice`.
pub fn init(system: Lang, choice: Option<Lang>) {
    SYSTEM.store(system.index() as u8, Ordering::Relaxed);
    select(choice);
}

/// Shows `choice`, or the system's language for `None` (`Config::language`). Windows drawn afterwards use it.
pub fn select(choice: Option<Lang>) {
    let lang = choice.unwrap_or_else(|| from_index(SYSTEM.load(Ordering::Relaxed)));
    CURRENT.store(lang.index() as u8, Ordering::Relaxed);
}

/// The language shown.
pub fn lang() -> Lang {
    from_index(CURRENT.load(Ordering::Relaxed))
}

fn from_index(i: u8) -> Lang {
    Lang::ALL.get(usize::from(i)).copied().unwrap_or(Lang::En)
}

/// The current language's text.
pub fn t() -> &'static Strings {
    strings(lang())
}

/// One language's text.
pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::En => &EN,
        _ => &EN,
    }
}

/// `pattern` with each `{i}` replaced by `args[i]`. A slot without an argument is left out.
pub fn fill(pattern: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(pattern.len() + 16 * args.len());
    let mut rest = pattern;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let tail = &rest[open + 1..];
        match tail.find('}').and_then(|close| Some((close, tail[..close].parse::<usize>().ok()?))) {
            Some((close, i)) => {
                if let Some(a) = args.get(i) {
                    let _ = write!(out, "{a}");
                }
                rest = &tail[close + 1..];
            }
            None => {
                out.push('{');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The form of `p` for `n` in the current language, filled with `n`.
pub fn plural(p: &Plural, n: u64) -> String {
    fill(form(lang(), p, n), &[&n])
}

/// The form of `p` for `n` in the current language, filled with `args`: for a count that shares the string
/// with other values, `{0}` being the count (pass `n` first) and `{1}`… the others.
pub fn plural_with(p: &Plural, n: u64, args: &[&dyn Display]) -> String {
    fill(form(lang(), p, n), args)
}

/// The form of `p` that `lang`'s plural rules pick for `n`.
fn form(lang: Lang, p: &Plural, n: u64) -> &'static str {
    let (n10, n100) = (n % 10, n % 100);
    let slavic_few = (2..=4).contains(&n10) && !(12..=14).contains(&n100);
    match lang {
        Lang::Ja | Lang::Ko | Lang::ZhHans => p.other,
        Lang::Fr | Lang::PtBr if n <= 1 => p.one,
        Lang::Ru if n10 == 1 && n100 != 11 => p.one,
        Lang::Ru | Lang::Pl if slavic_few => p.few,
        Lang::Pl if n == 1 => p.one,
        Lang::Ru | Lang::Pl | Lang::Fr | Lang::PtBr => p.other,
        _ if n == 1 => p.one,
        _ => p.other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_slots_in_any_order() {
        assert_eq!(fill("{1} of {0}", &[&"a", &2]), "2 of a");
        assert_eq!(fill("{0}{0}", &[&1]), "11");
        assert_eq!(fill("no slots", &[&1]), "no slots");
        assert_eq!(fill("{2} {x} {", &[&1]), " {x} {");
    }

    #[test]
    fn plural_rules() {
        let p = Plural { one: "one", few: "few", other: "other" };
        let forms = |l| [0, 1, 2, 5, 11, 21, 22, 25, 112].map(|n| form(l, &p, n));
        assert_eq!(forms(Lang::En), ["other", "one", "other", "other", "other", "other", "other", "other", "other"]);
        assert_eq!(forms(Lang::Fr), ["one", "one", "other", "other", "other", "other", "other", "other", "other"]);
        assert_eq!(forms(Lang::Ru), ["other", "one", "few", "other", "other", "one", "few", "other", "other"]);
        assert_eq!(forms(Lang::Pl), ["other", "one", "few", "other", "other", "other", "few", "other", "other"]);
        assert_eq!(forms(Lang::Ja), ["other"; 9]);
    }

    #[test]
    fn a_plural_with_more_values() {
        let p = Plural { one: "{0} file on {1}", few: "{0} files on {1}", other: "{0} files on {1}" };
        assert_eq!(plural_with(&p, 1, &[&1, &"C:"]), "1 file on C:");
        assert_eq!(plural_with(&p, 3, &[&3, &"C:"]), "3 files on C:");
    }

    /// The `{i}` slots of a pattern, sorted.
    fn slots(s: &str) -> Vec<usize> {
        let mut v: Vec<usize> =
            s.split('{').skip(1).filter_map(|p| p.split_once('}').and_then(|(i, _)| i.parse().ok())).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Every string of every language is there and has the slots its English one has, so no value is lost or
    /// made up. A `Plural`'s forms may drop `{0}` ("one minute"), never add another slot.
    #[test]
    fn translations_are_complete() {
        let en = EN.texts();
        for l in Lang::ALL {
            let texts = strings(l).texts();
            assert_eq!(texts.len(), en.len(), "{l:?}");
            for (&(field, s, plural), &(_, e, _)) in texts.iter().zip(&en) {
                assert!(!s.trim().is_empty(), "{l:?} {field} is empty");
                let (have, want) = (slots(s), slots(e));
                let dropped_count = plural && have.is_empty() && want == [0];
                assert!(have == want || dropped_count, "{l:?} {field}: {s:?} vs {e:?}");
            }
        }
    }
}
