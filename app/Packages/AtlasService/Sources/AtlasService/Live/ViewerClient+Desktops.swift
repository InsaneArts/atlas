import Foundation

/// Every desktop the service serves, and the one the app views: reading `desktops.json`, viewing another, and a VM's
/// power.
extension ViewerClient {
  /// Reads `desktops.json`, and views from then on the desktop it says the service views.
  ///
  /// - Returns: Every desktop, in the settings' order, every missing field at its safe default.
  /// - Throws: ``ServiceError/notOffered`` when an older service answers 404; else ``ServiceError`` with the service's
  ///   error, or when it cannot be read.
  public func desktops() async throws(ServiceError) -> DesktopList {
    let list = try await listed()
    viewed.view(list.viewed)
    return list
  }

  /// Reads `desktops.json`, the binding left as it is.
  ///
  /// - Returns: Every desktop, in the settings' order.
  /// - Throws: As ``desktops()``.
  func listed() async throws(ServiceError) -> DesktopList {
    let route = base.appendingPathComponent("desktops.json")
    let (data, response) = try await fetched(request(to: route), limit: Self.listLimit)
    return try await Self.decodedReply { () throws(ServiceError) in try Self.desktops(data, response: response) }
  }

  /// A `desktops.json` reply read: only a 200 carries the list.
  ///
  /// - Parameters:
  ///   - data: The reply's body.
  ///   - response: The reply's response.
  /// - Returns: The list.
  /// - Throws: ``ServiceError/notOffered`` for a 404; ``ServiceError`` with the service's error for any other reply
  ///   that is not a 200, or when its body is not an object.
  static func desktops(_ data: Data, response: HTTPURLResponse) throws(ServiceError) -> DesktopList {
    guard response.statusCode != 404 else { throw .notOffered }
    return try listed(DesktopList.self, data, response: response, fallback: "Atlas couldn't read its desktops.")
  }

  /// Posts to `view`: the service views `desktop` from then on, letting go of control on the one viewed before, and
  /// every request names it.
  ///
  /// - Parameter desktop: The desktop's ID.
  /// - Throws: ``ServiceError`` when the service refuses, as for a desktop it does not serve.
  public func view(_ desktop: String) async throws {
    /// The request: the desktop to view.
    struct Body: Encodable {
      /// The desktop's ID.
      let desktop: String
    }
    _ = try await post("view", Body(desktop: desktop))
    viewed.view(desktop)
  }

  /// Posts to `desktops/power`: the person starts or stops the VM `desktop` names, whichever desktop is viewed.
  ///
  /// - Parameters:
  ///   - desktop: The VM's ID.
  ///   - on: Whether it starts.
  /// - Throws: ``ServiceError`` when the service refuses, as for a VM under maintenance.
  public func setPower(_ desktop: String, on: Bool) async throws {
    /// The request: the VM, and whether it starts.
    struct Body: Encodable {
      /// The VM's ID.
      let desktop: String
      /// Whether it starts.
      let on: Bool
    }
    _ = try await post("desktops/power", Body(desktop: desktop, on: on))
  }
}
