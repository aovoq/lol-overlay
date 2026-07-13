//! Generic runtime routing shared by build and player-stat providers.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, RwLock};

use crate::error::{ProviderError, Result};

/// Keeps a registry and an active key without imposing fallback or
/// normalization policy on provider results.
pub struct ProviderRouter<K, P: ?Sized> {
    providers: HashMap<K, Arc<P>>,
    active: RwLock<K>,
}

impl<K, P: ?Sized> ProviderRouter<K, P>
where
    K: Copy + Eq + Hash + std::fmt::Debug,
{
    pub fn new(initial: K, providers: impl IntoIterator<Item = (K, Arc<P>)>) -> Result<Self> {
        let providers = providers.into_iter().collect::<HashMap<_, _>>();
        if !providers.contains_key(&initial) {
            return Err(ProviderError::Other(format!(
                "provider {initial:?} not registered"
            )));
        }
        Ok(Self {
            providers,
            active: RwLock::new(initial),
        })
    }

    pub fn set_active(&self, kind: K) -> Result<()> {
        if !self.providers.contains_key(&kind) {
            return Err(ProviderError::Other(format!(
                "provider {kind:?} not registered"
            )));
        }
        *self.active.write().unwrap() = kind;
        Ok(())
    }

    pub fn active(&self) -> K {
        *self.active.read().unwrap()
    }

    pub fn available_by(&self, mut compare: impl FnMut(&K, &K) -> std::cmp::Ordering) -> Vec<K> {
        let mut kinds = self.providers.keys().copied().collect::<Vec<_>>();
        kinds.sort_by(&mut compare);
        kinds
    }

    pub fn current(&self) -> Arc<P> {
        let kind = *self.active.read().unwrap();
        self.providers
            .get(&kind)
            .expect("active provider must be registered")
            .clone()
    }

    pub fn get(&self, kind: K) -> Option<Arc<P>> {
        self.providers.get(&kind).cloned()
    }

    pub fn registered(&self) -> impl Iterator<Item = &Arc<P>> {
        self.providers.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_and_rejects_unregistered_keys() {
        let router =
            ProviderRouter::new("a", [("a", Arc::new(1)), ("b", Arc::new(2))]).expect("router");
        assert_eq!(*router.current(), 1);
        router.set_active("b").expect("switch");
        assert_eq!(*router.current(), 2);
        assert!(router.set_active("missing").is_err());
        assert_eq!(router.available_by(Ord::cmp), vec!["a", "b"]);
    }

    #[test]
    fn rejects_unregistered_initial_key() {
        assert!(ProviderRouter::new("missing", [("a", Arc::new(1))]).is_err());
    }
}
