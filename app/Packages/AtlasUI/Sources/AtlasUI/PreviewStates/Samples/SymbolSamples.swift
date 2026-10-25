#if DEBUG
  import SwiftUI

  /// Every symbol Atlas draws, each at a row's size beside its name, in two columns.
  struct SymbolSamples: View {
    /// The grid's columns.
    private static let columns = [
      GridItem(.fixed(SampleWidth.narrow), alignment: .leading),
      GridItem(.fixed(SampleWidth.narrow), alignment: .leading),
    ]

    /// The symbols with their names.
    var body: some View {
      LazyVGrid(columns: Self.columns, alignment: .leading, spacing: Spacing.s) {
        ForEach(SymbolName.allCases, id: \.self) { symbol in
          HStack(spacing: Spacing.s) {
            Image(systemName: symbol.rawValue)
              .font(Typography.symbol(.control, weight: .medium))
              .frame(width: IconSize.m)
            Text(symbol.rawValue)
              .font(Typography.font(.callout, design: .monospaced))
              .foregroundStyle(Color.textSecondary)
          }
        }
      }
      .sampleSurface()
    }
  }
#endif
