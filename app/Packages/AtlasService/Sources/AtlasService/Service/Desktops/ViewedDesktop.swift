import os

/// Which desktop the app views: the one the service named last, by its ID. Every request names it, so a service that
/// serves more than one desktop answers for that one, and a reply that comes back after the app viewed another is
/// dropped. Shared by every copy of a client.
public final class ViewedDesktop: Sendable {
  /// The viewed desktop's ID; nil until the service names one, as an older service never does.
  private let viewed = OSAllocatedUnfairLock<String?>(initialState: nil)

  /// A binding to no desktop yet: requests name none, as before there was more than one.
  public init() {}

  /// The viewed desktop's ID, or nil while none is named.
  public var id: String? {
    viewed.withLock { $0 }
  }

  /// Views the desktop `id` names from now on.
  ///
  /// - Parameter id: The desktop's ID, or nil for none.
  func view(_ id: String?) {
    viewed.withLock { $0 = id }
  }

  /// Refuses a reply to a request made while `asked` was viewed, once another is. A request made before the service
  /// named any was answered for the one it views, which naming it since does not change.
  ///
  /// - Parameter asked: The desktop viewed when the request was made, or nil when none was named yet.
  /// - Throws: ``ServiceError/otherDesktop`` when another desktop is viewed now.
  func still(_ asked: String?) throws(ServiceError) {
    guard asked == nil || id == asked else { throw .otherDesktop }
  }
}
