import Foundation

/// The viewer service, as the app sees it; a protocol so the models are
/// tested with a stand-in.
///
/// ``ViewerClient`` is the live conformer; the app's tests script their own.
public protocol ViewerService: Sendable {
  /// Reads the helper's status.
  ///
  /// - Returns: The status as the service reports it, every missing field at its safe default.
  /// - Throws: ``ServiceError`` or a transport error when the service cannot be read.
  func status() async throws -> ViewerStatus

  /// Reads a picture of the helper's desktop.
  ///
  /// - Returns: The picture, with what it was taken of.
  /// - Throws: ``ServiceError`` when the capture failed or its display size is unknown.
  func frame() async throws -> Frame

  /// How to pair with the frame service, when the viewer runs one.
  ///
  /// - Returns: The descriptor, or nil when there is none.
  /// - Throws: When the viewer could not be asked.
  func framePipe() async throws -> FramePipeDescriptor?

  /// Asks for mouse and keyboard control over the picture this viewer shows.
  ///
  /// - Parameter frame: The id of the picture the person acts on.
  /// - Returns: The lease that later requests name.
  /// - Throws: ``ServiceError`` when control is not granted.
  func acquire(frame: String) async throws -> String

  /// Keeps a lease alive.
  ///
  /// - Parameters:
  ///   - lease: The lease to renew.
  ///   - frame: The id of the picture now shown.
  /// - Throws: ``ServiceError`` when the lease has ended.
  func renew(lease: String, frame: String) async throws

  /// Gives control back.
  ///
  /// - Parameter lease: The lease to end.
  /// - Throws: ``ServiceError`` when the service refuses.
  func release(lease: String) async throws

  /// Ends whichever viewer's control there is: the emergency stop.
  ///
  /// - Throws: ``ServiceError`` when the service refuses.
  func revoke() async throws

  /// Stops agents in the helper, ending an agent's input under way and its background jobs, or lets them go on.
  ///
  /// - Parameter stopped: Whether agents stop.
  /// - Returns: What the Stop came to: what the service warned of, and the background jobs it could not end.
  /// - Throws: ``ServiceError`` when the service refuses.
  @discardableResult
  func setStopped(_ stopped: Bool) async throws -> StopOutcome

  /// Adds an app to the helper's accounts, by its bundle identifier and name: the person chose it.
  ///
  /// - Parameters:
  ///   - app: The app's bundle identifier.
  ///   - name: The app's name.
  /// - Throws: ``ServiceError`` when the service refuses.
  func addAccount(app: String, name: String) async throws

  /// Takes an account off the helper's list.
  ///
  /// - Parameter account: The account.
  /// - Throws: ``ServiceError`` when the service refuses.
  func removeAccount(_ account: ViewerStatus.Account) async throws

  /// Sets how freely agents act in an account.
  ///
  /// - Parameters:
  ///   - level: The level.
  ///   - account: The account.
  /// - Throws: ``ServiceError`` when the service refuses.
  func setLevel(_ level: AccountLevel, of account: ViewerStatus.Account) async throws

  /// Reads the questions Atlas asks the person now, with what each would send, and what they allowed. Reading takes
  /// down the questions whose time ran out.
  ///
  /// - Returns: The questions, oldest first, and the grants.
  /// - Throws: ``ServiceError/notOffered`` from an older service that asks nothing; else ``ServiceError`` when the
  ///   service cannot be read or refuses.
  func asks() async throws(ServiceError) -> AskList

  /// Answers a question as the person.
  ///
  /// - Parameters:
  ///   - ask: The question's id.
  ///   - answer: The answer.
  /// - Throws: ``ServiceError`` when the service refuses, as for a question that has ended.
  func answer(_ ask: String, with answer: AskAnswer) async throws

  /// Ends what the person allowed for a task in an account.
  ///
  /// - Parameter account: The account.
  /// - Throws: ``ServiceError`` when the service refuses.
  func endAllowed(in account: AccountKey) async throws

  /// Sends input to the helper under a lease.
  ///
  /// - Parameters:
  ///   - lease: The lease the input belongs to.
  ///   - frame: The id of the picture the input's coordinates refer to.
  ///   - actions: The actions, in order.
  /// - Throws: ``ServiceError`` when the input is refused.
  func input(lease: String, frame: String, actions: [InputAction]) async throws

  /// Reads what was done in the helper lately. Reading it records nothing.
  ///
  /// - Returns: The log, newest first, every missing field at its safe default.
  /// - Throws: ``ServiceError`` when the service cannot be read or refuses.
  func activity() async throws(ServiceError) -> ActivityLog

  /// Reads the apps open in the helper with their windows. Reading them records nothing and takes no control.
  ///
  /// - Returns: The apps, front to back, and the sessions that opened them, every missing field at its safe default.
  /// - Throws: ``ServiceError`` when the service cannot be read or cannot reach the helper; a refusal never reads as
  ///   no apps.
  func apps() async throws(ServiceError) -> HelperAppList

  /// Reads the screenshots agents took in the helper, and the folder they are kept in.
  ///
  /// - Returns: The screenshots, newest first, and their folder, every missing field at its safe default.
  /// - Throws: ``ServiceError`` when the service cannot be read or refuses.
  func screenshots() async throws(ServiceError) -> ScreenshotList

  /// Reads one screenshot an agent took.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name, as ``Screenshot/name`` gives it.
  ///   - full: Whether to read the picture as taken (PNG), rather than its thumbnail (JPEG).
  /// - Returns: The picture's bytes.
  /// - Throws: ``ServiceError`` when the service cannot be read, or no screenshot has that name.
  func screenshot(name: String, full: Bool) async throws(ServiceError) -> Data

  /// Reads the host sessions live or ended within the last seven days, and what each made. Reading them records
  /// nothing.
  ///
  /// - Returns: The sessions, newest last seen first, every missing field at its safe default.
  /// - Throws: ``ServiceError/notOffered`` from an older service that offers no list, which callers read as no list;
  ///   else ``ServiceError`` when the service cannot be read or refuses.
  func sessions() async throws(ServiceError) -> HostSessionList

  /// Reads what cleaning up a host session's work would take, or cleans it up as the person: quits what runs from
  /// its copies, rolls back its plans, moves what its tasks installed aside in the helper and removes the copies
  /// Atlas made for it.
  ///
  /// - Parameters:
  ///   - session: The session's key.
  ///   - apply: Whether to clean up now, rather than only read what it would take, which changes nothing.
  /// - Returns: The steps it takes, or took and how far each got, every missing field at its safe default; an apply
  ///   that did not finish comes back too, with its steps, so what was kept or failed can be shown.
  /// - Throws: ``ServiceError/notOffered`` from an older service that offers no cleanup; for an apply that got no
  ///   reply, which may still run, ``ServiceError/isCleanupUnanswered``; else ``ServiceError`` when the service cannot
  ///   be read or refuses, as while a viewer holds control.
  func cleanup(session: String, apply: Bool) async throws(ServiceError) -> CleanupReply
}
