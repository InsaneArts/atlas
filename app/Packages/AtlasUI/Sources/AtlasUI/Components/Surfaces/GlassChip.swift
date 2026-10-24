import SwiftUI

/// A chip on glass floating over the picture (v3 V5, V6): 28 high, 12-point medium words, an optional mark before
/// them and an optional time after them in tabular digits, which count rather than jump (v3 R11). Under Reduce
/// Transparency the glass turns opaque.
struct GlassChip<Content: View>: View {
  /// What leads the words.
  enum Lead: Equatable, Sendable {
    /// Nothing.
    case none
    /// A status dot in a tone, such as the active blue of the newest action.
    case dot(StatusTone)
    /// A status dot that breathes while something works, such as the newest action's on the panel's picture;
    /// steady under Reduce Motion.
    case pulsing(StatusTone)
    /// A spinner, which waits before it shows.
    case spinner
    /// A symbol, such as pause.
    case symbol(SymbolName)
  }

  /// What leads the words.
  private let lead: Lead
  /// A time after the words, such as "just now", or nil for none.
  private let time: String?
  /// The words, and anything beside them, such as keycaps.
  private let content: Content

  /// Creates a chip.
  ///
  /// - Parameters:
  ///   - lead: What leads the words; nothing by default.
  ///   - time: A time after the words, or nil for none.
  ///   - content: The words, and anything beside them.
  init(lead: Lead = .none, time: String? = nil, @ViewBuilder content: () -> Content) {
    self.lead = lead
    self.time = time
    self.content = content()
  }

  /// The mark, the words and the time, on a glass capsule.
  var body: some View {
    HStack(spacing: Spacing.s) {
      leadMark
      content
      if let time {
        Text(time)
          .font(Typography.digits(.callout))
          .foregroundStyle(Color.textSecondary)
          .contentTransition(.numericText())
      }
    }
    .font(Typography.font(.callout, weight: .medium))
    .foregroundStyle(Color.textPrimary)
    .lineLimit(1)
    .padding(.horizontal, Spacing.m)
    .frame(height: ControlHeight.s)
    .glass(.regular, in: shape)
    // A capsule's shape drawn as a rounded rectangle, as `CapsuleButtonStyle` draws its edge: a stroked `Capsule`
    // leaves a hairline outside each end where AppKit draws the view into an image.
    .overlay(
      shape
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .background { ChipShadow(shape: shape) }
    .accessibilityElement(children: .combine)
  }

  /// The dot, the breathing dot, the spinner, the symbol, or nothing.
  @ViewBuilder private var leadMark: some View {
    switch lead {
    case .none:
      EmptyView()
    case .dot(let tone):
      StatusDot(mark: .dot, tone: tone, size: .small)
    case .pulsing(let tone):
      StatusDot(mark: .pulsing, tone: tone, size: .small)
    case .spinner:
      Spinner()
        .appearsAfterDelay()
    case .symbol(let name):
      Image(systemName: name.rawValue)
        .font(Typography.symbol(.row, weight: .medium))
        .accessibilityHidden(true)
    }
  }

  /// The chip's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: Layout.corner, style: .circular)
  }
}

/// The chip's shadow, drawn only outside its outline, as CSS draws v3's `box-shadow`: a blurred shape cut away under
/// the chip, so nothing darkens the glass or the words on it.
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

/// v3's chip (`.gchip`).
private enum Layout {
  /// 14: the capsule's corner, half its height.
  static let corner = ControlHeight.s / 2
  /// 7: the shadow's blur, half v3's 14.
  static let shadowRadius: CGFloat = 7
  /// 4: how far the shadow falls.
  static let shadowDrop: CGFloat = 4
  /// 24: how far past the chip its shadow may spread, more than its blur and drop together.
  static let shadowRoom = Spacing.xxl
}

#if DEBUG
  #Preview("Samples") {
    GlassChipSamples(.chips)
  }
#endif
