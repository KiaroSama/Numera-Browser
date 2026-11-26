use super::registry::OptionsFamily;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io, sync::Arc};

pub type EngineError = io::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCapabilities {
    pub cdp: bool,
    pub stealth: bool,
    pub persistent_context: bool,
    pub auto_update: bool,
    pub custom_core: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreUpdateStatus {
    Unsupported,
    Current { version: String },
    Available { version: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreUpdateReceipt {
    pub version: String,
    pub cleaned_old: bool,
}

pub trait BrowserEngineProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn capabilities(&self) -> EngineCapabilities;
    fn check_update(&self) -> Result<CoreUpdateStatus, EngineError>;
    fn apply_update(&self, clean_old: bool) -> Result<CoreUpdateReceipt, EngineError>;
}

pub(super) const BUILTINS: [(&str, &str, OptionsFamily); 4] = [
    ("playwright", "Playwright", OptionsFamily::Chromium),
    ("patchright", "Patchright", OptionsFamily::Chromium),
    ("camoufox", "Camoufox", OptionsFamily::Camoufox),
    ("cloak", "Cloak", OptionsFamily::Cloak),
];

struct Builtin(&'static str, &'static str, OptionsFamily);

impl BrowserEngineProvider for Builtin {
    fn id(&self) -> &'static str {
        self.0
    }
    fn display_name(&self) -> &'static str {
        self.1
    }
    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            cdp: self.2 != OptionsFamily::Camoufox,
            stealth: self.0 != "playwright",
            persistent_context: true,
            ..EngineCapabilities::default()
        }
    }
    fn check_update(&self) -> Result<CoreUpdateStatus, EngineError> {
        Ok(CoreUpdateStatus::Unsupported)
    }
    fn apply_update(&self, _: bool) -> Result<CoreUpdateReceipt, EngineError> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Provider updates are not managed by this adapter",
        ))
    }
}

pub struct ProviderRegistry(BTreeMap<&'static str, Arc<dyn BrowserEngineProvider>>);

impl Default for ProviderRegistry {
    fn default() -> Self {
        let mut registry = Self(BTreeMap::new());
        for (id, name, family) in BUILTINS {
            registry.register(Arc::new(Builtin(id, name, family)));
        }
        registry
    }
}

impl ProviderRegistry {
    pub fn register(
        &mut self,
        provider: Arc<dyn BrowserEngineProvider>,
    ) -> Option<Arc<dyn BrowserEngineProvider>> {
        self.0.insert(provider.id(), provider)
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn BrowserEngineProvider>> {
        self.0.get(id).cloned()
    }

    pub fn list(&self) -> Vec<Arc<dyn BrowserEngineProvider>> {
        self.0.values().cloned().collect()
    }

    /// Replace an existing engine slot with a registered provider.
    pub fn swap(
        &mut self,
        slot: &str,
        replacement: &str,
    ) -> Option<Arc<dyn BrowserEngineProvider>> {
        let provider = self.get(replacement)?;
        Some(std::mem::replace(self.0.get_mut(slot)?, provider))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NumeraCore;
    impl BrowserEngineProvider for NumeraCore {
        fn id(&self) -> &'static str {
            "numera-core"
        }
        fn display_name(&self) -> &'static str {
            "Numera Core"
        }
        fn capabilities(&self) -> EngineCapabilities {
            EngineCapabilities {
                custom_core: true,
                ..EngineCapabilities::default()
            }
        }
        fn check_update(&self) -> Result<CoreUpdateStatus, EngineError> {
            Ok(CoreUpdateStatus::Available {
                version: "2".into(),
            })
        }
        fn apply_update(&self, clean_old: bool) -> Result<CoreUpdateReceipt, EngineError> {
            Ok(CoreUpdateReceipt {
                version: "2".into(),
                cleaned_old: clean_old,
            })
        }
    }

    #[test]
    fn custom_core_can_replace_every_builtin_slot() -> io::Result<()> {
        let mut registry = ProviderRegistry::default();
        assert!(registry.get("numera-core").is_none());
        assert!(registry.register(Arc::new(NumeraCore)).is_none());
        for (id, _, _) in BUILTINS {
            assert_eq!(
                registry.swap(id, "numera-core").map(|old| old.id()),
                Some(id)
            );
            let provider = registry
                .get(id)
                .ok_or_else(|| io::Error::other("missing provider"))?;
            assert_eq!(provider.id(), "numera-core");
            assert!(provider.capabilities().custom_core);
            assert!(provider.apply_update(true)?.cleaned_old);
        }
        assert!(registry.swap("absent", "numera-core").is_none());
        assert!(registry.swap("cloak", "absent").is_none());
        Ok(())
    }
}
