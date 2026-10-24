import AtlasKit

extension ViewerStatus.Unobservable {
  /// Why the helper's desktop cannot be watched, as the viewer says it: a silent helper's reason, else what the service
  /// said, with the app's words where the service gave none.
  var word: String {
    switch self {
    case .silent(let reason): reason ?? Words.connecting
    case .said(let words): words ?? Words.helperUnavailable
    }
  }
}
