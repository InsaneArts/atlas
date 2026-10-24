import AtlasKit

extension Mode {
  /// Its word, such as "Working": the service's own, else the row's.
  var word: String { label ?? kind.word }
}
