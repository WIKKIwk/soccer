use super::{EventSchemaVersion, EventType};
use std::fmt;

/// The identity of one event payload schema.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventSchemaKey {
    event_type: EventType,
    version: EventSchemaVersion,
}

impl EventSchemaKey {
    pub fn new(event_type: EventType, version: EventSchemaVersion) -> Self {
        Self {
            event_type,
            version,
        }
    }

    pub fn event_type(&self) -> &EventType {
        &self.event_type
    }

    pub const fn version(&self) -> EventSchemaVersion {
        self.version
    }
}

/// Read compatibility information for one canonical event type.
///
/// The current version is the shape that a reducer consumes. Older versions
/// may remain readable when an explicit upcaster is available. The registry
/// only records that compatibility boundary; it never guesses how a payload
/// should be transformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSchemaDefinition {
    event_type: EventType,
    current_version: EventSchemaVersion,
    readable_versions: Vec<EventSchemaVersion>,
}

impl EventSchemaDefinition {
    pub fn current(event_type: EventType, current_version: EventSchemaVersion) -> Self {
        Self {
            event_type,
            current_version,
            readable_versions: vec![current_version],
        }
    }

    pub fn try_new<I>(
        event_type: EventType,
        current_version: EventSchemaVersion,
        readable_versions: I,
    ) -> Result<Self, EventSchemaDefinitionError>
    where
        I: IntoIterator<Item = EventSchemaVersion>,
    {
        let mut readable_versions: Vec<_> = readable_versions.into_iter().collect();
        if readable_versions.is_empty() {
            return Err(EventSchemaDefinitionError::NoReadableVersions);
        }

        readable_versions.sort_unstable();
        if let Some(duplicate) = readable_versions
            .windows(2)
            .find_map(|versions| (versions[0] == versions[1]).then_some(versions[0]))
        {
            return Err(EventSchemaDefinitionError::DuplicateReadableVersion(
                duplicate,
            ));
        }

        if !readable_versions.contains(&current_version) {
            return Err(EventSchemaDefinitionError::CurrentVersionNotReadable { current_version });
        }

        Ok(Self {
            event_type,
            current_version,
            readable_versions,
        })
    }

    pub fn event_type(&self) -> &EventType {
        &self.event_type
    }

    pub const fn current_version(&self) -> EventSchemaVersion {
        self.current_version
    }

    pub fn readable_versions(&self) -> &[EventSchemaVersion] {
        &self.readable_versions
    }

    pub fn supports(&self, version: EventSchemaVersion) -> bool {
        self.readable_versions.contains(&version)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSchemaDefinitionError {
    NoReadableVersions,
    CurrentVersionNotReadable { current_version: EventSchemaVersion },
    DuplicateReadableVersion(EventSchemaVersion),
}

impl fmt::Display for EventSchemaDefinitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoReadableVersions => {
                formatter.write_str("event schema has no readable versions")
            }
            Self::CurrentVersionNotReadable { current_version } => write!(
                formatter,
                "current event schema version {} is not readable",
                current_version.value()
            ),
            Self::DuplicateReadableVersion(version) => write!(
                formatter,
                "event schema version {} is listed more than once",
                version.value()
            ),
        }
    }
}

impl std::error::Error for EventSchemaDefinitionError {}

/// Result of resolving a persisted event schema against the current reducer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventSchemaResolution {
    Current(EventSchemaKey),
    RequiresUpcast {
        from: EventSchemaKey,
        to: EventSchemaKey,
    },
}

/// Registry of event types and the payload schema versions they can read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventSchemaRegistry {
    definitions: Vec<EventSchemaDefinition>,
}

impl EventSchemaRegistry {
    pub fn register(
        &mut self,
        definition: EventSchemaDefinition,
    ) -> Result<(), EventSchemaRegistryError> {
        if self
            .definitions
            .iter()
            .any(|existing| existing.event_type() == definition.event_type())
        {
            return Err(EventSchemaRegistryError::DuplicateEventType(
                definition.event_type().clone(),
            ));
        }

        self.definitions.push(definition);
        Ok(())
    }

    pub fn resolve(
        &self,
        event_type: &EventType,
        version: EventSchemaVersion,
    ) -> Result<EventSchemaResolution, EventSchemaLookupError> {
        let definition = self
            .definitions
            .iter()
            .find(|definition| definition.event_type() == event_type)
            .ok_or_else(|| EventSchemaLookupError::UnknownEventType(event_type.clone()))?;

        if !definition.supports(version) {
            return Err(EventSchemaLookupError::UnsupportedVersion {
                event_type: event_type.clone(),
                version,
            });
        }

        let from = EventSchemaKey::new(event_type.clone(), version);
        if version == definition.current_version() {
            Ok(EventSchemaResolution::Current(from))
        } else {
            Ok(EventSchemaResolution::RequiresUpcast {
                from,
                to: EventSchemaKey::new(event_type.clone(), definition.current_version()),
            })
        }
    }

    pub fn definitions(&self) -> &[EventSchemaDefinition] {
        &self.definitions
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventSchemaRegistryError {
    DuplicateEventType(EventType),
}

impl fmt::Display for EventSchemaRegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateEventType(event_type) => write!(
                formatter,
                "event schema is already registered for type {}",
                event_type.as_str()
            ),
        }
    }
}

impl std::error::Error for EventSchemaRegistryError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventSchemaLookupError {
    UnknownEventType(EventType),
    UnsupportedVersion {
        event_type: EventType,
        version: EventSchemaVersion,
    },
}

impl fmt::Display for EventSchemaLookupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEventType(event_type) => {
                write!(
                    formatter,
                    "event schema type {} is not registered",
                    event_type.as_str()
                )
            }
            Self::UnsupportedVersion {
                event_type,
                version,
            } => write!(
                formatter,
                "event schema type {} does not support version {}",
                event_type.as_str(),
                version.value()
            ),
        }
    }
}

impl std::error::Error for EventSchemaLookupError {}

#[cfg(test)]
mod tests {
    use super::{
        EventSchemaDefinition, EventSchemaDefinitionError, EventSchemaKey, EventSchemaLookupError,
        EventSchemaRegistry, EventSchemaRegistryError, EventSchemaResolution,
    };
    use crate::{EventSchemaVersion, EventType};

    fn make_event_type(value: &str) -> EventType {
        EventType::try_new(value).expect("test event type should be non-blank")
    }

    fn version(value: u16) -> EventSchemaVersion {
        EventSchemaVersion::try_new(value).expect("test schema version should be positive")
    }

    #[test]
    fn current_schema_resolves_without_an_upcast() {
        let event_type = make_event_type("GoalRecorded");
        let mut registry = EventSchemaRegistry::default();
        registry
            .register(EventSchemaDefinition::current(
                event_type.clone(),
                version(1),
            ))
            .unwrap();

        assert_eq!(
            registry.resolve(&event_type, version(1)).unwrap(),
            EventSchemaResolution::Current(EventSchemaKey::new(event_type, version(1)))
        );
    }

    #[test]
    fn readable_historical_schema_requires_an_explicit_upcast() {
        let event_type = make_event_type("GoalRecorded");
        let mut registry = EventSchemaRegistry::default();
        registry
            .register(
                EventSchemaDefinition::try_new(
                    event_type.clone(),
                    version(2),
                    [version(1), version(2)],
                )
                .unwrap(),
            )
            .unwrap();

        assert_eq!(
            registry.resolve(&event_type, version(1)).unwrap(),
            EventSchemaResolution::RequiresUpcast {
                from: EventSchemaKey::new(event_type.clone(), version(1)),
                to: EventSchemaKey::new(event_type, version(2)),
            }
        );
    }

    #[test]
    fn unknown_type_and_version_are_rejected_without_guessing() {
        let event_type = make_event_type("GoalRecorded");
        let unknown_type = make_event_type("UnknownEvent");
        let mut registry = EventSchemaRegistry::default();
        registry
            .register(EventSchemaDefinition::current(
                event_type.clone(),
                version(1),
            ))
            .unwrap();

        assert_eq!(
            registry.resolve(&unknown_type, version(1)).unwrap_err(),
            EventSchemaLookupError::UnknownEventType(unknown_type)
        );
        assert_eq!(
            registry.resolve(&event_type, version(2)).unwrap_err(),
            EventSchemaLookupError::UnsupportedVersion {
                event_type,
                version: version(2),
            }
        );
    }

    #[test]
    fn schema_definition_requires_one_current_and_unique_readable_versions() {
        let event_type = make_event_type("GoalRecorded");

        assert_eq!(
            EventSchemaDefinition::try_new(event_type.clone(), version(1), []).unwrap_err(),
            EventSchemaDefinitionError::NoReadableVersions
        );
        assert_eq!(
            EventSchemaDefinition::try_new(event_type.clone(), version(2), [version(1)])
                .unwrap_err(),
            EventSchemaDefinitionError::CurrentVersionNotReadable {
                current_version: version(2),
            }
        );
        assert_eq!(
            EventSchemaDefinition::try_new(event_type, version(1), [version(1), version(1)])
                .unwrap_err(),
            EventSchemaDefinitionError::DuplicateReadableVersion(version(1))
        );
    }

    #[test]
    fn registry_rejects_two_definitions_for_one_event_type() {
        let event_type = make_event_type("GoalRecorded");
        let mut registry = EventSchemaRegistry::default();
        registry
            .register(EventSchemaDefinition::current(
                event_type.clone(),
                version(1),
            ))
            .unwrap();

        assert_eq!(
            registry
                .register(EventSchemaDefinition::current(
                    event_type.clone(),
                    version(2)
                ))
                .unwrap_err(),
            EventSchemaRegistryError::DuplicateEventType(event_type)
        );
    }
}
