/// The viewer service, as the app sees it; a protocol so the models are
/// tested with a stand-in.
///
/// ``ViewerClient`` is the live conformer; the app's tests script their own.
package protocol ViewerService: Sendable {
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

  /// Stops agents in the helper, ending an agent's input under way, or lets them go on.
  ///
  /// - Parameter stopped: Whether agents stop.
  /// - Throws: ``ServiceError`` when the service refuses.
  func setStopped(_ stopped: Bool) async throws

  /// Sends input to the helper under a lease.
  ///
  /// - Parameters:
  ///   - lease: The lease the input belongs to.
  ///   - frame: The id of the picture the input's coordinates refer to.
  ///   - actions: The actions, in order.
  /// - Throws: ``ServiceError`` when the input is refused.
  func input(lease: String, frame: String, actions: [InputAction]) async throws

  /// Recovers the helper.
  ///
  /// - Returns: Whether it restarted the helper, rather than starting its input.
  /// - Throws: ``ServiceError`` when recovery did not run.
  func recover() async throws -> Bool
}
