import SwiftUI

/// One fact in an inspector (v3 V3, `.vdetails .kv` and `.chk`), at 12 points with a hairline above it: a quiet name
/// on the left and its value on the right in tabular digits; or, for a check, its mark, its name and, under the name,
/// what it means, orange while it fails.
struct FactRow: View {
  /// The name, such as "Account" or "Mouse and keyboard".
  private let name: String
  /// The value, or for a check the line under its name; empty for none.
  private let value: String
  /// How the check stands, which its mark shows; nil for a plain fact.
  private let mark: ReadinessState?

  /// Creates a fact.
  ///
  /// - Parameters:
  ///   - name: The name.
  ///   - value: The value, or for a check the line under its name; empty for none.
  ///   - mark: How the check stands; nil, by default, for a plain fact.
  init(_ name: String, value: String = "", mark: ReadinessState? = nil) {
    self.name = name
    self.value = value
    self.mark = mark
  }

  /// The fact at 12 points, the hairline above it.
  var body: some View {
    content
      .textStyle(.callout)
      .padding(.vertical, Layout.inset)
      .frame(maxWidth: .infinity, minHeight: ControlHeight.s, alignment: .leading)
      .overlay(alignment: .top) { SeparatorLine() }
      .accessibilityElement(children: .combine)
      .accessibilityValue(mark?.spokenValue ?? "")
  }

  /// A check's mark, name and line, or a fact's name and value.
  @ViewBuilder private var content: some View {
    if let mark {
      HStack(alignment: .top, spacing: Spacing.s) {
        FactMark(state: mark)
          .padding(.top, Layout.markDrop)
        VStack(alignment: .leading, spacing: .zero) {
          Text(name)
            .foregroundStyle(Color.textPrimary)
          if !value.isEmpty {
            Text(value)
              .textStyle(.caption)
              .foregroundStyle(Self.lineColor(mark))
              .fixedSize(horizontal: false, vertical: true)
          }
        }
      }
    } else {
      HStack(alignment: .firstTextBaseline, spacing: Spacing.m) {
        Text(name)
          .foregroundStyle(Color.textSecondary)
        Spacer(minLength: .zero)
        Text(value)
          .font(Typography.digits(.callout, weight: .medium))
          .foregroundStyle(Color.textPrimary)
          .lineLimit(1)
          .truncationMode(.tail)
          .help(value)
      }
    }
  }

  /// The colour of the line under a check's name: orange while it fails, secondary otherwise (v3 `.chk small`).
  ///
  /// - Parameter mark: How the check stands.
  /// - Returns: The colour.
  nonisolated static func lineColor(_ mark: ReadinessState) -> Color {
    mark == .notReady ? .textAttention : .textSecondary
  }
}

/// The row's own measures (v3 `.vdetails .kv`).
private enum Layout {
  /// 6: above and below the row's text.
  static let inset = Spacing.xsPlus
  /// 1: how far a check's mark drops to centre on its name's first line (v3 `.chk .mk`'s margin).
  static let markDrop = Spacing.hairline
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: .zero) {
      FactRow(String("Account"), value: String("Atlas"))
      FactRow(String("Display"), value: String("1440 × 900"))
      FactRow(String("Ready to act"), value: String("All checks pass"), mark: .ready)
      FactRow(
        String("Mouse and keyboard"),
        value: String("Mouse and keyboard aren't ready. Recover starts them again."),
        mark: .notReady
      )
      FactRow(String("Save and Open dialogs"), value: String("Not checked yet"), mark: .unknown)
    }
    .frame(width: SampleWidth.narrow)
    .sampleSurface()
  }
#endif
