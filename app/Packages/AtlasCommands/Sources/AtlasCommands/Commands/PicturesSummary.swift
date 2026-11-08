import AtlasService
import Foundation

/// What `atlas config --pictures-summary` reports: how Atlas keeps pictures of agent actions, and how many this Mac
/// keeps now and their size. A setting it leaves out is Atlas's default.
public struct PicturesSummary: Equatable, Sendable {
  /// How pictures are kept.
  public var settings: PicturesSettings
  /// How many pictures this Mac keeps.
  public var count: Int
  /// Their size on disk, in bytes.
  public var bytes: Int

  /// Creates a summary.
  ///
  /// - Parameters:
  ///   - settings: How pictures are kept.
  ///   - count: How many this Mac keeps; none by default.
  ///   - bytes: Their size on disk; none by default.
  public init(settings: PicturesSettings, count: Int = 0, bytes: Int = 0) {
    self.settings = settings
    self.count = count
    self.bytes = bytes
  }

  /// The summary the command reported.
  ///
  /// - Parameter outcome: The command's outcome.
  /// - Returns: The summary, or nil when the command did not answer with its settings.
  public static func of(_ outcome: Outcome) -> PicturesSummary? {
    guard outcome.ok,
      let reply = try? JSONSerialization.jsonObject(with: outcome.reply) as? [String: Any],
      let config = reply["config"] as? [String: Any]
    else { return nil }
    let activity = config["activity"] as? [String: Any] ?? [:]
    let pictures = reply["pictures"] as? [String: Any] ?? [:]
    let keepDays = activity["keep_days"] as? Int ?? 0
    let limit = activity["typed_text_limit"] as? Int ?? 0
    return PicturesSummary(
      settings: PicturesSettings(
        isOn: activity["pictures_before_actions"] as? Bool ?? true,
        keepDays: keepDays > 0 ? keepDays : PicturesSettings.defaultKeepDays,
        typedTextLimit: limit > 0 ? limit : nil
      ),
      count: max(0, pictures["count"] as? Int ?? 0),
      bytes: max(0, pictures["bytes"] as? Int ?? 0)
    )
  }
}
