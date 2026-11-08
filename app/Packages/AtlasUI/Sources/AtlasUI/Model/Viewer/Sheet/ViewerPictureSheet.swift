import CoreGraphics

/// The one picture sheet: a screenshot, or the screen just before a step with its mark, over the dimmed window. Its
/// title row names what happened, with the app, the time and the session on one quiet line, and Before and After when
/// a picture after exists; its footer steps through that run's pictures and offers Copy, Show in Finder and Done.
public struct ViewerPictureSheet: Equatable, Sendable {
  /// The picture's identity: the screenshot's file name, or the step's row.
  public var id: String
  /// The app's icon, or the mark of what the run held when it was in no app.
  public var icon: ActivityIcon
  /// What happened, such as "Took a screenshot" or "Pressed ⏎ Return".
  public var title: ViewerStepWords
  /// The app, the time and the session, such as "TextEdit · 01:58:41 · Claude Code · atlas".
  public var line: String
  /// The picture shown: as taken once read, its thumbnail before then, nil before either.
  public var picture: DesktopPicture?
  /// The picture's shape, which its frame keeps.
  public var aspect: CGSize
  /// The mark over a step's picture; nil for a screenshot or when nothing was recorded to place one.
  public var mark: ViewerActionMark?
  /// Which of a step's pictures shows; before for a screenshot.
  public var side: ViewerPictureSide
  /// Whether the step has a picture after it, so Before and After show.
  public var hasAfter: Bool
  /// Where it stands among the run's pictures, such as "120 of 120".
  public var position: String
  /// Which run, such as "in this TextEdit run", which a narrow sheet leaves out.
  public var run: String
  /// Whether ‹ goes anywhere.
  public var canGoBack: Bool
  /// Whether › goes anywhere.
  public var canGoForward: Bool
  /// Whether Show in Finder can find the picture's file; Copy reads the picture whatever it finds.
  public var canShowInFinder: Bool

  /// Creates the sheet's state.
  ///
  /// - Parameters:
  ///   - id: The picture's identity.
  ///   - icon: The app's icon.
  ///   - title: What happened.
  ///   - line: The app, the time and the session.
  ///   - picture: The picture shown.
  ///   - aspect: The picture's shape.
  ///   - mark: The mark over a step's picture; none by default.
  ///   - side: Which of a step's pictures shows; before by default.
  ///   - hasAfter: Whether there is a picture after; there is not by default.
  ///   - position: Where it stands among the run's pictures.
  ///   - run: Which run.
  ///   - canGoBack: Whether ‹ goes anywhere.
  ///   - canGoForward: Whether › goes anywhere.
  ///   - canShowInFinder: Whether Show in Finder can find the file.
  public init(
    id: String,
    icon: ActivityIcon,
    title: ViewerStepWords,
    line: String,
    picture: DesktopPicture?,
    aspect: CGSize,
    mark: ViewerActionMark? = nil,
    side: ViewerPictureSide = .before,
    hasAfter: Bool = false,
    position: String,
    run: String,
    canGoBack: Bool,
    canGoForward: Bool,
    canShowInFinder: Bool
  ) {
    self.id = id
    self.icon = icon
    self.title = title
    self.line = line
    self.picture = picture
    self.aspect = aspect
    self.mark = mark
    self.side = side
    self.hasAfter = hasAfter
    self.position = position
    self.run = run
    self.canGoBack = canGoBack
    self.canGoForward = canGoForward
    self.canShowInFinder = canShowInFinder
  }

  /// Which side shows as drawn: before while there is no picture after.
  public var shownSide: ViewerPictureSide {
    hasAfter ? side : .before
  }
}
