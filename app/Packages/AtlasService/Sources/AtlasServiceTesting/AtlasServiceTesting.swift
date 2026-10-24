/// AtlasServiceTesting — fakes for AtlasService's seam, imported by dependents' test targets. Nothing here ships.
///
/// `ScriptedService` is a `ViewerService` that answers from fixed replies, holds or fails a request until told
/// (`HeldShot` is a held screenshot request) and records what it was asked. `status(_:)` decodes a status reply from
/// its JSON fields.
