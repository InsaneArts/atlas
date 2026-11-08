import SwiftUI

/// The measures of pictures of agent actions: the marks drawn over them, a step's small picture, and a task's
/// replay.
extension ViewerLayout {
  /// 36: a mark's ring over a large picture.
  static let markRing: CGFloat = 36
  /// 14: a mark's ring over a small picture.
  static let markRingCompact: CGFloat = 14
  /// 6: the corner of the ring around an element, over a large picture.
  static let markOutlineRadius = Radius.extraSmall
  /// 3: the same over a small picture.
  static let markOutlineRadiusCompact: CGFloat = 3
  /// 0.65: how far right an element's ring ends before why it did not work goes under it instead of beside it.
  static let markNearRight: CGFloat = 0.65
  /// 8: the dot where the pointer landed, over a large picture.
  static let markDot: CGFloat = 8
  /// 0.8: how far either of a double click's dots stands from the spot, in dots.
  static let markDotSpread: CGFloat = 0.8
  /// 5: the same over a small picture.
  static let markDotCompact: CGFloat = 5
  /// 4: the soft glow around a ring or a dot over a large picture.
  static let markGlow: CGFloat = 4
  /// 2: the same over a small picture.
  static let markGlowCompact: CGFloat = 2
  /// 0.28: how strong the glow's colour is.
  static let markGlowOpacity = 0.28
  /// 12: a scroll pill's width over a large picture.
  static let markPill: CGFloat = 12
  /// 6: the same over a small picture.
  static let markPillCompact: CGFloat = 6
  /// 2.6: how much longer a scroll pill is than wide.
  static let markPillLength: CGFloat = 2.6
  /// 5 and 4: a dashed ring's or path's dash and gap.
  static let markDash: [CGFloat] = [5, 4]
  /// 2.5: how much wider a drag's halo is than its path.
  static let markHaloWidth: CGFloat = 2.5
  /// 8: between a mark and its bubble, keys or tag.
  static let markGap = Spacing.s
  /// 0.52: the widest a bubble stands, as a share of the picture's width.
  static let markBubbleShare: CGFloat = 0.52
  /// 3: the most lines of typed text a bubble shows.
  static let markBubbleLines = 3
  /// 0.25: how near the top a spot is before its bubble hangs under it instead.
  static let markNearTop: CGFloat = 0.25
  /// 0.2: how near either side a spot is before its bubble stands to that side.
  static let markNearSide: CGFloat = 0.2

  /// 76: a step's small picture's width.
  static let stepPictureWidth: CGFloat = 76
  /// 16:10: a step's small picture's shape, the same for every step so the rows stand even.
  static let stepPictureShape = ShotShape.standard
  /// 2.6: how far a step's small picture zooms in around its spot.
  static let stepPictureZoom: CGFloat = 2.6
  /// 3: how far each picture behind a folded step's stands up and to the side.
  static let stepPictureStack: CGFloat = 3
  /// 22: a replayed step's disc.
  static let stepDisc: CGFloat = 22
  /// 6: a replayed step's small picture's corner.
  static let stepPictureRadius = Radius.extraSmall
  /// 15: a folded step's count's height.
  static let stepCount: CGFloat = 15

  /// 24: the agent's icon leading a replay's title.
  static let replayAgentIcon = IconSize.m
  /// 820: a replay's widest.
  static let replayWidth: CGFloat = 820
  /// 44: the least height of the line under a replay's picture.
  static let replaySay = ControlHeight.xl
  /// 30: a replay's round buttons.
  static let replayButton = ControlHeight.sPlus
  /// 72: the least width of where a replay stands, such as "Step 1 of 5".
  static let replayPosition: CGFloat = 72
  /// 20: the app's name over a run of a replay's frames in one app, with the rule over it.
  static let replayGroupLabel: CGFloat = 20
  /// 4: the strip's inset around its frames, room for the ring around the frame shown.
  static let replayStripInset = Spacing.xs
  /// 6: the most frames the strip shows side by side before it scrolls.
  static let replayStripMost = 6

  /// The window's margins either side of a replay's card, and the card's own insets.
  private static let replaySides = (Spacing.xxxl + Spacing.mPlus) * 2
  /// The height a replay's card takes beside its picture and strip: the window's margins and the card's insets, its
  /// head, what the step did, its controls, and the four gaps between them.
  private static let replayAround =
    replaySides + ControlHeight.xl + replaySay + replayButton + (Spacing.m + Spacing.m) * 2

  /// How large a replay's picture is drawn: at its own shape, as wide as the replay, and within a window `window`
  /// large less its words, its strip, the strip's app names when it has them, and its bar.
  ///
  /// - Parameters:
  ///   - aspect: The picture's shape.
  ///   - window: The window's size.
  ///   - labeled: Whether the strip names the apps over its frames; it does not by default.
  /// - Returns: The picture's size; zero when nothing fits.
  static func replayPictureSize(aspect: CGSize, in window: CGSize, labeled: Bool = false) -> CGSize {
    let width =
      min(replayWidth, window.width - replaySides + (Spacing.mPlus + Spacing.mPlus))
      - (Spacing.mPlus + Spacing.mPlus)
    let strip =
      width / CGFloat(replayStripMost) / ShotShape.standard + Spacing.l
      + (labeled ? replayGroupLabel + Spacing.xs : .zero)
    let room = CGSize(width: width, height: window.height - replayAround - strip)
    return fitted(aspect, in: room)
  }

  /// How wide each frame of a replay's strip stands, so ``replayStripMost`` fit across its picture.
  ///
  /// - Parameter width: The replay's picture's width.
  /// - Returns: A frame's width.
  static func replayFrameWidth(picture width: CGFloat) -> CGFloat {
    let insets = replayStripInset + replayStripInset
    let gaps = Spacing.s * CGFloat(replayStripMost - 1)
    return (width - insets - gaps) / CGFloat(replayStripMost)
  }

  /// A shape scaled to fit a room.
  ///
  /// - Parameters:
  ///   - aspect: The shape.
  ///   - room: The room.
  /// - Returns: The size; zero when nothing fits.
  static func fitted(_ aspect: CGSize, in room: CGSize) -> CGSize {
    guard aspect.width > 0, aspect.height > 0, room.width > 0, room.height > 0 else { return .zero }
    let scale = min(room.width / aspect.width, room.height / aspect.height)
    return CGSize(width: aspect.width * scale, height: aspect.height * scale)
  }
}
