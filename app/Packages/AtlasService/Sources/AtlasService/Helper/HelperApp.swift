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
  /// Whether it is the helper's frontmost app.
  public var front: Bool
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
  ///   - front: Whether it is the frontmost app.
  ///   - icon: The app's icon as PNG data.
  ///   - windows: The app's windows, front to back.
  public init(
    pid: Int,
    name: String,
    bundle: String? = nil,
    front: Bool = false,
    icon: Data? = nil,
    windows: [HelperWindow] = []
  ) {
    self.pid = pid
    self.name = name
    self.bundle = bundle
    self.front = front
    self.icon = icon
    self.windows = windows
  }

  /// The fields of one `apps` row.
  enum CodingKeys: String, CodingKey {
    /// Each field by the name the service writes.
    case pid, name, bundle, front, icon, windows
  }

  /// Decodes an app, keeping a safe default for anything missing or of the wrong type; an icon that is not base64
  /// reads as none.
  ///
  /// - Parameter decoder: The decoder reading one `apps` row.
  /// - Throws: `DecodingError` only when the row is not an object.
  public init(from decoder: any Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    pid = (try? container.decodeIfPresent(Int.self, forKey: .pid)) ?? 0
    name = (try? container.decodeIfPresent(String.self, forKey: .name)) ?? ""
    bundle = try? container.decodeIfPresent(String.self, forKey: .bundle)
    front = (try? container.decodeIfPresent(Bool.self, forKey: .front)) == true
    let encoded = try? container.decodeIfPresent(String.self, forKey: .icon)
    icon = encoded.flatMap { Data(base64Encoded: $0) }
      .flatMap { $0.isEmpty ? nil : $0 }
    windows = (try? container.decodeIfPresent([HelperWindow].self, forKey: .windows)) ?? []
  }
}
