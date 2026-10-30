import AtlasService
import AtlasUI

/// One host session as every surface of the main window names it, looked up once through the status, the sessions'
/// list (live, then recorded) and the activity log (``ViewerPresenter/resolvedSession(_:_:)``): its name, host and
/// state, and what each source says of it.
struct ViewerResolvedSession: Equatable {
  /// The session's key; empty for Terminal.
  var key: String
  /// Its name: as the status lists it, else the list's live or recorded one, else the log's; Terminal, or Another
  /// session once nothing names it.
  var name: String
  /// Its host, as the first source that names it says; none for Terminal, nil once nothing names it.
  var host: SessionHost?
  /// What it is doing: a live session's own state; Ended once the status or the list says it ended; Idle for one any
  /// source still names that is neither, Terminal's included, as it never ends; nil for one nothing names.
  var state: HostSessionState?
  /// The session as the status lists it, live or not.
  var listed: ViewerStatus.HostSession?
  /// The session while it is live, as the status lists it, else as the list names it live.
  var live: ViewerStatus.HostSession?
  /// The session as the list records it.
  var record: HostSessionRecord?

  /// Its icon, by its host; none for a session nothing names.
  var icon: RowIcon? { host?.icon }
}
