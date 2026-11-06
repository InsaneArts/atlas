/// The button that mutes the helper or lets it play, as the panel and the main window draw it beside their pause: it
/// shows the person's saved choice, and when the helper cannot make that choice hold, the attention tint with why as
/// its tooltip.
public struct HelperSoundButton: Equatable, Sendable {
  /// Whether the person chose to keep apps in the helper silent.
  public var isMuted: Bool
  /// Why the helper's sound is not as chosen, with its fix; nil when it is.
  public var problem: String?

  /// Creates the button.
  ///
  /// - Parameters:
  ///   - isMuted: Whether the person chose to keep apps in the helper silent; yes by default, as Atlas mutes unless
  ///     told otherwise.
  ///   - problem: Why the helper's sound is not as chosen; none by default.
  public init(isMuted: Bool = true, problem: String? = nil) {
    self.isMuted = isMuted
    self.problem = problem
  }

  /// What a press does, which names the button for VoiceOver.
  public var label: String { isMuted ? "Unmute the helper" : "Mute the helper" }

  /// Its tooltip: why the sound is not as chosen, else what a press does.
  public var help: String { problem ?? label }

  /// Whether it draws in the attention tint: the sound is not as chosen.
  public var needsAttention: Bool { problem != nil }

  /// Its symbol: a speaker struck through while muted, sounding while not.
  public var symbol: SymbolName { isMuted ? .speakerMuted : .speakerOn }
}
