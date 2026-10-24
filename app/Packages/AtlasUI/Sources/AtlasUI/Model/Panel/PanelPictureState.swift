/// The panel's picture: the helper's screen in the look of its state, with the newest action over its corner while an
/// agent works and the pause button on it; or, with no screen to show, the empty stage.
public struct PanelPictureState: Equatable, Sendable {
  /// How the picture looks.
  public enum Look: Equatable, Sendable {
    /// The screen as it is.
    case live
    /// The screen inside the blue rim of control: the person holds it in Atlas.
    case control
    /// The screen greyed and darkened under a small orange badge: the helper needs the person.
    case needsYou
    /// The screen greyed and darkened with no badge: the person paused the picture.
    case paused
    /// No screen: Atlas's mark over one quiet line saying why, such as "The helper is logged out".
    ///
    /// - Parameter message: The line.
    case empty(message: String)
    /// No screen yet, before the first answer: Atlas's mark breathing on the empty stage.
    case connecting
  }

  /// How the picture looks.
  public var look: Look
  /// The screen; nil shows the empty stage until the first picture arrives.
  public var picture: DesktopPicture?
  /// The newest action while an agent works and the picture is live; nil shows none.
  public var caption: PanelCaption?
  /// The pause button on the picture: whether the picture is paused, and its words.
  public var pause: PanelPause

  /// Creates the picture's state.
  ///
  /// - Parameters:
  ///   - look: How the picture looks.
  ///   - picture: The screen; nil shows the empty stage.
  ///   - caption: The newest action while an agent works; nil shows none.
  ///   - pause: The pause button's state and words.
  public init(look: Look, picture: DesktopPicture? = nil, caption: PanelCaption? = nil, pause: PanelPause) {
    self.look = look
    self.picture = picture
    self.caption = caption
    self.pause = pause
  }

  /// Whether the frame shows the screen, with its pause button: every look but the empty stage's.
  public var showsScreen: Bool {
    switch look {
    case .live, .control, .needsYou, .paused: true
    case .empty, .connecting: false
    }
  }

  /// Whether the blue rim of control lies round the picture: only while the person holds control.
  public var showsRim: Bool { look == .control }

  /// Whether the screen is greyed and darkened: while the helper needs the person, and while the picture is paused.
  public var isDimmed: Bool { look == .needsYou || look == .paused }

  /// Whether the orange badge sits on the picture's corner: only while the helper needs the person.
  public var showsBadge: Bool { look == .needsYou }

  /// The empty stage's line, or nil while the screen shows; empty while connecting, when only the mark shows.
  public var emptyMessage: String? {
    switch look {
    case .empty(let message): message
    case .connecting: ""
    case .live, .control, .needsYou, .paused: nil
    }
  }

  /// Whether the empty stage's mark breathes: only while connecting, before the first answer.
  public var breathes: Bool { look == .connecting }
}
