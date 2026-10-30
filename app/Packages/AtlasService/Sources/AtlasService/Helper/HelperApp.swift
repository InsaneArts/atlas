import Foundation

/// An app open in the helper, as the viewer service's `apps.json` lists it: front to back, with its windows front to
/// back. Every field may be missing, so each has a safe default.
public struct HelperApp: Decodable, Equatable, Sendable, Identifiable {
  /// The app's process id in the helper; 0 when the service gives none.
  public var pid: Int
  /// The app's name; empty when the service gives none.
  public var name: String
  /// The path of the app's bundle, such as `/Applications/Opie.app`, when known.
  var bundle: String?
  /// The app's bundle identifier, such as `com.apple.TextEdit`, when known.
  public var bundleId: String?
  /// Whether it is the helper's frontmost app.
  public var front: Bool
  /// Whether the app is hidden.
  public var hidden: Bool
  /// When the app was launched, when known.
  public var launchedAt: Date?
  /// Which agent's session opened it, and how; nil for an app that was already open.
  public var origin: HelperAppOrigin?
  /// The app's icon as PNG data, given only for apps installed where apps go; nil when there is none or it does not
  /// read.
  public var icon: Data?
  /// The app's windows, front to back.
  public var windows: [HelperWindow]

  /// The app's identity in a list: its process id.
  public var id: Int { pid }

  /// Creates an app.
  ///
  /// - Parameters:
  ///   - pid: The app's process id in the helper.
  ///   - name: The app's name.
  ///   - bundle: The path of the app's bundle.
  ///   - bundleId: The app's bundle identifier.
  ///   - front: Whether it is the frontmost app.
  ///   - hidden: Whether the app is hidden.
  ///   - launchedAt: When the app was launched.
  ///   - origin: Which agent's session opened it, and how.
  ///   - icon: The app's icon as PNG data.
  ///   - windows: The app's windows, front to back.
  public init(
    pid: Int,
    name: String,
    bundle: String? = nil,
    bundleId: String? = nil,
    front: Bool = false,
    hidden: Bool = false,
    launchedAt: Date? = nil,
    origin: HelperAppOrigin? = nil,
    icon: Data? = nil,
    windows: [HelperWindow] = []
  ) {
    self.pid = pid
    self.name = name
    self.bundle = bundle
    self.bundleId = bundleId
    self.front = front
    self.hidden = hidden
    self.launchedAt = launchedAt
    self.origin = origin
    self.icon = icon
    self.windows = windows
  }

  /// The fields of one `apps` row.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case pid, name, bundle, bundleId, front, hidden, launchedAt, origin, icon, windows
  }

  /// Decodes an app, keeping a safe default for anything missing or of the wrong type; an icon that is not base64
  /// reads as none, as does an origin that is not an object.
  ///
  /// - Parameter decoder: The decoder reading one `apps` row.
  /// - Throws: `DecodingError` only when the row is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    pid = (try? container.decodeIfPresent(Int.self, forKey: .pid)) ?? 0
    name = (try? container.decodeIfPresent(String.self, forKey: .name)) ?? ""
    bundle = try? container.decodeIfPresent(String.self, forKey: .bundle)
    bundleId = container.lenientText(.bundleId)
    front = container.lenientFlag(.front)
    hidden = container.lenientFlag(.hidden)
    launchedAt = container.lenientDate(milliseconds: .launchedAt)
    origin = try? container.decodeIfPresent(HelperAppOrigin.self, forKey: .origin)
    let encoded = try? container.decodeIfPresent(String.self, forKey: .icon)
    icon = encoded.flatMap { Data(base64Encoded: $0) }
      .flatMap { $0.isEmpty ? nil : $0 }
    windows = (try? container.decodeIfPresent([HelperWindow].self, forKey: .windows)) ?? []
  }
}
