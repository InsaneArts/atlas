/// An app open in the helper, as one card of the Apps pane: its icon and name, which session opened it and when,
/// tags saying whether it is in front, hidden, a copy or installed by Atlas, a picture of its front window or where
/// its windows sit, and its windows front to back.
public struct ViewerAppCard: Equatable, Sendable, Identifiable {
  /// The app's process id in the helper.
  public var id: Int
  /// The stable string target used by the card's shared scroll-position binding.
  public var scrollID: String { String(id) }
  /// Its name.
  public var name: String
  /// Its icon, or the tile drawn from its name.
  public var icon: AppIconFace
  /// The session that opened it, while every session's apps show; nil for one already open, or while one session's
  /// own apps show.
  public var origin: ViewerAppOrigin?
  /// When it was opened, or since when it has been open, such as "4 min ago" or "Open since 09:12"; empty beside a
  /// session that says no time.
  public var line: String
  /// Its tags, in order.
  public var tags: [ViewerAppTag]
  /// Whether it is the helper's frontmost app, which raises its card in place.
  public var isFront: Bool
  /// Its front window's picture, or where its windows sit; nil for an app with no window.
  public var picture: ViewerAppPicture?
  /// Its windows, front to back; the card lists the first few and puts the rest in a menu.
  public var windows: [ViewerWindowRow]

  /// Creates an app's card.
  ///
  /// - Parameters:
  ///   - id: The app's process id.
  ///   - name: Its name.
  ///   - icon: Its icon, or the tile drawn from its name; by default the tile.
  ///   - origin: The session that opened it.
  ///   - line: When it was opened, or since when it has been open.
  ///   - tags: Its tags.
  ///   - isFront: Whether it is in front.
  ///   - picture: Its front window's picture, or where its windows sit.
  ///   - windows: Its windows, front to back.
  public init(
    id: Int,
    name: String,
    icon: AppIconFace? = nil,
    origin: ViewerAppOrigin? = nil,
    line: String,
    tags: [ViewerAppTag] = [],
    isFront: Bool = false,
    picture: ViewerAppPicture? = nil,
    windows: [ViewerWindowRow] = []
  ) {
    self.id = id
    self.name = name
    self.icon = icon ?? .monogram(AppMonogram(name: name))
    self.origin = origin
    self.line = line
    self.tags = tags
    self.isFront = isFront
    self.picture = picture
    self.windows = windows
  }

  /// What VoiceOver says of the card's head: its name, who opened it and when, and its tags.
  public var label: String {
    let opened = [origin?.name, line]
      .compactMap { $0 }
      .filter { !$0.isEmpty }
    let words = [name] + opened + tags.map(\.words)
    return words.joined(separator: ", ")
  }
}
