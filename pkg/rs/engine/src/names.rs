//! Interned names: every name a definition uses is numbered once at
//! compile time, so catalog lookups compare integers, not strings.

use std::collections::BTreeMap;

/// An interned name, numbered by the [`Names`] table of its definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sym(u32);

impl Sym {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The names of one compiled definition, in first-use order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Names {
    spelled: Vec<Box<str>>,
    index: BTreeMap<Box<str>, Sym>,
}

impl Names {
    pub(crate) fn intern(&mut self, name: &str) -> Sym {
        if let Some(sym) = self.index.get(name) {
            return *sym;
        }
        let sym = Sym(u32::try_from(self.spelled.len()).unwrap_or(u32::MAX));
        self.spelled.push(name.into());
        self.index.insert(name.into(), sym);
        sym
    }

    /// The symbol of a name, when the definition uses it at all.
    pub fn get(&self, name: &str) -> Option<Sym> {
        self.index.get(name).copied()
    }

    /// The spelling of a symbol; empty for a symbol of another table.
    pub fn resolve(&self, sym: Sym) -> &str {
        self.spelled.get(sym.index()).map_or("", AsRef::as_ref)
    }

    pub fn len(&self) -> usize {
        self.spelled.len()
    }

    pub fn is_empty(&self) -> bool {
        self.spelled.is_empty()
    }
}
