/// The screenshot an Activity line took: its file name, which Open shows large, and its thumbnail once read.
public struct ViewerActivityShot: Equatable, Sendable {
  /// The screenshot's file name.
  public var name: String
  /// Its thumbnail; nil until it is read.
  public var thumbnail: DesktopPicture?
  /// What VoiceOver reads for it, such as "Screenshot taken at 10:31"; empty reads "Screenshot".
  public var label: String

  /// Creates a line's screenshot.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - thumbnail: Its thumbnail; nil until it is read.
  ///   - label: What VoiceOver reads for it; empty by default, which reads "Screenshot".
  public init(name: String, thumbnail: DesktopPicture? = nil, label: String = "") {
    self.name = name
    self.thumbnail = thumbnail
    self.label = label
  }
}
