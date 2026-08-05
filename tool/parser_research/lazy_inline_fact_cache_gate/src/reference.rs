use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use flark_comrak_inline_fragment_gate::{
    InlineReferenceSnapshot, InlineReferenceTarget, ReferenceDependency,
};

#[derive(Clone, Debug)]
struct SymbolState {
    symbol_id: u64,
    presence_generation: u64,
    defined: bool,
    value: Arc<str>,
}

#[derive(Clone, Debug)]
pub struct ReferenceSnapshot {
    identity: u64,
    generation: u64,
    next_symbol_id: u64,
    symbols: BTreeMap<String, SymbolState>,
    resolve_calls: Arc<AtomicUsize>,
}

impl Default for ReferenceSnapshot {
    fn default() -> Self {
        Self {
            identity: 1,
            generation: 1,
            next_symbol_id: 1,
            symbols: BTreeMap::new(),
            resolve_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl ReferenceSnapshot {
    #[must_use]
    pub fn with_symbol(mut self, normalized: &str, defined: bool, value: &str) -> Self {
        let symbol_id = self.next_symbol_id;
        self.next_symbol_id += 1;
        self.symbols.insert(
            normalized.to_owned(),
            SymbolState {
                symbol_id,
                presence_generation: u64::from(defined),
                defined,
                value: Arc::from(value),
            },
        );
        self
    }

    pub fn set_value(&mut self, normalized: &str, value: &str) -> bool {
        let Some(symbol) = self.symbols.get_mut(normalized) else {
            return false;
        };
        symbol.value = Arc::from(value);
        self.generation += 1;
        true
    }

    pub fn set_defined(&mut self, normalized: &str, defined: bool) -> bool {
        let Some(symbol) = self.symbols.get_mut(normalized) else {
            return false;
        };
        if symbol.defined != defined {
            symbol.defined = defined;
            symbol.presence_generation += 1;
            self.generation += 1;
        }
        true
    }

    #[must_use]
    pub fn dependency_is_current(&self, dependency: &ReferenceDependency) -> bool {
        self.symbols
            .get(&dependency.normalized_label)
            .is_some_and(|symbol| {
                symbol.symbol_id == dependency.symbol_id
                    && symbol.presence_generation == dependency.presence_generation
                    && symbol.defined == dependency.resolved
            })
    }

    #[must_use]
    pub fn value(&self, normalized: &str) -> Option<&str> {
        self.symbols
            .get(normalized)
            .map(|symbol| symbol.value.as_ref())
    }

    #[must_use]
    pub fn resolve_calls(&self) -> usize {
        self.resolve_calls.load(Ordering::Relaxed)
    }

    pub fn reset_resolve_calls(&self) {
        self.resolve_calls.store(0, Ordering::Relaxed);
    }
}

impl InlineReferenceSnapshot for ReferenceSnapshot {
    fn identity(&self) -> u64 {
        self.identity
    }

    fn generation(&self) -> u64 {
        self.generation
    }

    fn resolve(&self, normalized: &str, _original: &str) -> InlineReferenceTarget {
        self.resolve_calls.fetch_add(1, Ordering::Relaxed);
        self.symbols.get(normalized).map_or(
            InlineReferenceTarget {
                symbol_id: 0,
                presence_generation: 0,
                defined: false,
            },
            |symbol| InlineReferenceTarget {
                symbol_id: symbol.symbol_id,
                presence_generation: symbol.presence_generation,
                defined: symbol.defined,
            },
        )
    }
}
