import SwiftUI

/// A solid chip floating over the picture: 28 high, 12-point medium words, an optional mark before them and an
/// optional time after them in tabular digits, which changes every second, so it takes no transition. The panel's
/// picture takes the small one, 26 high with 11.5-point words. It is opaque and dark in either appearance, as the
/// helper's screen it floats over is framed in black, with a fine edge and a faint shadow outside it.
struct PictureChip<Content: View>: View {
  /// What leads the words.
  enum Lead: Equatable, Sendable {
    /// Nothing.
    case none
    /// A status dot in a tone, such as the active blue of the newest action. It never breathes: one dot breathes in
    /// a window, the status's.
    case dot(StatusTone)
    /// A spinner, which waits before it shows.
    case spinner
    /// A symbol, such as pause.
    case symbol(SymbolName)
  }

  /// How large the chip draws.
  enum Size: Equatable, Sendable {
    /// The viewer's chip: 28 high, 12-point words, 12 points in from each end, 8 between its parts.
    case regular
    /// The panel caption's chip: 26 high, 11.5-point words, 10 points in from each end, 7 between its parts.
    case small

    /// The chip's height.
    var height: CGFloat {
      switch self {
      case .regular: ControlHeight.s
      case .small: ControlHeight.xsPlus
      }
    }

    /// How far the words sit in from each end.
    var inset: CGFloat {
      switch self {
      case .regular: Spacing.m
      case .small: Spacing.sPlus
      }
    }

    /// Between the mark, the words and the time.
    var gap: CGFloat {
      switch self {
      case .regular: Spacing.s
      case .small: Layout.smallGap
      }
    }

    /// The words' font, at a weight.
    ///
    /// - Parameter weight: The weight, or nil for the system's regular.
    /// - Returns: The callout's font, or the small chip's 11.5 points.
    func font(weight: Font.Weight? = nil) -> Font {
      switch self {
      case .regular: Typography.font(.callout, weight: weight)
      case .small: Typography.font(size: Layout.smallText, weight: weight)
      }
    }
  }

  /// What leads the words.
  private let lead: Lead
  /// A time after the words, such as "just now", or nil for none.
  private let time: String?
  /// How large the chip draws.
  private let size: Size
  /// The words, and anything beside them, such as keycaps.
  private let content: Content

  /// Creates a chip.
  ///
  /// - Parameters:
  ///   - lead: What leads the words; nothing by default.
  ///   - time: A time after the words, or nil for none.
  ///   - size: How large the chip draws; the viewer's by default.
  ///   - content: The words, and anything beside them.
  init(lead: Lead = .none, time: String? = nil, size: Size = .regular, @ViewBuilder content: () -> Content) {
    self.lead = lead
    self.time = time
    self.size = size
    self.content = content()
  }

  /// The mark, the words and the time, on a solid capsule.
  var body: some View {
    HStack(spacing: size.gap) {
      leadMark
      content
      if let time {
        Text(time)
          .font(
            size.font()
              .monospacedDigit()
          )
          .foregroundStyle(Color.textSecondary)
      }
    }
    .font(size.font(weight: .medium))
    .foregroundStyle(Color.textPrimary)
    .lineLimit(1)
    .padding(.horizontal, size.inset)
    .frame(height: size.height)
    .background {
      shape
        .fill(Color.fillOverPicture)
    }
    // A capsule's shape drawn as a rounded rectangle, as `CapsuleButtonStyle` draws its edge: a stroked `Capsule`
    // leaves a hairline outside each end where AppKit draws the view into an image.
    .overlay(
      shape
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .background { ChipShadow(shape: shape) }
    .environment(\.colorScheme, .dark)
    .accessibilityElement(children: .combine)
  }

  /// The dot, the spinner, the symbol, or nothing.
  @ViewBuilder private var leadMark: some View {
    switch lead {
    case .none:
      EmptyView()
    case .dot(let tone):
      StatusDot(mark: .dot, tone: tone, size: .small)
    case .spinner:
      Spinner()
        .appearsAfterDelay()
    case .symbol(let name):
      Image(systemName: name.rawValue)
        .font(Typography.symbol(.row, weight: .medium))
        .accessibilityHidden(true)
    }
  }

  /// The chip's outline, a capsule: its corners half its height.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: size.height / 2, style: .circular)
  }
}

/// The chip's shadow, drawn only outside its outline, as CSS draws `box-shadow`: a blurred shape cut away
/// under the chip, so nothing darkens the chip or the words on it.
private struct ChipShadow: View {
  /// The chip's outline.
  let shape: RoundedRectangle

  var body: some View {
    shape
      .fill(Color.dropShadowFaint)
      .blur(radius: Layout.shadowRadius)
      .offset(y: Layout.shadowDrop)
      .mask {
        Rectangle()
          .padding(-Layout.shadowRoom)
          .overlay {
            shape
              .blendMode(.destinationOut)
          }
          .compositingGroup()
      }
      .allowsHitTesting(false)
  }
}

/// The chip's own measures for its floating shadow.
private enum Layout {
  /// 11.5: the small chip's words, between the scale's caption and callout.
  static let smallText: CGFloat = 11.5
  /// 7: between the small chip's parts.
  static let smallGap: CGFloat = 7
  /// 7: the shadow's blur, half the equivalent 14-point CSS blur.
  static let shadowRadius: CGFloat = 7
  /// 4: how far the shadow falls.
  static let shadowDrop: CGFloat = 4
  /// 24: how far past the chip its shadow may spread, more than its blur and drop together.
  static let shadowRoom = Spacing.xxl
}

#if DEBUG
  #Preview("Samples") {
    PictureChipSamples(.chips)
  }
#endif
